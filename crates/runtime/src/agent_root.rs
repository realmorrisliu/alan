use crate::{InstallChannel, paths::AlanHomePaths};
use std::{
    ffi::OsStr,
    path::{Component, Path, PathBuf},
};

pub const DEFAULT_AGENT_NAME: &str = "default";
const ALAN_DIRNAME: &str = ".alan";
const AGENTS_DIRNAME: &str = "agents";
const AGENT_CONFIG_FILENAME: &str = "agent.toml";
const PERSONA_DIRNAME: &str = "persona";
const SKILLS_DIRNAME: &str = "skills";
const POLICY_FILENAME: &str = "policy.yaml";
const RUNTIME_DIRNAME: &str = "runtime";
const SESSIONS_DIRNAME: &str = "sessions";
const MEMORY_DIRNAME: &str = "memory";

/// Canonical path contract for alan agent roots.
///
/// `AgentRootLayout` is the runtime-owned source of truth for agent definition
/// roots and their standard assets. Host crates should use these semantic
/// helpers instead of joining `.alan/agents/default` manually.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AgentRootLayout;

impl AgentRootLayout {
    pub const fn new() -> Self {
        Self
    }

    pub fn global_default_root(&self, home_paths: &AlanHomePaths) -> AgentRootPaths {
        AgentRootPaths::new(
            AgentRootKind::GlobalDefault,
            self.global_default_root_dir(home_paths),
        )
    }

    pub fn global_default_root_dir(&self, home_paths: &AlanHomePaths) -> PathBuf {
        self.default_root_dir_from_alan_dir(&home_paths.alan_home_dir)
    }

    pub fn global_named_agents_dir(&self, home_paths: &AlanHomePaths) -> PathBuf {
        self.agent_roots_dir_from_alan_dir(&home_paths.alan_home_dir)
    }

    pub fn global_named_root(&self, home_paths: &AlanHomePaths, name: &str) -> AgentRootPaths {
        AgentRootPaths::new(
            AgentRootKind::GlobalNamed(name.to_string()),
            self.global_named_root_dir(home_paths, name),
        )
    }

    pub fn global_named_root_dir(&self, home_paths: &AlanHomePaths, name: &str) -> PathBuf {
        self.global_named_agents_dir(home_paths).join(name)
    }

    pub fn workspace_default_root(&self, workspace_root: &Path) -> AgentRootPaths {
        AgentRootPaths::new(
            AgentRootKind::WorkspaceDefault,
            self.workspace_default_root_dir(workspace_root),
        )
    }

    pub fn workspace_default_root_from_alan_dir(&self, alan_dir: &Path) -> AgentRootPaths {
        AgentRootPaths::new(
            AgentRootKind::WorkspaceDefault,
            self.default_root_dir_from_alan_dir(alan_dir),
        )
    }

    pub fn workspace_default_root_dir(&self, workspace_root: &Path) -> PathBuf {
        self.default_root_dir_from_alan_dir(&workspace_alan_dir(workspace_root))
    }

    pub fn workspace_default_root_dir_from_alan_dir(&self, alan_dir: &Path) -> PathBuf {
        self.default_root_dir_from_alan_dir(alan_dir)
    }

    pub fn workspace_named_agents_dir(&self, workspace_root: &Path) -> PathBuf {
        self.agent_roots_dir_from_alan_dir(&workspace_alan_dir(workspace_root))
    }

    pub fn workspace_named_root(&self, workspace_root: &Path, name: &str) -> AgentRootPaths {
        AgentRootPaths::new(
            AgentRootKind::WorkspaceNamed(name.to_string()),
            self.workspace_named_root_dir(workspace_root, name),
        )
    }

    pub fn workspace_named_root_dir(&self, workspace_root: &Path, name: &str) -> PathBuf {
        self.workspace_named_agents_dir(workspace_root).join(name)
    }

    pub fn launch_root(&self, root_dir: PathBuf) -> AgentRootPaths {
        AgentRootPaths::new(AgentRootKind::LaunchRoot, root_dir)
    }

    pub fn agent_config_path(&self, root_dir: &Path) -> PathBuf {
        root_dir.join(AGENT_CONFIG_FILENAME)
    }

    pub fn persona_dir(&self, root_dir: &Path) -> PathBuf {
        root_dir.join(PERSONA_DIRNAME)
    }

