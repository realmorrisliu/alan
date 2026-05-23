//! Install-channel identity descriptors for host-facing packaging surfaces.

/// Stable and local development install identities.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallChannel {
    /// Public stable Alan install.
    Stable,
    /// Local-only Alan Dev install.
    Dev,
}

/// Host-facing install-channel values that must remain stable across packaging,
/// launcher, and runtime path-resolution code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstallChannelDescriptor {
    /// Channel id used by scripts and future runtime selection.
    pub id: &'static str,
    /// App bundle directory name.
    pub app_bundle_name: &'static str,
    /// Human-readable app name.
    pub display_name: &'static str,
    /// macOS bundle identifier.
    pub bundle_identifier: &'static str,
    /// CLI executable/link name.
    pub cli_name: &'static str,
    /// TUI executable/link name.
    pub tui_name: &'static str,
    /// Channel root under the user's home directory.
    pub alan_home: &'static str,
    /// Global public skill install root.
    pub global_skills_dir: &'static str,
    /// Default daemon bind address.
    pub daemon_bind: &'static str,
    /// Default daemon client URL.
    pub daemon_url: &'static str,
    /// Shell-control namespace.
    pub shell_control_namespace: &'static str,
}

impl InstallChannel {
    /// Resolve an install channel from its script-facing id.
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "stable" => Some(Self::Stable),
            "dev" => Some(Self::Dev),
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
                tui_name: "alan-tui",
                alan_home: "~/.alan",
                global_skills_dir: "~/.agents/skills",
                daemon_bind: "0.0.0.0:8090",
                daemon_url: "http://127.0.0.1:8090",
                shell_control_namespace: "alan-shell-control",
            },
            Self::Dev => InstallChannelDescriptor {
                id: "dev",
                app_bundle_name: "Alan Dev.app",
                display_name: "Alan Dev",
                bundle_identifier: "app.alanworks.macos.dev",
                cli_name: "alan-dev",
                tui_name: "alan-dev-tui",
                alan_home: "~/.alan-dev",
                global_skills_dir: "~/.agents-dev/skills",
                daemon_bind: "127.0.0.1:8091",
                daemon_url: "http://127.0.0.1:8091",
                shell_control_namespace: "alan-dev-shell-control",
            },
        }
    }
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
                tui_name: "alan-tui",
                alan_home: "~/.alan",
                global_skills_dir: "~/.agents/skills",
                daemon_bind: "0.0.0.0:8090",
                daemon_url: "http://127.0.0.1:8090",
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
                tui_name: "alan-dev-tui",
                alan_home: "~/.alan-dev",
                global_skills_dir: "~/.agents-dev/skills",
                daemon_bind: "127.0.0.1:8091",
                daemon_url: "http://127.0.0.1:8091",
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
}
