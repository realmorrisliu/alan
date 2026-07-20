use alan_agent_engine::Config;
use alan_agent_engine::tools::ToolContext;
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) fn tool_context_with_root(
    root: PathBuf,
    scratch_dir: PathBuf,
    config: Arc<Config>,
) -> ToolContext {
    ToolContext::from_binding(
        crate::execution_adapter::standalone_binding(root, scratch_dir),
        config,
    )
}