    pub fn skills_dir(&self, root_dir: &Path) -> PathBuf {
        root_dir.join(SKILLS_DIRNAME)
    }

    pub fn policy_path(&self, root_dir: &Path) -> PathBuf {
        root_dir.join(POLICY_FILENAME)
    }

    pub fn normalize_agent_name<'a>(&self, agent_name: Option<&'a str>) -> Option<&'a str> {
        agent_name.and_then(|name| {
            let trimmed = name.trim();
            if trimmed.is_empty() || !self.is_single_path_component(trimmed) {
                None
            } else {
                Some(trimmed)
            }
        })
    }

    pub fn normalize_named_agent_name<'a>(&self, agent_name: Option<&'a str>) -> Option<&'a str> {
        self.normalize_agent_name(agent_name).and_then(|name| {
            if name == DEFAULT_AGENT_NAME {
                None
            } else {
                Some(name)
            }
        })
    }

    pub fn is_single_path_component(&self, name: &str) -> bool {
        let mut components = Path::new(name).components();
        matches!(components.next(), Some(Component::Normal(component)) if component == OsStr::new(name))
            && components.next().is_none()
    }

    pub fn is_default_agent_config_path_shape(&self, path: &Path) -> bool {
        let file_name = path.file_name().and_then(|name| name.to_str());
        let parent_name = path
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str());
        let grandparent_name = path
            .parent()
            .and_then(Path::parent)
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str());
        let great_grandparent_name = path
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str());

        file_name == Some(AGENT_CONFIG_FILENAME)
            && parent_name == Some(DEFAULT_AGENT_NAME)
            && grandparent_name == Some(AGENTS_DIRNAME)
            && great_grandparent_name == Some(ALAN_DIRNAME)
    }

    pub fn default_agent_config_suffix(&self) -> PathBuf {
        Path::new(ALAN_DIRNAME)
            .join(AGENTS_DIRNAME)
            .join(DEFAULT_AGENT_NAME)
            .join(AGENT_CONFIG_FILENAME)
    }

    pub fn agent_roots_dir_from_alan_dir(&self, alan_dir: &Path) -> PathBuf {
        alan_dir.join(AGENTS_DIRNAME)
    }

    pub fn default_root_dir_from_alan_dir(&self, alan_dir: &Path) -> PathBuf {
        self.agent_roots_dir_from_alan_dir(alan_dir)
            .join(DEFAULT_AGENT_NAME)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRootKind {
    GlobalDefault,
    WorkspaceDefault,
    GlobalNamed(String),
    WorkspaceNamed(String),
    LaunchRoot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRootPaths {
    pub kind: AgentRootKind,
    pub root_dir: PathBuf,
    pub config_path: PathBuf,
    pub persona_dir: PathBuf,
    pub skills_dir: PathBuf,
    pub policy_path: PathBuf,
}

impl AgentRootPaths {
    pub fn new(kind: AgentRootKind, root_dir: PathBuf) -> Self {
        let layout = AgentRootLayout::new();
        Self {
            kind,
            config_path: layout.agent_config_path(&root_dir),
            persona_dir: layout.persona_dir(&root_dir),
            skills_dir: layout.skills_dir(&root_dir),
            policy_path: layout.policy_path(&root_dir),
            root_dir,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolvedAgentRoots {
    roots: Vec<AgentRootPaths>,
}

impl ResolvedAgentRoots {
    pub fn for_workspace(workspace_root: Option<&Path>, agent_name: Option<&str>) -> Self {
        Self::with_home_paths(AlanHomePaths::detect(), workspace_root, agent_name)
    }

    pub(crate) fn with_home_paths(
        home_paths: Option<AlanHomePaths>,
        workspace_root: Option<&Path>,
        agent_name: Option<&str>,
    ) -> Self {
        let layout = AgentRootLayout::new();
        let mut roots = Vec::new();

        if let Some(home_paths) = home_paths {
            roots.push(layout.global_default_root(&home_paths));
            if let Some(workspace_root) = workspace_root {
                roots.push(layout.workspace_default_root(workspace_root));
            }
            if let Some(name) = layout.normalize_named_agent_name(agent_name) {
                roots.push(layout.global_named_root(&home_paths, name));
                if let Some(workspace_root) = workspace_root {
                    roots.push(layout.workspace_named_root(workspace_root, name));
                }
            }
        } else if let Some(workspace_root) = workspace_root {
            roots.push(layout.workspace_default_root(workspace_root));
            if let Some(name) = layout.normalize_named_agent_name(agent_name) {
                roots.push(layout.workspace_named_root(workspace_root, name));
            }
        }

        Self { roots }
    }

    pub fn roots(&self) -> &[AgentRootPaths] {
        &self.roots
    }

    pub fn with_appended_root(mut self, root: AgentRootPaths) -> Self {
        self.roots.push(root);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    pub fn config_paths(&self) -> Vec<PathBuf> {
        self.roots
            .iter()
            .map(|root| root.config_path.clone())
            .collect()
    }

    pub fn persona_dirs(&self) -> Vec<PathBuf> {
        self.roots
            .iter()
            .map(|root| root.persona_dir.clone())
            .collect()
    }

    pub fn skills_dirs(&self) -> Vec<PathBuf> {
        self.roots
            .iter()
            .map(|root| root.skills_dir.clone())
            .collect()
    }

    pub fn highest_precedence_policy_path(&self) -> Option<PathBuf> {
        self.roots
            .iter()
            .rev()
            .map(|root| root.policy_path.clone())
            .find(|path| path.exists())
    }

    pub fn writable_root_dir(&self) -> Option<PathBuf> {
        self.roots.last().map(|root| root.root_dir.clone())
    }

    pub fn writable_config_path(&self) -> Option<PathBuf> {
        self.roots.last().map(|root| root.config_path.clone())
    }

    pub fn writable_persona_dir(&self) -> Option<PathBuf> {
        self.roots
            .iter()
            .rev()
            .find(|root| {
                matches!(
                    root.kind,
                    AgentRootKind::LaunchRoot
                        | AgentRootKind::WorkspaceDefault
                        | AgentRootKind::WorkspaceNamed(_)
                )
            })
            .map(|root| root.persona_dir.clone())
    }
}

pub fn workspace_alan_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join(ALAN_DIRNAME)
}

pub fn workspace_sessions_dir(workspace_root: &Path) -> PathBuf {
    workspace_sessions_dir_from_alan_dir(&workspace_alan_dir(workspace_root))
}

pub fn workspace_sessions_dir_from_alan_dir(workspace_alan_dir: &Path) -> PathBuf {
    workspace_alan_dir.join(SESSIONS_DIRNAME)
}

pub fn workspace_memory_dir(workspace_root: &Path) -> PathBuf {
    workspace_memory_dir_from_alan_dir(&workspace_alan_dir(workspace_root))
}

pub fn workspace_memory_dir_from_alan_dir(workspace_alan_dir: &Path) -> PathBuf {
    workspace_alan_dir.join(MEMORY_DIRNAME)
}

pub fn workspace_runtime_dir(workspace_root: &Path, channel: InstallChannel) -> PathBuf {
    workspace_runtime_dir_from_alan_dir(&workspace_alan_dir(workspace_root), channel)
}

pub fn workspace_runtime_dir_from_alan_dir(
    workspace_alan_dir: &Path,
    channel: InstallChannel,
) -> PathBuf {
    workspace_alan_dir
        .join(RUNTIME_DIRNAME)
        .join(channel.descriptor().id)
}

pub fn workspace_runtime_sessions_dir(workspace_root: &Path, channel: InstallChannel) -> PathBuf {
    workspace_runtime_sessions_dir_from_alan_dir(&workspace_alan_dir(workspace_root), channel)
}

pub fn workspace_runtime_sessions_dir_from_alan_dir(
    workspace_alan_dir: &Path,
    channel: InstallChannel,
) -> PathBuf {
    workspace_runtime_dir_from_alan_dir(workspace_alan_dir, channel).join(SESSIONS_DIRNAME)
}

pub fn workspace_runtime_memory_dir(workspace_root: &Path, channel: InstallChannel) -> PathBuf {
    workspace_runtime_memory_dir_from_alan_dir(&workspace_alan_dir(workspace_root), channel)
}

pub fn workspace_runtime_memory_dir_from_alan_dir(
    workspace_alan_dir: &Path,
    channel: InstallChannel,
) -> PathBuf {
    workspace_runtime_dir_from_alan_dir(workspace_alan_dir, channel).join(MEMORY_DIRNAME)
}

pub fn workspace_runtime_cache_dir_from_alan_dir(
    workspace_alan_dir: &Path,
    channel: InstallChannel,
) -> PathBuf {
    workspace_runtime_dir_from_alan_dir(workspace_alan_dir, channel).join("cache")
}

pub fn workspace_runtime_shell_restore_dir_from_alan_dir(
    workspace_alan_dir: &Path,
    channel: InstallChannel,
) -> PathBuf {
    workspace_runtime_dir_from_alan_dir(workspace_alan_dir, channel).join("shell-restore")
}

pub fn workspace_runtime_metadata_dir_from_alan_dir(
    workspace_alan_dir: &Path,
    channel: InstallChannel,
) -> PathBuf {
    workspace_runtime_dir_from_alan_dir(workspace_alan_dir, channel).join("metadata")
}

pub fn workspace_runtime_tmp_dir_from_alan_dir(
    workspace_alan_dir: &Path,
    channel: InstallChannel,
) -> PathBuf {
    workspace_runtime_dir_from_alan_dir(workspace_alan_dir, channel).join("tmp")
}

pub fn workspace_sessions_dir_for_channel_from_alan_dir(
    workspace_alan_dir: &Path,
    channel: InstallChannel,
) -> PathBuf {
    workspace_runtime_sessions_dir_from_alan_dir(workspace_alan_dir, channel)
}

pub fn workspace_memory_dir_for_channel_from_alan_dir(
    workspace_alan_dir: &Path,
    channel: InstallChannel,
) -> PathBuf {
    let runtime_memory_dir =
        workspace_runtime_memory_dir_from_alan_dir(workspace_alan_dir, channel);
    if channel == InstallChannel::Stable && !runtime_memory_dir.exists() {
        let legacy_memory_dir = workspace_memory_dir_from_alan_dir(workspace_alan_dir);
        if legacy_memory_dir.exists() {
            return legacy_memory_dir;
        }
    }
    runtime_memory_dir
}

pub fn workspace_session_read_dirs_for_channel_from_alan_dir(
    workspace_alan_dir: &Path,
    channel: InstallChannel,
) -> Vec<PathBuf> {
    let mut dirs = vec![workspace_runtime_sessions_dir_from_alan_dir(
        workspace_alan_dir,
        channel,
    )];
    if channel == InstallChannel::Stable {
        dirs.push(workspace_sessions_dir_from_alan_dir(workspace_alan_dir));
    }
    dirs
}

pub fn workspace_agent_root_dir(workspace_root: &Path) -> PathBuf {
    workspace_agent_root_dir_from_alan_dir(&workspace_alan_dir(workspace_root))
}

pub fn workspace_agent_root_dir_from_alan_dir(workspace_alan_dir: &Path) -> PathBuf {
    AgentRootLayout::new().workspace_default_root_dir_from_alan_dir(workspace_alan_dir)
}

pub fn workspace_persona_dir(workspace_root: &Path) -> PathBuf {
    workspace_persona_dir_from_alan_dir(&workspace_alan_dir(workspace_root))
}

pub fn workspace_persona_dir_from_alan_dir(workspace_alan_dir: &Path) -> PathBuf {
    let layout = AgentRootLayout::new();
    layout.persona_dir(&layout.workspace_default_root_dir_from_alan_dir(workspace_alan_dir))
}

pub fn workspace_skills_dir(workspace_root: &Path) -> PathBuf {
    workspace_skills_dir_from_alan_dir(&workspace_alan_dir(workspace_root))
}

pub fn workspace_skills_dir_from_alan_dir(workspace_alan_dir: &Path) -> PathBuf {
    let layout = AgentRootLayout::new();
    layout.skills_dir(&layout.workspace_default_root_dir_from_alan_dir(workspace_alan_dir))
}

pub fn workspace_named_agents_dir(workspace_root: &Path) -> PathBuf {
    AgentRootLayout::new().workspace_named_agents_dir(workspace_root)
}

pub fn workspace_public_skills_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".agents").join("skills")
}

pub fn workspace_named_agent_root_dir(workspace_root: &Path, agent_name: &str) -> PathBuf {
    AgentRootLayout::new().workspace_named_root_dir(workspace_root, agent_name)
}

pub fn normalize_agent_name(agent_name: Option<&str>) -> Option<&str> {
    AgentRootLayout::new().normalize_agent_name(agent_name)
}

pub fn normalize_named_agent_name(agent_name: Option<&str>) -> Option<&str> {
    AgentRootLayout::new().normalize_named_agent_name(agent_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn resolve_workspace_default_roots_in_overlay_order() {
        let workspace_root = Path::new("/tmp/demo-workspace");
        let roots = ResolvedAgentRoots::with_home_paths(
            Some(AlanHomePaths::from_home_dir(Path::new("/tmp/demo-home"))),
            Some(workspace_root),
            None,
        );

        assert_eq!(roots.roots().len(), 2);
        assert!(matches!(
            roots.roots()[0].kind,
            AgentRootKind::GlobalDefault
        ));
        assert_eq!(
            roots.roots()[0].root_dir,
            Path::new("/tmp/demo-home/.alan/agents/default")
        );
        assert!(matches!(
            roots.roots()[1].kind,
            AgentRootKind::WorkspaceDefault
        ));
        assert_eq!(
            roots.roots()[1].root_dir,
            workspace_root.join(".alan").join("agents").join("default")
        );
    }

    #[test]
    fn resolve_named_agent_roots_after_base_roots() {
        let workspace_root = Path::new("/tmp/demo-workspace");
        let roots = ResolvedAgentRoots::with_home_paths(
            Some(AlanHomePaths::from_home_dir(Path::new("/tmp/demo-home"))),
            Some(workspace_root),
            Some("coder"),
        );

        assert_eq!(roots.roots().len(), 4);
        assert!(matches!(
            roots.roots()[0].kind,
            AgentRootKind::GlobalDefault
        ));
        assert!(matches!(
            roots.roots()[1].kind,
            AgentRootKind::WorkspaceDefault
        ));
        assert!(
            matches!(roots.roots()[2].kind, AgentRootKind::GlobalNamed(ref name) if name == "coder")
        );
        assert!(
            matches!(roots.roots()[3].kind, AgentRootKind::WorkspaceNamed(ref name) if name == "coder")
        );
        assert_eq!(
            roots.roots()[2].root_dir,
            Path::new("/tmp/demo-home/.alan/agents/coder")
        );
        assert_eq!(
            roots.roots()[3].root_dir,
            workspace_root.join(".alan").join("agents").join("coder")
        );
    }

    #[test]
    fn highest_precedence_policy_path_prefers_workspace_named_root() {
        let workspace_root = tempfile::TempDir::new().unwrap();
        let roots = ResolvedAgentRoots::with_home_paths(
            Some(AlanHomePaths::from_home_dir(Path::new("/tmp/demo-home"))),
            Some(workspace_root.path()),
            Some("coder"),
        );
        std::fs::create_dir_all(roots.roots()[0].root_dir.clone()).unwrap();
        std::fs::write(&roots.roots()[0].policy_path, "rules: []\n").unwrap();
        std::fs::create_dir_all(roots.roots()[3].root_dir.clone()).unwrap();
        std::fs::write(&roots.roots()[3].policy_path, "rules: []\n").unwrap();

        assert_eq!(
            roots.highest_precedence_policy_path(),
            Some(roots.roots()[3].policy_path.clone())
        );
    }

    #[test]
    fn writable_persona_dir_prefers_workspace_root() {
        let workspace_root = Path::new("/tmp/demo-workspace");
        let roots = ResolvedAgentRoots::with_home_paths(
            Some(AlanHomePaths::from_home_dir(Path::new("/tmp/demo-home"))),
            Some(workspace_root),
            None,
        );

        assert_eq!(
            roots.writable_persona_dir(),
            Some(
                workspace_root
                    .join(".alan")
                    .join("agents")
                    .join("default")
                    .join("persona")
            )
        );
    }

    #[test]
    fn explicit_default_agent_name_uses_default_roots_only() {
        let workspace_root = Path::new("/tmp/demo-workspace");
        let roots = ResolvedAgentRoots::with_home_paths(
            Some(AlanHomePaths::from_home_dir(Path::new("/tmp/demo-home"))),
            Some(workspace_root),
            Some(DEFAULT_AGENT_NAME),
        );

        assert_eq!(roots.roots().len(), 2);
        assert!(matches!(
            roots.roots()[0].kind,
            AgentRootKind::GlobalDefault
        ));
        assert!(matches!(
            roots.roots()[1].kind,
            AgentRootKind::WorkspaceDefault
        ));
        assert!(roots.roots().iter().all(|root| !matches!(
            root.kind,
            AgentRootKind::GlobalNamed(_) | AgentRootKind::WorkspaceNamed(_)
        )));
    }

    #[test]
    fn writable_root_dir_prefers_highest_precedence_root() {
        let workspace_root = Path::new("/tmp/demo-workspace");
        let roots = ResolvedAgentRoots::with_home_paths(
            Some(AlanHomePaths::from_home_dir(Path::new("/tmp/demo-home"))),
            Some(workspace_root),
            Some("coder"),
        );

        assert_eq!(
            roots.writable_root_dir(),
            Some(workspace_root.join(".alan").join("agents").join("coder"))
        );
    }

    #[test]
    fn named_agent_roots_ignore_path_traversal_names() {
        let workspace_root = Path::new("/tmp/demo-workspace");
        let roots = ResolvedAgentRoots::with_home_paths(
            Some(AlanHomePaths::from_home_dir(Path::new("/tmp/demo-home"))),
            Some(workspace_root),
            Some("../coder"),
        );

        assert_eq!(roots.roots().len(), 2);
        assert!(matches!(
            roots.roots()[0].kind,
            AgentRootKind::GlobalDefault
        ));
        assert!(matches!(
            roots.roots()[1].kind,
            AgentRootKind::WorkspaceDefault
        ));
    }

    #[test]
    fn named_agent_roots_keep_single_component_names() {
        let workspace_root = Path::new("/tmp/demo-workspace");
        let roots = ResolvedAgentRoots::with_home_paths(
            Some(AlanHomePaths::from_home_dir(Path::new("/tmp/demo-home"))),
            Some(workspace_root),
            Some("coder.v2"),
        );

        assert_eq!(roots.roots().len(), 4);
        assert_eq!(
            roots.roots()[2].root_dir,
            Path::new("/tmp/demo-home/.alan/agents/coder.v2")
        );
        assert_eq!(
            roots.roots()[3].root_dir,
            workspace_root.join(".alan").join("agents").join("coder.v2")
        );
    }

    #[test]
    fn workspace_layout_helpers_share_the_same_canonical_layout() {
        let workspace_root = Path::new("/tmp/demo-workspace");
        let alan_dir = workspace_alan_dir(workspace_root);
        let layout = AgentRootLayout::new();
        let default_root = layout.workspace_default_root(workspace_root);

        assert_eq!(
            workspace_sessions_dir(workspace_root),
            alan_dir.join("sessions")
        );
        assert_eq!(
            workspace_sessions_dir_from_alan_dir(&alan_dir),
            alan_dir.join("sessions")
        );
        assert_eq!(
            workspace_memory_dir(workspace_root),
            alan_dir.join("memory")
        );
        assert_eq!(
            workspace_memory_dir_from_alan_dir(&alan_dir),
            alan_dir.join("memory")
        );
        assert_eq!(
            workspace_runtime_dir(workspace_root, InstallChannel::Dev),
            alan_dir.join("runtime").join("dev")
        );
        assert_eq!(
            workspace_runtime_sessions_dir(workspace_root, InstallChannel::Stable),
            alan_dir.join("runtime").join("stable").join("sessions")
        );
        assert_eq!(
            workspace_runtime_memory_dir_from_alan_dir(&alan_dir, InstallChannel::Dev),
            alan_dir.join("runtime").join("dev").join("memory")
        );
        assert_eq!(
            workspace_runtime_cache_dir_from_alan_dir(&alan_dir, InstallChannel::Dev),
            alan_dir.join("runtime").join("dev").join("cache")
        );
        assert_eq!(
            workspace_runtime_shell_restore_dir_from_alan_dir(&alan_dir, InstallChannel::Dev),
            alan_dir.join("runtime").join("dev").join("shell-restore")
        );
        assert_eq!(
            workspace_runtime_metadata_dir_from_alan_dir(&alan_dir, InstallChannel::Dev),
            alan_dir.join("runtime").join("dev").join("metadata")
        );
        assert_eq!(
            workspace_runtime_tmp_dir_from_alan_dir(&alan_dir, InstallChannel::Dev),
            alan_dir.join("runtime").join("dev").join("tmp")
        );
        assert_eq!(
            workspace_agent_root_dir(workspace_root),
            default_root.root_dir.clone()
        );
        assert_eq!(
            workspace_agent_root_dir_from_alan_dir(&alan_dir),
            default_root.root_dir.clone()
        );
        assert_eq!(
            workspace_persona_dir(workspace_root),
            default_root.persona_dir.clone()
        );
        assert_eq!(
            workspace_persona_dir_from_alan_dir(&alan_dir),
            default_root.persona_dir.clone()
        );
        assert_eq!(
            workspace_skills_dir(workspace_root),
            default_root.skills_dir.clone()
        );
        assert_eq!(
            workspace_skills_dir_from_alan_dir(&alan_dir),
            default_root.skills_dir.clone()
        );
        assert_eq!(
            workspace_named_agents_dir(workspace_root),
            layout.workspace_named_agents_dir(workspace_root)
        );
    }

    #[test]
    fn stable_memory_helper_reads_existing_legacy_memory_only_for_stable() {
        let temp = tempfile::TempDir::new().unwrap();
        let alan_dir = temp.path().join(".alan");
        std::fs::create_dir_all(alan_dir.join("memory")).unwrap();

        assert_eq!(
            workspace_memory_dir_for_channel_from_alan_dir(&alan_dir, InstallChannel::Stable),
            alan_dir.join("memory")
        );
        assert_eq!(
            workspace_memory_dir_for_channel_from_alan_dir(&alan_dir, InstallChannel::Dev),
            alan_dir.join("runtime").join("dev").join("memory")
        );
    }

    #[test]
    fn stable_memory_helper_prefers_channel_runtime_memory_when_present() {
        let temp = tempfile::TempDir::new().unwrap();
        let alan_dir = temp.path().join(".alan");
        std::fs::create_dir_all(alan_dir.join("memory")).unwrap();
        std::fs::create_dir_all(alan_dir.join("runtime/stable/memory")).unwrap();

        assert_eq!(
            workspace_memory_dir_for_channel_from_alan_dir(&alan_dir, InstallChannel::Stable),
            alan_dir.join("runtime").join("stable").join("memory")
        );
    }

    #[test]
    fn session_read_dirs_include_legacy_only_for_stable() {
        let alan_dir = Path::new("/tmp/demo-workspace/.alan");

        assert_eq!(
            workspace_session_read_dirs_for_channel_from_alan_dir(alan_dir, InstallChannel::Stable),
            vec![
                alan_dir.join("runtime").join("stable").join("sessions"),
                alan_dir.join("sessions"),
            ]
        );
        assert_eq!(
            workspace_session_read_dirs_for_channel_from_alan_dir(alan_dir, InstallChannel::Dev),
            vec![alan_dir.join("runtime").join("dev").join("sessions")]
        );
    }

    #[test]
    fn typed_layout_exposes_standard_agent_root_assets() {
        let layout = AgentRootLayout::new();
        let root = Path::new("/tmp/demo-workspace/.alan/agents/default");

        assert_eq!(layout.agent_config_path(root), root.join("agent.toml"));
        assert_eq!(layout.persona_dir(root), root.join("persona"));
        assert_eq!(layout.skills_dir(root), root.join("skills"));
        assert_eq!(layout.policy_path(root), root.join("policy.yaml"));
        assert!(layout.is_default_agent_config_path_shape(&root.join("agent.toml")));
        assert!(!layout.is_default_agent_config_path_shape(Path::new(
            "/tmp/demo-workspace/.alan/agents/coder/agent.toml"
        )));
    }

    #[test]
    fn typed_layout_centralizes_agent_name_semantics() {
        let layout = AgentRootLayout::new();

        assert_eq!(
            layout.normalize_agent_name(Some(" default ")),
            Some("default")
        );
        assert_eq!(layout.normalize_named_agent_name(Some("default")), None);
        assert_eq!(
            layout.normalize_named_agent_name(Some("coder")),
            Some("coder")
        );
        assert_eq!(layout.normalize_agent_name(Some("../coder")), None);
        assert_eq!(layout.normalize_agent_name(Some("nested/coder")), None);
    }
}
