pub mod connection;
pub mod daemon;
pub mod emacs;
pub mod init;
pub mod shell;
pub mod skill_authoring;
pub mod skill_eval;
pub mod skills;
pub mod workspace;

pub(crate) fn load_agent_config_metadata_with_notice() -> anyhow::Result<alan_runtime::LoadedConfig>
{
    alan_runtime::Config::load_with_metadata()
}

pub(crate) fn load_agent_config_with_notice() -> anyhow::Result<alan_runtime::Config> {
    Ok(load_agent_config_metadata_with_notice()?.into_config())
}
