use alan_runtime::{AlanHomePaths, InstallChannel};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct HostConfig {
    pub bind_address: String,
    pub daemon_url: String,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self::default_for_channel(InstallChannel::Stable)
    }
}

#[derive(Debug, Deserialize)]
struct RawHostConfig {
    #[serde(default = "default_bind_address")]
    bind_address: String,
    #[serde(default)]
    daemon_url: Option<String>,
}

impl HostConfig {
    pub fn load() -> Result<Self> {
        let channel = InstallChannel::detect_current();
        Self::load_with_path_for_channel(channel, Self::host_file_path())
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).with_context(|| {
            format!("failed to read host configuration file {}", path.display())
        })?;
        let raw: RawHostConfig = toml::from_str(&content).with_context(|| {
            format!("failed to parse host configuration file {}", path.display())
        })?;
        Ok(Self::from_raw(raw))
    }

    pub fn host_file_path() -> Option<PathBuf> {
        AlanHomePaths::detect().map(|paths| paths.global_host_config_path)
    }

    #[cfg(test)]
    pub fn host_file_path_from_home(home: &Path) -> Option<PathBuf> {
        Some(AlanHomePaths::from_home_dir(home).global_host_config_path)
    }

    #[cfg(test)]
    pub fn host_file_path_from_home_for_channel(
        home: &Path,
        channel: InstallChannel,
    ) -> Option<PathBuf> {
        Some(AlanHomePaths::from_home_dir_for_channel(home, channel).global_host_config_path)
    }

    pub fn resolve_bind_address() -> Result<String> {
        Self::resolve_bind_address_from(std::env::var("BIND_ADDRESS").ok(), Self::load())
    }

    pub fn resolve_bind_address_best_effort() -> String {
        Self::resolve_bind_address_best_effort_from(
            std::env::var("BIND_ADDRESS").ok(),
            Self::load(),
        )
    }

    pub fn resolve_daemon_url_best_effort() -> String {
        Self::resolve_daemon_url_best_effort_from(daemon_url_env_override(), Self::load())
    }

    pub(crate) fn local_daemon_url_for_bind_address(bind_address: &str) -> String {
        let port = bind_address
            .rsplit(':')
            .next()
            .and_then(|raw| raw.parse::<u16>().ok())
            .unwrap_or(8090);
        format!("http://127.0.0.1:{port}")
    }

    pub(crate) fn default_for_channel(channel: InstallChannel) -> Self {
        let descriptor = channel.descriptor();
        Self {
            bind_address: descriptor.daemon_bind.to_string(),
            daemon_url: descriptor.daemon_url.to_string(),
        }
    }

    fn load_with_path_for_channel(channel: InstallChannel, path: Option<PathBuf>) -> Result<Self> {
        if let Some(path) = path
            && path.exists()
        {
            return Self::from_file(&path);
        }

        Ok(Self::default_for_channel(channel))
    }

    fn from_raw(raw: RawHostConfig) -> Self {
        let bind_address = raw.bind_address;
        let daemon_url = raw
            .daemon_url
            .unwrap_or_else(|| Self::local_daemon_url_for_bind_address(&bind_address));
        Self {
            bind_address,
            daemon_url,
        }
    }

    fn resolve_bind_address_from(
        env_override: Option<String>,
        config: Result<Self>,
    ) -> Result<String> {
        match env_override {
            Some(bind_address) => Ok(bind_address),
            None => config.map(|config| config.bind_address),
        }
    }

    fn resolve_bind_address_best_effort_from(
        env_override: Option<String>,
        config: Result<Self>,
    ) -> String {
        Self::resolve_bind_address_from(env_override, config)
            .unwrap_or_else(|_| default_bind_address())
    }

    fn resolve_daemon_url_best_effort_from(
        env_override: Option<String>,
        config: Result<Self>,
    ) -> String {
        match env_override {
            Some(daemon_url) => daemon_url,
            None => config.map(|config| config.daemon_url).unwrap_or_else(|_| {
                default_daemon_url_for_channel(InstallChannel::detect_current())
            }),
        }
    }
}

pub(crate) fn daemon_url_env_override() -> Option<String> {
    normalize_env_override(std::env::var("ALAN_AGENTD_URL").ok())
}

fn default_bind_address() -> String {
    InstallChannel::detect_current()
        .descriptor()
        .daemon_bind
        .to_string()
}

