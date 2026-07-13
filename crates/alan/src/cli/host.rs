use std::path::{Path, PathBuf};

#[cfg(not(target_os = "macos"))]
use alan_agent_engine::INSTALL_CHANNEL_ENV;
use alan_agent_engine::InstallChannel;
use anyhow::{Context, Result};

pub async fn attach_or_start_host(
    channel: InstallChannel,
) -> Result<alan_os_host::AttachedNamespace> {
    let attachment = alan_os_host::LocalAttachment::detect(channel.descriptor().id)?;
    if let Ok(attached) = attachment.connect().await {
        return Ok(attached);
    }

    let executable = dedicated_host_executable(channel)?;
    let mut start = request_platform_host_start(channel, &executable)?;
    let mut launcher_status = None;
    let mut last_error = None;
    let ready = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            match attachment.connect().await {
                Ok(attached) => return Ok(attached),
                Err(error) => last_error = Some(error),
            }
            if launcher_status.is_none() {
                launcher_status = start.poll_status()?;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await;
    if let Ok(result) = ready {
        return result;
    }
    anyhow::bail!(
        "dedicated Alan OS Host did not become ready (launcher={launcher_status:?}): {}",
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "no attachment diagnostic".to_string())
    )
}

struct HostStartAttempt {
    child: Option<std::process::Child>,
    launcher_status: Option<std::process::ExitStatus>,
}

impl HostStartAttempt {
    fn poll_status(&mut self) -> Result<Option<std::process::ExitStatus>> {
        if let Some(child) = &mut self.child {
            return child
                .try_wait()
                .context("poll dedicated Alan OS Host process");
        }
        Ok(self.launcher_status)
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn os_host_launch_label(channel: InstallChannel) -> String {
    format!("{}.os-host", channel.descriptor().bundle_identifier)
}

#[cfg(target_os = "macos")]
fn request_platform_host_start(
    channel: InstallChannel,
    executable: &Path,
) -> Result<HostStartAttempt> {
    let label = os_host_launch_label(channel);
    let status = std::process::Command::new("/bin/launchctl")
        .arg("submit")
        .arg("-l")
        .arg(&label)
        .arg("-p")
        .arg(executable)
        .arg("-o")
        .arg("/dev/null")
        .arg("-e")
        .arg("/dev/null")
        .arg("--")
        .arg(executable)
        .status()
        .with_context(|| {
            format!(
                "request launchd start for dedicated Host {} ({label})",
                executable.display()
            )
        })?;
    Ok(HostStartAttempt {
        child: None,
        launcher_status: Some(status),
    })
}

#[cfg(not(target_os = "macos"))]
fn request_platform_host_start(
    channel: InstallChannel,
    executable: &Path,
) -> Result<HostStartAttempt> {
    use std::os::unix::process::CommandExt;

    let mut command = std::process::Command::new(executable);
    command
        .env(INSTALL_CHANNEL_ENV, channel.descriptor().id)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0);
    let child = command
        .spawn()
        .with_context(|| format!("start dedicated Host {}", executable.display()))?;
    Ok(HostStartAttempt {
        child: Some(child),
        launcher_status: None,
    })
}

fn dedicated_host_executable(channel: InstallChannel) -> Result<PathBuf> {
    let name = channel.descriptor().os_host_name;
    if let Ok(current) = std::env::current_exe()
        && let Some(sibling) = sibling_executable(&current, name)
    {
        return Ok(sibling);
    }
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    anyhow::bail!("dedicated Alan OS Host executable {name} was not found beside alan or on PATH")
}

pub(crate) fn sibling_executable(current: &Path, name: &str) -> Option<PathBuf> {
    let current = current
        .canonicalize()
        .unwrap_or_else(|_| current.to_owned());
    let sibling = current.parent()?.join(name);
    sibling.is_file().then_some(sibling)
}
