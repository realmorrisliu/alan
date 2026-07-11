pub mod connection;
pub mod init;
pub mod shell;
pub mod skill_authoring;
pub mod skill_eval;
pub mod skills;
pub mod workspace;

pub(crate) fn load_agent_config_metadata_with_notice()
-> anyhow::Result<alan_agent_engine::LoadedConfig> {
    alan_agent_engine::Config::load_with_metadata()
}
