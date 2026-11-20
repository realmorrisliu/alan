//! Caller `/mnt/<name>` validation and capability-preserving projection onto Boot Units.

use alan_kernel::{Access, Namespace};
use anyhow::{Result, ensure};

use crate::BootUnit;

/// Mount segments under `/mnt/` owned by Alan OS system services and reserved
/// against caller-supplied overrides.
pub(crate) const RESERVED_MNT_ROOTS: &[&str] = &[
    "connections",
    "host-mount",
    "llm",
    "mem",
    "package",
    "route",
    "service-manager",
];

fn reserved_mnt_root(path: &str) -> Option<&'static str> {
    if path == "/mnt" {
        return Some("");
    }
    let suffix = path.strip_prefix("/mnt/")?;
    let segment = suffix.split('/').next()?;
    RESERVED_MNT_ROOTS
        .iter()
        .copied()
        .find(|reserved| *reserved == segment)
}

/// Extract caller-supplied `/mnt/<name>` mounts while rejecting any reserved
/// Alan OS mount root (`connections`, `host-mount`, `llm`, `mem`, `package`,
/// `route`, `service-manager`, or `/mnt` itself).
pub(crate) fn caller_mnt_mounts(namespace: &Namespace) -> Result<Vec<(String, Access)>> {
    let mut mounts = Vec::new();
    for (path, access) in namespace.describe() {
        if path == "/mnt" {
            anyhow::bail!("caller cannot supply `/mnt` directly; use `/mnt/<name>`");
        }
        if let Some(reserved) = reserved_mnt_root(&path) {
            anyhow::bail!("caller cannot supply reserved `/mnt/{reserved}` mount `{path}`");
        }
        if path.starts_with("/mnt/") {
            mounts.push((path, access));
        }
    }
    Ok(mounts)
}

/// Extract non-reserved `/mnt/<name>` mounts from a namespace without failing on
/// Alan OS internal service mounts already installed in `system_namespace`.
pub(crate) fn non_reserved_mnt_mounts(namespace: &Namespace) -> Vec<(String, Access)> {
    let mut mounts = Vec::new();
    for (path, access) in namespace.describe() {
        if path.starts_with("/mnt/") && reserved_mnt_root(&path).is_none() {
            mounts.push((path, access));
        }
    }
    mounts
}

/// Project `unit` mounts from `base`, re-projecting caller non-reserved
/// `/mnt/<name>` mounts after `/mnt` (`ReadOnly`) so caller-requested `ReadWrite`
/// access is preserved without violating monotonic `project_mounts`.
pub(crate) fn project_boot_unit_with_caller_mnt(
    base: &Namespace,
    unit: &BootUnit,
) -> Result<Namespace> {
    let caller_mnt = non_reserved_mnt_mounts(base);
    let extra_mnt: Vec<(&str, &str, Access)> = caller_mnt
        .iter()
        .filter(|(path, _)| !unit.mounts.iter().any(|mount| mount.path == *path))
        .map(|(path, access)| (path.as_str(), path.as_str(), *access))
        .collect();
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
        .chain(extra_mnt);
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
    Ok(namespace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_ap::InProcessTransport;
    use std::sync::Arc;

    #[test]
    fn caller_mnt_mounts_rejects_reserved_roots() {
        for reserved in [
            "/mnt",
            "/mnt/connections",
            "/mnt/host-mount",
            "/mnt/llm",
            "/mnt/mem",
            "/mnt/package",
            "/mnt/route",
            "/mnt/service-manager",
            "/mnt/llm/nested",
        ] {
            let mut ns = Namespace::new();
            ns.mount(
                reserved,
                InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::writable())),
                Access::ReadWrite,
            );
            assert!(
                caller_mnt_mounts(&ns).is_err(),
                "expected `{reserved}` to be rejected"
            );
        }
    }

    #[test]
    fn project_boot_unit_preserves_caller_read_write_without_clamping() {
        let mut base = Namespace::new();
        base.mount(
            "/mnt",
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
            Access::ReadOnly,
        );
        base.mount(
            "/mnt/my-app",
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::writable())),
            Access::ReadWrite,
        );
        base.mount(
            "/mnt/ro-app",
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
            Access::ReadOnly,
        );
        let unit = BootUnit::parse(
            r#"
name = "root-agent"
executable = "/bin/alan-agent"
after = []
required = true
timeout_ms = 1000
restart = "on-failure"
restart_limit = 3
initial_backoff_ms = 10
max_backoff_ms = 100
stable_reset_ms = 1000
published_handles = []
[[mounts]]
path = "/mnt"
source = "/mnt"
access = "read"
"#,
        )
        .unwrap();
        let projected = project_boot_unit_with_caller_mnt(&base, &unit).unwrap();
        assert_eq!(projected.resolve("/mnt").unwrap().access, Access::ReadOnly);
        assert_eq!(
            projected.resolve("/mnt/my-app").unwrap().access,
            Access::ReadWrite
        );
        assert_eq!(
            projected.resolve("/mnt/ro-app").unwrap().access,
            Access::ReadOnly
        );
    }
}
