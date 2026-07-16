mod pane_tree;

pub use pane_tree::{
    PaneTreeKind, PaneTreeNode, PaneTreeNodeResizeOutcome, PaneTreeNodeResizeResult,
    SpatialFocusDirection, SplitDirection, SplitPlacement,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Workspace attention state shared by Spaces and pane slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellAttentionState {
    /// No current signal.
    Idle,
    /// Active liveness signal that does not require user action.
    Active,
    /// Waiting for user input or approval.
    AwaitingUser,
    /// Notable failure or intervention signal.
    Notable,
}

/// Portable active task state for a terminal-backed tab.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellTabActiveTaskState {
    /// No active task.
    #[default]
    Inactive,
    /// A foreground shell command is running.
    ForegroundCommand,
    /// Alan is running.
    AlanRunning,
    /// Alan is waiting for user input or approval.
    AlanPendingYield,
    /// An Alan Process is visible in the terminal pane.
    AlanProcess,
    /// Unknown task state that should be retained conservatively.
    #[serde(other)]
    Unknown,
}

impl ShellTabActiveTaskState {
    /// Returns whether this task state protects a tab from automatic pruning.
    pub fn protects_from_pruning(self) -> bool {
        !matches!(self, ShellTabActiveTaskState::Inactive)
    }
}

/// Portable terminal launch target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellLaunchTarget {
    /// Default shell.
    Shell,
}

/// Portable terminal activity source kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalActivitySourceKind {
    /// Codex activity.
    Codex,
    /// Claude activity.
    Claude,
    /// OpenCode activity.
    OpenCode,
    /// Alan activity.
    Alan,
    /// Shell activity.
    Shell,
    /// Progress activity.
    Progress,
    /// Command activity.
    Command,
    /// Process activity.
    Process,
    /// Unknown activity source.
    Unknown,
}

/// Portable terminal activity status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalActivityStatus {
    /// Activity needs input.
    NeedsInput,
    /// Activity failed.
    Failed,
    /// Activity paused.
    Paused,
    /// Activity reports progress.
    Progress,
    /// Activity is running.
    Running,
    /// Terminal bell activity.
    Bell,
    /// Process exited.
    Exited,
    /// Idle activity.
    Idle,
    /// Activity completed.
    Done,
    /// Activity is stale.
    Stale,
}

/// Portable terminal activity priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalActivityPriority {
    /// Passive activity.
    Passive,
    /// Active activity.
    Active,
    /// Notable activity.
    Notable,
    /// Awaiting user activity.
    AwaitingUser,
}

/// Portable terminal activity source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalActivitySource {
    /// Source kind.
    pub kind: TerminalActivitySourceKind,
    /// Optional display label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Portable terminal activity agent metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalActivityAgentMetadata {
    /// Agent kind.
    pub kind: TerminalActivitySourceKind,
    /// Sanitized session label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safe_session_label: Option<String>,
    /// Project label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_label: Option<String>,
    /// Working directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
}

/// Portable terminal activity display metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalActivityDisplay {
    /// Source label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_label: Option<String>,
    /// State label.
    pub state_label: String,
    /// Detail label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail_label: Option<String>,
    /// Pane hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_hint: Option<String>,
}

/// Portable terminal activity freshness metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalActivityFreshness {
    /// Last update time as an ISO-8601 string.
    pub updated_at: String,
    /// Optional stale deadline as an ISO-8601 string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_at: Option<String>,
    /// Optional expiry deadline as an ISO-8601 string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// Portable terminal activity snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalActivitySnapshot {
    /// Activity source.
    pub source: TerminalActivitySource,
    /// Activity status.
    pub status: TerminalActivityStatus,
    /// Activity priority.
    pub priority: TerminalActivityPriority,
    /// Optional agent metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<TerminalActivityAgentMetadata>,
    /// Display metadata.
    pub display: TerminalActivityDisplay,
    /// Freshness metadata.
    pub freshness: TerminalActivityFreshness,
}

/// Portable terminal runtime metadata projected into shell core.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalRuntimeMetadata {
    /// Runtime-reported title.
    pub title: Option<String>,
    /// Runtime-reported current working directory.
    pub cwd: Option<String>,
    /// Active task state used for pruning and close guards.
    #[serde(default)]
    pub active_task_state: ShellTabActiveTaskState,
    /// Optional activity snapshot.
    pub activity: Option<TerminalActivitySnapshot>,
}

/// Terminal payload for restored content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellTerminalContentPayload {
    /// Launch target.
    pub launch_target: ShellLaunchTarget,
    /// Restored current working directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Restored terminal title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Restored transcript snapshot, preserved opaquely by shell core.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_snapshot: Option<Value>,
    /// Terminal Profile id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_profile_id: Option<String>,
}

