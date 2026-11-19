//! Rollout resume & explicit trust posture validation helpers for Service Manager.

use anyhow::ensure;

use super::ServiceManagerConfig;

/// Enforce explicit invocation-only rollout recovery and trust posture invariants.
pub(super) fn validate_boot_trust_and_resume(config: &ServiceManagerConfig) -> anyhow::Result<()> {
    ensure!(
        !config.resume_root || config.process.store_bindings.is_some(),
        "Root Agent recovery requires durable store bindings"
    );
    ensure!(
        !(config.resume_root && config.ephemeral_rollout),
        "an ephemeral Root rollout cannot also resume the selected Root rollout"
    );
    ensure!(
        config.resume_rollout_id.is_none()
            || (config.resume_root && config.process.store_bindings.is_some()),
        "explicit Root rollout id recovery requires `--resume` and durable store bindings"
    );
    ensure!(
        !(config.resume_rollout_id.is_some() && config.ephemeral_rollout),
        "an ephemeral Root rollout cannot resume an explicit rollout id"
    );
    if let Some(rollout_id) = config.resume_rollout_id.as_deref() {
        ensure!(
            !rollout_id.trim().is_empty(),
            "explicit Root rollout id must not be empty"
        );
    }
    Ok(())
}
