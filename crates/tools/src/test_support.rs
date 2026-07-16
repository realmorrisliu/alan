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
        alan_agent_engine::tools::ToolExecutionBinding::new(root.clone(), scratch_dir)
            .with_sandbox_spec(alan_agent_engine::tools::SandboxSpec::seed(root)),
        config,
    )
}