/// Stable identity of one Alan OS Process within a single Host boot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentProcessReference {
    /// Alan OS Host boot identity.
    pub boot_id: String,
    /// Kernel Process identifier.
    pub pid: u64,
}

/// Renderer-owned byte offsets for AgentFS streams.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentStreamOffsets {
    /// `/agent/<pid>/io/output` offset.
    pub output: u64,
    /// `/agent/<pid>/requests/events` offset.
    pub requests: u64,
    /// `/agent/<pid>/actions/events` offset.
    pub actions: u64,
    /// `/agent/<pid>/machine/ui/events` offset.
    pub ui: u64,
}

/// Host-owned presentation preferences for an Agent ContentInstance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentContentPresentation {
    /// Whether a live renderer follows newly appended output.
    #[serde(default)]
    pub follows_output: bool,
}

impl Default for AgentContentPresentation {
    fn default() -> Self {
        Self {
            follows_output: true,
        }
    }
}

/// Narrow persisted attachment to an Agent Process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentAttachment {
    /// Process identity validated through `/proc` before AgentFS access.
    pub process: AgentProcessReference,
    /// Independent caller-held stream positions.
    #[serde(default)]
    pub offsets: AgentStreamOffsets,
    /// Host presentation only; never Agent Machine state.
    #[serde(default)]
    pub presentation: AgentContentPresentation,
}

/// Portable content restore payload.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShellContentPayload {
    /// Terminal payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal: Option<ShellTerminalContentPayload>,
    /// Markdown payload, preserved opaquely by shell core.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markdown: Option<Value>,
    /// Settings payload, preserved opaquely by shell core.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<Value>,
    /// Agent Process attachment reference and renderer-owned offsets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentAttachment>,
}

impl ShellContentPayload {
    /// Creates a terminal payload.
    pub fn terminal(
        launch_target: ShellLaunchTarget,
        cwd: Option<&str>,
        title: Option<&str>,
    ) -> Self {
        Self::terminal_with_profile(launch_target, cwd, title, None)
    }

    pub(crate) fn terminal_with_profile(
        launch_target: ShellLaunchTarget,
        cwd: Option<&str>,
        title: Option<&str>,
        terminal_profile_id: Option<&str>,
    ) -> Self {
        Self {
            terminal: Some(ShellTerminalContentPayload {
                launch_target,
                cwd: cwd.map(ToOwned::to_owned),
                title: title.map(ToOwned::to_owned),
                transcript_snapshot: None,
                terminal_profile_id: terminal_profile_id.map(ToOwned::to_owned),
            }),
            markdown: None,
            settings: None,
            agent: None,
        }
    }

    /// Returns whether the payload carries no restorable content-specific data.
    pub fn is_empty(&self) -> bool {
        self.terminal.is_none()
            && self.markdown.is_none()
            && self.settings.is_none()
            && self.agent.is_none()
    }
}

/// Portable tab kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TabKind {
    /// Terminal-backed tab.
    Terminal,
    /// Scratch tab.
    Scratch,
    /// Log tab.
    Log,
}

/// Portable tab organization section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TabOrganizationSection {
    /// Pinned tab section.
    Pinned,
    /// Unpinned tab section.
    Unpinned,
}

impl TabOrganizationSection {
    pub(crate) fn is_pinned(self) -> bool {
        matches!(self, Self::Pinned)
    }
}

/// Portable content kind mounted in a pane slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentKind {
    /// Terminal content.
    Terminal,
    /// Markdown viewer content.
    Markdown,
    /// Settings surface content.
    Settings,
    /// File-backed Agent Process renderer.
    Agent,
}

impl ContentKind {
    /// Default portable capabilities for this content kind.
    pub fn default_capabilities(self) -> Vec<ContentCapability> {
        match self {
            ContentKind::Terminal => vec![
                ContentCapability::TerminalInput,
                ContentCapability::TerminalSearch,
                ContentCapability::TerminalPaste,
                ContentCapability::TerminalRuntimeMetadata,
            ],
            ContentKind::Markdown => vec![ContentCapability::MarkdownReadOnlyViewer],
            ContentKind::Settings => vec![ContentCapability::SettingsSurface],
            ContentKind::Agent => vec![
                ContentCapability::AgentInput,
                ContentCapability::AgentRequestResponse,
                ContentCapability::AgentMachineControl,
                ContentCapability::AgentStopProcess,
            ],
        }
    }
}

