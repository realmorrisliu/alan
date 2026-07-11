use std::path::{Path, PathBuf};

use crate::{InstallChannel, agent_root::AgentRootLayout};

/// Canonical alan home paths derived from a user home directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlanHomePaths {
    pub channel: InstallChannel,
    pub home_dir: PathBuf,
    pub alan_home_dir: PathBuf,
    pub global_agent_root_dir: PathBuf,
    pub global_named_agents_dir: PathBuf,
    pub global_public_skills_dir: PathBuf,
    pub global_agent_config_path: PathBuf,
    pub global_models_path: PathBuf,
    pub global_connections_path: PathBuf,
    pub global_credentials_dir: PathBuf,
    pub global_auth_path: PathBuf,
    pub global_registry_path: PathBuf,
}

impl AlanHomePaths {
    /// Resolve alan home paths from the current user's home directory.
    pub fn detect() -> Option<Self> {
        dirs::home_dir()
            .map(|home| Self::from_home_dir_for_channel(&home, InstallChannel::detect_current()))
    }

    /// Resolve stable-channel alan home paths from an explicit home directory.
    pub fn from_home_dir(home_dir: &Path) -> Self {
        Self::from_home_dir_for_channel(home_dir, InstallChannel::Stable)
    }

    /// Resolve alan home paths from an explicit home directory and install channel.
    pub fn from_home_dir_for_channel(home_dir: &Path, channel: InstallChannel) -> Self {
        let home_dir = home_dir.to_path_buf();
        let descriptor = channel.descriptor();
        let alan_home_dir = home_dir.join(descriptor.alan_home_dir_name);
        Self::from_explicit_alan_home_dir(channel, home_dir, alan_home_dir)
    }

    /// Resolve alan home paths from an explicit alan home directory.
    pub fn from_alan_home_dir(alan_home_dir: &Path) -> Self {
        let channel = install_channel_from_alan_home_dir(alan_home_dir);
        let home_dir = alan_home_dir
            .parent()
            .unwrap_or_else(|| Path::new("/"))
            .to_path_buf();
        Self::from_explicit_alan_home_dir(channel, home_dir, alan_home_dir.to_path_buf())
    }

    fn from_explicit_alan_home_dir(
        channel: InstallChannel,
        home_dir: PathBuf,
        alan_home_dir: PathBuf,
    ) -> Self {
        let layout = AgentRootLayout::new();
        let descriptor = channel.descriptor();
        let global_named_agents_dir = layout.agent_roots_dir_from_alan_dir(&alan_home_dir);
        let global_agent_root_dir = layout.default_root_dir_from_alan_dir(&alan_home_dir);
        let global_public_skills_dir = home_dir
            .join(descriptor.global_skills_parent_dir_name)
            .join("skills");
        Self {
            channel,
            home_dir: home_dir.clone(),
            alan_home_dir: alan_home_dir.clone(),
            global_agent_root_dir: global_agent_root_dir.clone(),
            global_named_agents_dir,
            global_public_skills_dir,
            global_agent_config_path: layout.agent_config_path(&global_agent_root_dir),
            global_models_path: alan_home_dir.join("models.toml"),
            global_connections_path: alan_home_dir.join("connections.toml"),
            global_credentials_dir: alan_home_dir.join("credentials"),
            global_auth_path: alan_home_dir.join("auth.json"),
            global_registry_path: alan_home_dir.join("registry.json"),
        }
    }
}

fn install_channel_from_alan_home_dir(alan_home_dir: &Path) -> InstallChannel {
    let name = alan_home_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if name == InstallChannel::Dev.descriptor().alan_home_dir_name {
        InstallChannel::Dev
    } else {
        InstallChannel::Stable
    }
}

#[cfg(test)]
mod tests {
    use super::AlanHomePaths;
    use crate::InstallChannel;
    use std::path::Path;

    #[test]
    fn test_from_home_dir_builds_expected_layout() {
        let paths = AlanHomePaths::from_home_dir(Path::new("/tmp/demo-home"));

        assert_eq!(paths.alan_home_dir, Path::new("/tmp/demo-home/.alan"));
        assert_eq!(
            paths.global_agent_root_dir,
            Path::new("/tmp/demo-home/.alan/agents/default")
        );
        assert_eq!(
            paths.global_named_agents_dir,
            Path::new("/tmp/demo-home/.alan/agents")
        );
        assert_eq!(
            paths.global_public_skills_dir,
            Path::new("/tmp/demo-home/.agents/skills")
        );
        assert_eq!(
            paths.global_agent_config_path,
            Path::new("/tmp/demo-home/.alan/agents/default/agent.toml")
        );
        assert_eq!(
            paths.global_models_path,
            Path::new("/tmp/demo-home/.alan/models.toml")
        );
        assert_eq!(
            paths.global_connections_path,
            Path::new("/tmp/demo-home/.alan/connections.toml")
        );
        assert_eq!(
            paths.global_credentials_dir,
            Path::new("/tmp/demo-home/.alan/credentials")
        );
        assert_eq!(
            paths.global_auth_path,
            Path::new("/tmp/demo-home/.alan/auth.json")
        );
        assert_eq!(
            paths.global_registry_path,
            Path::new("/tmp/demo-home/.alan/registry.json")
        );
    }

    #[test]
    fn test_from_home_dir_for_dev_channel_builds_isolated_layout() {
        let paths = AlanHomePaths::from_home_dir_for_channel(
            Path::new("/tmp/demo-home"),
            InstallChannel::Dev,
        );

        assert_eq!(paths.channel, InstallChannel::Dev);
        assert_eq!(paths.alan_home_dir, Path::new("/tmp/demo-home/.alan-dev"));
        assert_eq!(
            paths.global_agent_root_dir,
            Path::new("/tmp/demo-home/.alan-dev/agents/default")
        );
        assert_eq!(
            paths.global_public_skills_dir,
            Path::new("/tmp/demo-home/.agents-dev/skills")
        );
        assert_eq!(
            paths.global_connections_path,
            Path::new("/tmp/demo-home/.alan-dev/connections.toml")
        );
        assert_eq!(
            paths.global_credentials_dir,
            Path::new("/tmp/demo-home/.alan-dev/credentials")
        );
        assert_eq!(
            paths.global_auth_path,
            Path::new("/tmp/demo-home/.alan-dev/auth.json")
        );
    }

    #[test]
    fn test_from_alan_home_dir_infers_dev_channel() {
        let paths = AlanHomePaths::from_alan_home_dir(Path::new("/tmp/demo-home/.alan-dev"));

        assert_eq!(paths.channel, InstallChannel::Dev);
        assert_eq!(
            paths.global_public_skills_dir,
            Path::new("/tmp/demo-home/.agents-dev/skills")
        );
    }
}
