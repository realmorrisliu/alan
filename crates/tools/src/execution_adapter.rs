use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use alan_agent_engine::tools::{Sandbox, SandboxSpec, ToolExecutionAdapter, ToolExecutionBinding};
use anyhow::{Context, Result};

const STANDALONE_NAMESPACE_ROOT: &str = "/mnt/source";

/// Single-root adapter used only by standalone Tool registries and Tool tests.
///
/// Product execution receives an adapter from Host Mount Service instead.
#[derive(Debug)]
struct StandaloneToolAdapter {
    host_root: PathBuf,
    namespace_cwd: PathBuf,
}

impl StandaloneToolAdapter {
    fn new(host_root: PathBuf, namespace_cwd: PathBuf) -> Self {
        Self {
            host_root,
            namespace_cwd,
        }
    }

    fn visible(&self, namespace_cwd: &Path, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            namespace_cwd.join(path)
        }
    }
}

impl ToolExecutionAdapter for StandaloneToolAdapter {
    fn namespace_cwd(&self) -> PathBuf {
        self.namespace_cwd.clone()
    }

    fn cwd(&self) -> Result<PathBuf> {
        self.resolve_path(&self.namespace_cwd, Path::new("."))
    }

    fn resolve_path(&self, namespace_cwd: &Path, path: &Path) -> Result<PathBuf> {
        let visible = self.visible(namespace_cwd, path);
        let suffix = visible
            .strip_prefix(STANDALONE_NAMESPACE_ROOT)
            .with_context(|| format!("path {} is outside the Tool namespace", visible.display()))?;
        Ok(self.host_root.join(suffix))
    }

    fn visible_path(&self, host_path: &Path) -> PathBuf {
        host_path.strip_prefix(&self.host_root).map_or_else(
            |_| PathBuf::from("<unmapped-host-path>"),
            |suffix| Path::new(STANDALONE_NAMESPACE_ROOT).join(suffix),
        )
    }

    fn project_text(&self, text: &str) -> String {
        text.replace(
            self.host_root.to_string_lossy().as_ref(),
            STANDALONE_NAMESPACE_ROOT,
        )
    }

    fn sandbox(&self) -> Result<Sandbox> {
        Ok(Sandbox::from_spec(SandboxSpec::seed(
            self.host_root.clone(),
        )))
    }
}

pub(crate) fn standalone_binding(host_root: PathBuf, scratch_dir: PathBuf) -> ToolExecutionBinding {
    standalone_binding_at(
        host_root,
        PathBuf::from(STANDALONE_NAMESPACE_ROOT),
        scratch_dir,
    )
}

pub(crate) fn standalone_binding_at(
    host_root: PathBuf,
    namespace_cwd: PathBuf,
    scratch_dir: PathBuf,
) -> ToolExecutionBinding {
    ToolExecutionBinding::awaiting_host_projection(namespace_cwd.clone(), scratch_dir).with_adapter(
        Arc::new(StandaloneToolAdapter::new(host_root, namespace_cwd)),
    )
}
