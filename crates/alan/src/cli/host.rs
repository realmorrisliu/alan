use std::path::PathBuf;

use alan_agent_engine::InstallChannel;
use anyhow::{Context, Result};

/// Explicit runtime directory of a live Alan instance selected by its native caller.
pub const INSTANCE_RUNTIME_DIR_ENV: &str = "ALAN_INSTANCE_RUNTIME_DIR";

pub fn explicit_instance_paths(channel: InstallChannel) -> Result<alan_os_host::HostEndpointPaths> {
    let runtime = std::env::var_os(INSTANCE_RUNTIME_DIR_ENV)
        .context("select a live Alan instance with ALAN_INSTANCE_RUNTIME_DIR")?;
    alan_os_host::HostEndpointPaths::from_runtime_dir(
        &PathBuf::from(runtime),
        channel.descriptor().id,
    )
}
