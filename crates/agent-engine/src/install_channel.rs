//! Install-channel identity used by runtime path resolution.

use std::ffi::OsStr;
use std::path::Path;

/// Environment variable used as an explicit debug override for install-channel
/// selection.
pub const INSTALL_CHANNEL_ENV: &str = "ALAN_INSTALL_CHANNEL";

/// Stable and local development install identities.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum InstallChannel {
    /// Public stable Alan install.
    #[default]
    Stable,
    /// Local-only Alan Dev install.
    Dev,
}

/// Host-facing install-channel values that must remain stable across packaging,
/// launcher, and runtime path-resolution code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstallChannelDescriptor {
    /// Channel id used by scripts and runtime selection.
    pub id: &'static str,
    /// App bundle directory name.
    pub app_bundle_name: &'static str,
    /// Human-readable app name.
    pub display_name: &'static str,
    /// macOS bundle identifier.
    pub bundle_identifier: &'static str,
    /// CLI executable/link name.
    pub cli_name: &'static str,
    /// Channel root under the user's home directory.
    pub alan_home: &'static str,
    /// Alan home directory name, relative to the user's home directory.
    pub alan_home_dir_name: &'static str,
    /// Global public skill install root.
    pub global_skills_dir: &'static str,
    /// Global public skill parent directory name, relative to the user's home
    /// directory.
    pub global_skills_parent_dir_name: &'static str,
    /// Shell-control namespace.
    pub shell_control_namespace: &'static str,
}

impl InstallChannel {
    /// Resolve an install channel from its script-facing id.
    pub fn from_id(id: &str) -> Option<Self> {
        match id.trim() {
            "stable" => Some(Self::Stable),
            "dev" => Some(Self::Dev),
            _ => None,
        }
    }

    /// Detect the current channel from the explicit debug environment override
    /// and executable name. Unknown overrides are ignored so embedding hosts do
    /// not fail before logging is configured.
    pub fn detect_current() -> Self {
        let env_override = std::env::var(INSTALL_CHANNEL_ENV).ok();
        let argv0 = std::env::args_os().next();
        let argv0_name = argv0
            .as_deref()
            .and_then(executable_name)
            .and_then(OsStr::to_str);
        Self::detect_from_env_and_executable(env_override.as_deref(), argv0_name)
    }

    /// Detect a channel from explicit inputs. The environment override wins when
    /// it names a known channel; otherwise the executable name drives selection.
    pub fn detect_from_env_and_executable(
        env_override: Option<&str>,
        executable_name: Option<&str>,
    ) -> Self {
        if let Some(channel) = env_override.and_then(Self::from_id) {
            return channel;
        }

        match executable_name.and_then(Self::from_executable_name) {
            Some(channel) => channel,
            None => Self::Stable,
        }
    }

    /// Infer the channel from a command name.
    pub fn from_executable_name(name: &str) -> Option<Self> {
        let name = name
            .rsplit(std::path::MAIN_SEPARATOR)
            .next()
            .unwrap_or(name);
        let name = name.strip_suffix(".exe").unwrap_or(name);
        match name {
            "alan" => Some(Self::Stable),
            "alan-dev" => Some(Self::Dev),
            _ => None,
        }
    }

    /// Return the descriptor for this install channel.
    pub const fn descriptor(self) -> InstallChannelDescriptor {
        match self {
            Self::Stable => InstallChannelDescriptor {
                id: "stable",
                app_bundle_name: "Alan.app",
                display_name: "Alan",
                bundle_identifier: "app.alanworks.macos",
                cli_name: "alan",
                alan_home: "~/.alan",
                alan_home_dir_name: ".alan",
                global_skills_dir: "~/.agents/skills",
                global_skills_parent_dir_name: ".agents",
                shell_control_namespace: "alan-shell-control",
            },
            Self::Dev => InstallChannelDescriptor {
                id: "dev",
                app_bundle_name: "Alan Dev.app",
                display_name: "Alan Dev",
                bundle_identifier: "app.alanworks.macos.dev",
                cli_name: "alan-dev",
                alan_home: "~/.alan-dev",
                alan_home_dir_name: ".alan-dev",
                global_skills_dir: "~/.agents-dev/skills",
                global_skills_parent_dir_name: ".agents-dev",
                shell_control_namespace: "alan-dev-shell-control",
            },
        }
    }
}

fn executable_name(path: &OsStr) -> Option<&OsStr> {
    Path::new(path).file_name()
}

#[cfg(test)]
mod tests {
    use super::{InstallChannel, InstallChannelDescriptor};

    #[test]
    fn stable_descriptor_preserves_public_identity() {
        assert_eq!(
            InstallChannel::Stable.descriptor(),
            InstallChannelDescriptor {
                id: "stable",
                app_bundle_name: "Alan.app",
                display_name: "Alan",
                bundle_identifier: "app.alanworks.macos",
                cli_name: "alan",
                alan_home: "~/.alan",
                alan_home_dir_name: ".alan",
                global_skills_dir: "~/.agents/skills",
                global_skills_parent_dir_name: ".agents",
                shell_control_namespace: "alan-shell-control",
            }
        );
    }

    #[test]
    fn dev_descriptor_uses_isolated_local_identity() {
        assert_eq!(
            InstallChannel::Dev.descriptor(),
            InstallChannelDescriptor {
                id: "dev",
                app_bundle_name: "Alan Dev.app",
                display_name: "Alan Dev",
                bundle_identifier: "app.alanworks.macos.dev",
                cli_name: "alan-dev",
                alan_home: "~/.alan-dev",
                alan_home_dir_name: ".alan-dev",
                global_skills_dir: "~/.agents-dev/skills",
                global_skills_parent_dir_name: ".agents-dev",
                shell_control_namespace: "alan-dev-shell-control",
            }
        );
    }

    #[test]
    fn ids_resolve_to_known_channels_only() {
        assert_eq!(
            InstallChannel::from_id("stable"),
            Some(InstallChannel::Stable)
        );
        assert_eq!(InstallChannel::from_id("dev"), Some(InstallChannel::Dev));
        assert_eq!(InstallChannel::from_id("nightly"), None);
    }

    #[test]
    fn executable_name_selects_dev_channel() {
        assert_eq!(
            InstallChannel::from_executable_name("alan-dev"),
            Some(InstallChannel::Dev)
        );
        assert_eq!(
            InstallChannel::from_executable_name("/Applications/Alan Dev.app/alan-dev"),
            Some(InstallChannel::Dev)
        );
    }

    #[test]
    fn env_override_wins_over_executable_name() {
        assert_eq!(
            InstallChannel::detect_from_env_and_executable(Some("dev"), Some("alan")),
            InstallChannel::Dev
        );
        assert_eq!(
            InstallChannel::detect_from_env_and_executable(Some("stable"), Some("alan-dev")),
            InstallChannel::Stable
        );
    }

    #[test]
    fn executable_name_falls_back_to_stable() {
        assert_eq!(
            InstallChannel::detect_from_env_and_executable(None, Some("alan-dev")),
            InstallChannel::Dev
        );
        assert_eq!(
            InstallChannel::detect_from_env_and_executable(None, Some("unknown")),
            InstallChannel::Stable
        );
    }
}