fn default_daemon_url_for_channel(channel: InstallChannel) -> String {
    channel.descriptor().daemon_url.to_string()
}

fn normalize_env_override(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{HostConfig, normalize_env_override};
    use alan_runtime::InstallChannel;
    use anyhow::anyhow;
    use tempfile::TempDir;

    #[test]
    fn test_host_file_path_from_home_uses_alan_home_root() {
        let path = HostConfig::host_file_path_from_home(std::path::Path::new("/tmp/demo")).unwrap();
        assert_eq!(path, std::path::Path::new("/tmp/demo/.alan/host.toml"));
    }

    #[test]
    fn test_host_file_path_from_home_uses_dev_alan_home_root() {
        let path = HostConfig::host_file_path_from_home_for_channel(
            std::path::Path::new("/tmp/demo"),
            InstallChannel::Dev,
        )
        .unwrap();
        assert_eq!(path, std::path::Path::new("/tmp/demo/.alan-dev/host.toml"));
    }

    #[test]
    fn test_host_config_from_file() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("host.toml");
        std::fs::write(
            &path,
            r#"
bind_address = "127.0.0.1:9000"
daemon_url = "http://127.0.0.1:9000"
"#,
        )
        .unwrap();

        let config = HostConfig::from_file(&path).unwrap();
        assert_eq!(config.bind_address, "127.0.0.1:9000");
        assert_eq!(config.daemon_url, "http://127.0.0.1:9000");
    }

    #[test]
    fn test_host_config_from_file_derives_daemon_url_from_bind_address() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("host.toml");
        std::fs::write(&path, "bind_address = \"127.0.0.1:9123\"\n").unwrap();

        let config = HostConfig::from_file(&path).unwrap();
        assert_eq!(config.bind_address, "127.0.0.1:9123");
        assert_eq!(config.daemon_url, "http://127.0.0.1:9123");
    }

    #[test]
    fn test_host_config_defaults_when_file_missing() {
        let config = HostConfig::default();
        assert_eq!(config.bind_address, "0.0.0.0:8090");
        assert_eq!(config.daemon_url, "http://127.0.0.1:8090");
    }

    #[test]
    fn test_host_config_dev_defaults_when_file_missing() {
        let config = HostConfig::default_for_channel(InstallChannel::Dev);
        assert_eq!(config.bind_address, "127.0.0.1:8091");
        assert_eq!(config.daemon_url, "http://127.0.0.1:8091");
    }

    #[test]
    fn test_resolve_daemon_url_best_effort_prefers_env_on_load_error() {
        let resolved = HostConfig::resolve_daemon_url_best_effort_from(
            Some("http://127.0.0.1:9999".to_string()),
            Err(anyhow!("broken host config")),
        );
        assert_eq!(resolved, "http://127.0.0.1:9999");
    }

    #[test]
    fn test_resolve_daemon_url_best_effort_treats_blank_env_override_as_unset() {
        let resolved = HostConfig::resolve_daemon_url_best_effort_from(
            normalize_env_override(Some("   ".to_string())),
            Err(anyhow!("broken host config")),
        );
        assert_eq!(resolved, "http://127.0.0.1:8090");
    }

    #[test]
    fn test_resolve_bind_address_best_effort_prefers_env_on_load_error() {
        let resolved = HostConfig::resolve_bind_address_best_effort_from(
            Some("127.0.0.1:9999".to_string()),
            Err(anyhow!("broken host config")),
        );
        assert_eq!(resolved, "127.0.0.1:9999");
    }

    #[test]
    fn test_resolve_bind_address_prefers_env_before_load() {
        let resolved = HostConfig::resolve_bind_address_from(
            Some("127.0.0.1:9999".to_string()),
            Err(anyhow!("broken host config")),
        )
        .unwrap();
        assert_eq!(resolved, "127.0.0.1:9999");
    }

    #[test]
    fn test_normalize_env_override_trims_non_empty_values() {
        assert_eq!(
            normalize_env_override(Some(" http://example.test:8090/ws ".to_string())),
            Some("http://example.test:8090/ws".to_string())
        );
    }

    #[test]
    fn test_normalize_env_override_drops_blank_values() {
        assert_eq!(normalize_env_override(Some(" \t ".to_string())), None);
        assert_eq!(normalize_env_override(None), None);
    }
}
