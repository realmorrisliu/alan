use std::path::Path;

use alan_ap::{Fid, FileServer};
use alan_kernel::{ExecSpec, LiveNamespace, Pid};
use anyhow::{Context, Result, ensure};

use crate::BootUnit;

pub(super) fn resolve_child_connection(passed: &str, requested: Option<&str>) -> Result<String> {
    let requested = requested.unwrap_or(passed);
    ensure!(
        requested == passed,
        "Connection '{requested}' was not passed to the child Agent Process by the parent Process; available Connection is '{passed}'."
    );
    Ok(requested.to_string())
}

pub(super) fn validate_child_memory_mount(
    namespace: &alan_kernel::Namespace,
    delegated: bool,
    memory_path: Option<&str>,
) -> Result<()> {
    if !delegated && let Some(memory_path) = memory_path {
        let memory_path = Path::new(memory_path);
        ensure!(
            namespace.describe().into_iter().all(|(mount, _)| {
                let mount = Path::new(&mount);
                !mount.starts_with(memory_path) && !memory_path.starts_with(mount)
            }),
            "child Agent Process retained a Memory Store mount without the Memory handle"
        );
    }
    Ok(())
}

pub(super) fn project_boot_unit_namespace(
    base: &alan_kernel::Namespace,
    unit: &BootUnit,
) -> Result<alan_kernel::Namespace> {
    crate::runtime::embed_mounts::project_boot_unit_with_caller_mnt(base, unit)
}

pub(super) fn validate_process_cwd(namespace: &LiveNamespace, cwd: &str) -> Result<()> {
    ensure!(
        cwd == "/" || namespace.snapshot().resolve(cwd).is_ok(),
        "Agent Process cwd '{cwd}' is outside its namespace"
    );
    Ok(())
}

pub(super) async fn read_clone_pid(server: &impl FileServer, fid: Fid) -> Result<Pid> {
    let pid = String::from_utf8(server.read(fid, 0, 64).await?)?
        .parse::<u64>()
        .context("Agent Process clone PID is invalid")?;
    Ok(Pid(pid))
}

pub(super) async fn commit_clone(
    server: &impl FileServer,
    fid: Fid,
    exec: &ExecSpec,
) -> Result<()> {
    server
        .write(fid, 0, &serde_json::to_vec(exec)?)
        .await
        .context("write Agent Process exec spec")?;
    server
        .clunk(fid)
        .await
        .context("commit Agent Process through /proc/clone")
}
