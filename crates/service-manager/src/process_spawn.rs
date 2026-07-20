//! Shared `/proc/clone` helpers for Service Manager and its system services.

use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};

use alan_ap::{Fid, FileServer, OpenMode};
use alan_kernel::{Access, Credentials, LiveNamespace, Pid};
use anyhow::{Context, Result, ensure};

use crate::BootUnit;

static NEXT_BOOT_FID: AtomicU64 = AtomicU64::new(800_000);

pub(crate) async fn spawn_process(
    procfs: &alan_kernel::ProcFs,
    parent: Option<Pid>,
    namespace: LiveNamespace,
    credentials: Credentials,
    executable: &str,
) -> Result<Pid> {
    spawn_process_with_descriptors(
        procfs,
        parent,
        namespace,
        credentials,
        executable,
        BTreeMap::new(),
    )
    .await
}

pub(crate) async fn spawn_unit_process(
    procfs: &alan_kernel::ProcFs,
    parent: Pid,
    system_namespace: &LiveNamespace,
    credentials: Credentials,
    unit: &BootUnit,
    extra_mounts: &[(String, Access)],
) -> Result<(Pid, LiveNamespace)> {
    let base = system_namespace.snapshot();
    let declarations = unit
        .mounts
        .iter()
        .map(|mount| {
            (
                mount.path.as_str(),
                mount.source.as_str(),
                match mount.access {
                    crate::MountAccess::Read => Access::ReadOnly,
                    crate::MountAccess::Write => Access::ReadWrite,
                },
            )
        })
        .chain(
            extra_mounts
                .iter()
                .map(|(path, access)| (path.as_str(), path.as_str(), *access)),
        );
    let namespace = base.project_mounts(declarations).map_err(|_| {
        anyhow::anyhow!(
            "Boot Unit `{}` requests an unavailable mount projection",
            unit.name
        )
    })?;
    for descriptor in &unit.descriptors {
        ensure!(
            namespace.resolve(&descriptor.path).is_ok(),
            "Boot Unit `{}` descriptor {} is outside its namespace",
            unit.name,
            descriptor.number
        );
    }
    let descriptors = unit
        .descriptors
        .iter()
        .map(|descriptor| (descriptor.number, descriptor.path.clone()))
        .collect();
    let live_namespace = LiveNamespace::new(namespace);
    let pid = spawn_process_with_descriptors(
        procfs,
        Some(parent),
        live_namespace.clone(),
        credentials,
        &unit.executable,
        descriptors,
    )
    .await?;
    Ok((pid, live_namespace))
}

async fn spawn_process_with_descriptors(
    procfs: &alan_kernel::ProcFs,
    parent: Option<Pid>,
    namespace: LiveNamespace,
    credentials: Credentials,
    executable: &str,
    descriptors: BTreeMap<u32, String>,
) -> Result<Pid> {
    let spawner = procfs.for_live_spawner(parent, namespace.clone(), credentials);
    let fid = Fid(NEXT_BOOT_FID.fetch_add(1, Ordering::Relaxed));
    spawner
        .walk(Fid::ROOT, fid, &["clone".to_string()])
        .await
        .with_context(|| format!("walk {executable} /proc/clone"))?;
    spawner
        .open(fid, OpenMode::ReadWrite)
        .await
        .with_context(|| format!("open {executable} /proc/clone"))?;
    let pid = String::from_utf8(spawner.read(fid, 0, 64).await?)
        .with_context(|| format!("{executable} PID is not UTF-8"))?
        .parse::<u64>()
        .with_context(|| format!("{executable} PID is invalid"))?;
    let exec = alan_kernel::ExecSpec {
        executable: executable.to_string(),
        args: Vec::new(),
        namespace: alan_kernel::ExecNamespaceManifest::from_namespace(&namespace.snapshot()),
        descriptors,
    };
    spawner
        .write(fid, 0, &serde_json::to_vec(&exec)?)
        .await
        .with_context(|| format!("write {executable} exec spec"))?;
    spawner
        .clunk(fid)
        .await
        .with_context(|| format!("commit {executable} Process"))?;
    Ok(Pid(pid))
}