/// Portable content capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentCapability {
    /// Terminal accepts input.
    TerminalInput,
    /// Terminal supports search.
    TerminalSearch,
    /// Terminal supports paste.
    TerminalPaste,
    /// Terminal can publish runtime metadata.
    TerminalRuntimeMetadata,
    /// Markdown content is rendered read-only.
    MarkdownReadOnlyViewer,
    /// Settings content renders a settings surface.
    SettingsSurface,
    /// AgentFS input writes.
    AgentInput,
    /// AgentFS request response writes.
    AgentRequestResponse,
    /// Agent Machine control and interrupt writes.
    AgentMachineControl,
    /// Explicit `/proc/<pid>/ctl` stop action.
    AgentStopProcess,
}

/// Portable content lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentLifecycleState {
    /// Content is active.
    Active,
    /// Content is closing.
    Closing,
    /// Content is closed.
    Closed,
    /// Content failed.
    Failed,
}

/// Presentation/runtime renderer state for mounted content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentRendererState {
    /// Renderer phase such as placeholder or surface_ready.
    pub phase: String,
    /// Optional renderer detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl ContentRendererState {
    /// Placeholder renderer state used before live runtime information exists.
    pub fn placeholder() -> Self {
        Self {
            phase: "placeholder".to_string(),
            detail: None,
        }
    }

    /// True when this state carries no live renderer information.
    pub fn is_placeholder(&self) -> bool {
        self.phase == "placeholder" && self.detail.is_none()
    }
}

impl Default for ContentRendererState {
    fn default() -> Self {
        Self::placeholder()
    }
}

/// Mounted content instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentInstance {
    /// Stable content id.
    pub content_id: String,
    /// Content kind.
    pub kind: ContentKind,
    /// Human-readable title.
    pub title: String,
    /// Optional icon name owned by adapters or presentation layers.
    pub icon_name: Option<String>,
    /// Portable capabilities.
    pub capabilities: Vec<ContentCapability>,
    /// Restore payload carried by the mounted content.
    #[serde(default, skip_serializing_if = "ShellContentPayload::is_empty")]
    pub payload: ShellContentPayload,
    /// Terminal runtime metadata for terminal content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_metadata: Option<TerminalRuntimeMetadata>,
    /// Lifecycle state.
    pub lifecycle: ContentLifecycleState,
    /// Renderer state projected from the host runtime.
    #[serde(default, skip_serializing_if = "ContentRendererState::is_placeholder")]
    pub renderer_state: ContentRendererState,
}

/// Pane slot mounting a content instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneSlot {
    /// Stable pane slot id.
    pub pane_slot_id: String,
    /// Owning tab id.
    pub tab_id: String,
    /// Owning Space id.
    pub space_id: String,
    /// Mounted content id.
    pub content_id: String,
    /// Pane slot attention.
    pub attention: ShellAttentionState,
}

/// Portable tab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tab {
    /// Stable tab id.
    pub tab_id: String,
    /// Tab kind.
    pub kind: TabKind,
    /// Optional title.
    pub title: Option<String>,
    /// Pane split tree.
    pub pane_tree: PaneTreeNode,
    /// Tab-scoped zoomed pane id, when the tab is displaying one pane from a split.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoomed_pane_id: Option<String>,
    /// Whether the tab is pinned.
    pub is_pinned: bool,
    /// Whether the title was explicitly set by the user.
    pub is_title_user_locked: bool,
}

/// Portable Space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Space {
    /// Stable Space id.
    pub space_id: String,
    /// Space title.
    pub title: String,
    /// Space attention.
    pub attention: ShellAttentionState,
    /// Tabs contained by the Space.
    pub tabs: Vec<Tab>,
    /// Selected tab id.
    pub selected_tab_id: Option<String>,
    /// Default Terminal Profile id for terminal creation in this Space.
    pub terminal_profile_id: Option<String>,
    /// Optional presentation icon system name.
    pub presentation_icon: Option<String>,
}

/// Platform-neutral workspace state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceState {
    /// Workspace contract version.
    pub contract_version: String,
    /// Stable window id.
    pub window_id: String,
    /// Focused Space id.
    pub focused_space_id: Option<String>,
    /// Focused tab id.
    pub focused_tab_id: Option<String>,
    /// Focused pane id.
    pub focused_pane_id: Option<String>,
    /// Spaces in workspace order.
    pub spaces: Vec<Space>,
    /// Mounted pane slots.
    pub pane_slots: Vec<PaneSlot>,
    /// Mounted content instances.
    pub contents: Vec<ContentInstance>,
}

impl WorkspaceState {
    /// Returns a tab by stable id.
    pub fn tab(&self, tab_id: &str) -> Option<&Tab> {
        self.spaces
            .iter()
            .flat_map(|space| &space.tabs)
            .find(|tab| tab.tab_id == tab_id)
    }
}
