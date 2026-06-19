use serde::de;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::BTreeMap;

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
    /// Alan has an attached session.
    AlanSession,
    /// Unknown task state that should be retained conservatively.
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

/// Portable content restore payload.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
        }
    }

    /// Returns whether the payload carries no restorable content-specific data.
    pub fn is_empty(&self) -> bool {
        self.terminal.is_none() && self.markdown.is_none() && self.settings.is_none()
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

/// Split tree node kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaneTreeKind {
    /// Branch node with child nodes.
    Split,
    /// Leaf node containing one pane id.
    Pane,
}

/// Direction of a split branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitDirection {
    /// Children are stacked vertically.
    Horizontal,
    /// Children are arranged in columns.
    Vertical,
}

impl SplitDirection {
    /// Stable string used by serialized paths and diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            SplitDirection::Horizontal => "horizontal",
            SplitDirection::Vertical => "vertical",
        }
    }
}

/// Requested placement for a new or moved pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitPlacement {
    /// Place before the target in a vertical split.
    Left,
    /// Place after the target in a vertical split.
    Right,
    /// Place before the target in a horizontal split.
    Up,
    /// Place after the target in a horizontal split.
    Down,
}

impl SplitPlacement {
    fn split_direction(self) -> SplitDirection {
        match self {
            SplitPlacement::Left | SplitPlacement::Right => SplitDirection::Vertical,
            SplitPlacement::Up | SplitPlacement::Down => SplitDirection::Horizontal,
        }
    }

    fn places_new_pane_before_target(self) -> bool {
        matches!(self, SplitPlacement::Left | SplitPlacement::Up)
    }
}

/// Direction for spatial pane focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpatialFocusDirection {
    /// Move focus left.
    Left,
    /// Move focus right.
    Right,
    /// Move focus up.
    Up,
    /// Move focus down.
    Down,
}

/// Portable pane split tree.
#[derive(Debug, Clone, PartialEq)]
pub struct PaneTreeNode {
    /// Stable tree node id.
    pub node_id: String,
    /// Node kind.
    pub kind: PaneTreeKind,
    /// Split direction for branch nodes.
    pub direction: Option<SplitDirection>,
    /// Split ratio for branch nodes.
    pub ratio: Option<f64>,
    /// Pane id for leaf nodes.
    pub pane_id: Option<String>,
    /// Child nodes for branch nodes.
    pub children: Option<Vec<PaneTreeNode>>,
}

impl PaneTreeNode {
    /// Minimum usable persisted split ratio.
    pub const MINIMUM_SPLIT_RATIO: f64 = 0.15;
    /// Maximum usable persisted split ratio.
    pub const MAXIMUM_SPLIT_RATIO: f64 = 0.85;

    /// Creates a pane leaf node.
    pub fn pane(node_id: impl Into<String>, pane_id: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            kind: PaneTreeKind::Pane,
            direction: None,
            ratio: None,
            pane_id: Some(pane_id.into()),
            children: None,
        }
    }

    /// Creates a split branch with an equal ratio.
    pub fn split(
        node_id: impl Into<String>,
        direction: SplitDirection,
        children: Vec<PaneTreeNode>,
    ) -> Self {
        Self::split_with_ratio(node_id, direction, 0.5, children)
    }

    /// Creates a split branch with a persisted ratio.
    pub fn split_with_ratio(
        node_id: impl Into<String>,
        direction: SplitDirection,
        ratio: f64,
        children: Vec<PaneTreeNode>,
    ) -> Self {
        Self {
            node_id: node_id.into(),
            kind: PaneTreeKind::Split,
            direction: Some(direction),
            ratio: Some(Self::clamped_split_ratio(ratio)),
            pane_id: None,
            children: Some(children),
        }
    }

    /// Clamps a split ratio to the usable range, defaulting non-finite values.
    pub fn clamped_split_ratio(ratio: f64) -> f64 {
        if !ratio.is_finite() {
            return 0.5;
        }
        ratio.clamp(Self::MINIMUM_SPLIT_RATIO, Self::MAXIMUM_SPLIT_RATIO)
    }

    /// Returns the effective split ratio for this node.
    pub fn split_ratio(&self) -> f64 {
        Self::clamped_split_ratio(self.ratio.unwrap_or(0.5))
    }

    /// Returns pane ids in visible tree order.
    pub fn pane_ids(&self) -> Vec<String> {
        match self.kind {
            PaneTreeKind::Pane => self.pane_id.iter().cloned().collect(),
            PaneTreeKind::Split => self
                .children
                .as_deref()
                .unwrap_or_default()
                .iter()
                .flat_map(PaneTreeNode::pane_ids)
                .collect(),
        }
    }

    /// Returns node ids in tree order.
    pub fn node_ids(&self) -> Vec<String> {
        let mut ids = vec![self.node_id.clone()];
        if let Some(children) = &self.children {
            ids.extend(children.iter().flat_map(PaneTreeNode::node_ids));
        }
        ids
    }

    /// Returns split ratios keyed by split node id.
    pub fn split_ratios_by_node_id(&self) -> BTreeMap<String, f64> {
        let mut ratios = BTreeMap::new();
        self.collect_split_ratios(&mut ratios);
        ratios
    }

    fn collect_split_ratios(&self, ratios: &mut BTreeMap<String, f64>) {
        if self.kind == PaneTreeKind::Split {
            ratios.insert(self.node_id.clone(), self.split_ratio());
        }
        if let Some(children) = &self.children {
            for child in children {
                child.collect_split_ratios(ratios);
            }
        }
    }

    /// Returns split node ids whose ratios changed compared to `previous`.
    pub fn split_node_ids_with_changed_ratios(&self, previous: &PaneTreeNode) -> Vec<String> {
        let previous_ratios = previous.split_ratios_by_node_id();
        self.split_ratios_by_node_id()
            .into_iter()
            .filter_map(|(node_id, current_ratio)| {
                previous_ratios
                    .get(&node_id)
                    .is_some_and(|previous_ratio| *previous_ratio != current_ratio)
                    .then_some(node_id)
            })
            .collect()
    }

    /// Returns whether the tree contains a pane id.
    pub fn contains_pane_id(&self, pane_id: &str) -> bool {
        match self.kind {
            PaneTreeKind::Pane => self.pane_id.as_deref() == Some(pane_id),
            PaneTreeKind::Split => self
                .children
                .as_deref()
                .unwrap_or_default()
                .iter()
                .any(|child| child.contains_pane_id(pane_id)),
        }
    }

    /// Returns whether the tree contains a node id.
    pub fn contains_node_id(&self, node_id: &str) -> bool {
        self.node_id == node_id
            || self
                .children
                .as_deref()
                .unwrap_or_default()
                .iter()
                .any(|child| child.contains_node_id(node_id))
    }

    /// Returns a node by id.
    pub fn node(&self, node_id: &str) -> Option<&PaneTreeNode> {
        if self.node_id == node_id {
            return Some(self);
        }
        self.children
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find_map(|child| child.node(node_id))
    }

    /// Returns the pane leaf containing a pane id.
    pub fn leaf_node(&self, pane_id: &str) -> Option<&PaneTreeNode> {
        match self.kind {
            PaneTreeKind::Pane => (self.pane_id.as_deref() == Some(pane_id)).then_some(self),
            PaneTreeKind::Split => self
                .children
                .as_deref()
                .unwrap_or_default()
                .iter()
                .find_map(|child| child.leaf_node(pane_id)),
        }
    }

    /// Splits a pane by wrapping the target leaf in a new split branch.
    pub fn split_pane(
        &self,
        target_pane_id: &str,
        placement: SplitPlacement,
        split_node_id: impl Into<String>,
        new_leaf_node_id: impl Into<String>,
        new_pane_id: impl Into<String>,
    ) -> Self {
        let split_node_id = split_node_id.into();
        let new_leaf_node_id = new_leaf_node_id.into();
        let new_pane_id = new_pane_id.into();
        self.split_pane_inner(
            target_pane_id,
            placement,
            &split_node_id,
            &new_leaf_node_id,
            &new_pane_id,
        )
    }

    fn split_pane_inner(
        &self,
        target_pane_id: &str,
        placement: SplitPlacement,
        split_node_id: &str,
        new_leaf_node_id: &str,
        new_pane_id: &str,
    ) -> Self {
        match self.kind {
            PaneTreeKind::Pane => {
                if self.pane_id.as_deref() != Some(target_pane_id) {
                    return self.clone();
                }
                let current_leaf = Self::pane(self.node_id.clone(), target_pane_id.to_string());
                let new_leaf = Self::pane(new_leaf_node_id.to_string(), new_pane_id.to_string());
                let children = if placement.places_new_pane_before_target() {
                    vec![new_leaf, current_leaf]
                } else {
                    vec![current_leaf, new_leaf]
                };
                Self::split(
                    split_node_id.to_string(),
                    placement.split_direction(),
                    children,
                )
            }
            PaneTreeKind::Split => Self {
                node_id: self.node_id.clone(),
                kind: PaneTreeKind::Split,
                direction: self.direction,
                ratio: self.ratio,
                pane_id: None,
                children: Some(
                    self.children
                        .as_deref()
                        .unwrap_or_default()
                        .iter()
                        .map(|child| {
                            child.split_pane_inner(
                                target_pane_id,
                                placement,
                                split_node_id,
                                new_leaf_node_id,
                                new_pane_id,
                            )
                        })
                        .collect(),
                ),
            },
        }
    }

    /// Resizes a split branch by node id.
    pub fn resize_split(&self, target_node_id: &str, ratio: f64) -> PaneTreeNodeResizeResult {
        if self.kind == PaneTreeKind::Split && self.node_id == target_node_id {
            return PaneTreeNodeResizeResult {
                node: Self {
                    ratio: Some(Self::clamped_split_ratio(ratio)),
                    ..self.clone()
                },
                outcome: PaneTreeNodeResizeOutcome::Changed,
            };
        }

        let Some(children) = &self.children else {
            return PaneTreeNodeResizeResult {
                node: self.clone(),
                outcome: PaneTreeNodeResizeOutcome::Unchanged,
            };
        };

        let mut changed = false;
        let next_children = children
            .iter()
            .map(|child| {
                let result = child.resize_split(target_node_id, ratio);
                changed = changed || result.outcome == PaneTreeNodeResizeOutcome::Changed;
                result.node
            })
            .collect();

        if !changed {
            return PaneTreeNodeResizeResult {
                node: self.clone(),
                outcome: PaneTreeNodeResizeOutcome::Unchanged,
            };
        }

        PaneTreeNodeResizeResult {
            node: Self {
                children: Some(next_children),
                ..self.clone()
            },
            outcome: PaneTreeNodeResizeOutcome::Changed,
        }
    }

    /// Equalizes every split branch ratio to 0.5.
    pub fn equalized_splits(&self) -> Self {
        match self.kind {
            PaneTreeKind::Pane => self.clone(),
            PaneTreeKind::Split => Self {
                ratio: Some(0.5),
                children: self.children.as_ref().map(|children| {
                    children
                        .iter()
                        .map(PaneTreeNode::equalized_splits)
                        .collect()
                }),
                ..self.clone()
            },
        }
    }

    /// Removes a pane leaf and repairs single-child split branches.
    pub fn remove_pane(&self, target_pane_id: &str) -> Option<Self> {
        match self.kind {
            PaneTreeKind::Pane => {
                (self.pane_id.as_deref() != Some(target_pane_id)).then_some(self.clone())
            }
            PaneTreeKind::Split => {
                let remaining: Vec<_> = self
                    .children
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|child| child.remove_pane(target_pane_id))
                    .collect();
                match remaining.len() {
                    0 => None,
                    1 => remaining.into_iter().next(),
                    _ => Some(Self {
                        children: Some(remaining),
                        ..self.clone()
                    }),
                }
            }
        }
    }

    /// Attaches a pane to the end of this tree while preserving binary shape.
    pub fn attach_pane(
        &self,
        new_pane_id: impl Into<String>,
        direction: SplitDirection,
        split_node_id: impl Into<String>,
        new_leaf_node_id: impl Into<String>,
    ) -> Self {
        let new_leaf = Self::pane(new_leaf_node_id.into(), new_pane_id.into());
        let split_node_id = split_node_id.into();

        if self.kind == PaneTreeKind::Split
            && self.direction == Some(direction)
            && let Some(existing_children) = &self.children
            && let Some(last_child) = existing_children.last()
        {
            let nested_split =
                Self::split(split_node_id, direction, vec![last_child.clone(), new_leaf]);
            let mut next_children = existing_children[..existing_children.len() - 1].to_vec();
            next_children.push(nested_split);
            return Self {
                children: Some(next_children),
                ..self.clone()
            };
        }

        Self::split(split_node_id, direction, vec![self.clone(), new_leaf])
    }

    /// Returns the nearest adjacent pane id in a spatial direction.
    pub fn adjacent_pane_id(
        &self,
        target_pane_id: &str,
        direction: SpatialFocusDirection,
    ) -> Option<String> {
        let frames = self.leaf_frames(PaneFrame::unit());
        let target_frame = frames
            .iter()
            .find(|frame| frame.pane_id == target_pane_id)?;

        frames
            .iter()
            .filter(|frame| {
                frame.pane_id != target_pane_id
                    && target_frame.is_adjacent_candidate(frame, direction)
            })
            .min_by(|lhs, rhs| target_frame.compare_candidates(lhs, rhs, direction))
            .map(|frame| frame.pane_id.clone())
    }

    fn leaf_frames(&self, frame: PaneFrame) -> Vec<PaneFrame> {
        match self.kind {
            PaneTreeKind::Pane => self
                .pane_id
                .as_ref()
                .map(|pane_id| vec![frame.with_pane_id(pane_id)])
                .unwrap_or_default(),
            PaneTreeKind::Split => {
                let children = self.children.as_deref().unwrap_or_default();
                if children.is_empty() {
                    return Vec::new();
                }
                let direction = self.direction.unwrap_or(SplitDirection::Horizontal);
                if children.len() == 2 {
                    let ratio = self.split_ratio();
                    return match direction {
                        SplitDirection::Vertical => {
                            let split_x = frame.min_x + frame.width() * ratio;
                            let mut frames = children[0].leaf_frames(PaneFrame {
                                max_x: split_x,
                                ..frame.clone()
                            });
                            frames.extend(children[1].leaf_frames(PaneFrame {
                                min_x: split_x,
                                ..frame
                            }));
                            frames
                        }
                        SplitDirection::Horizontal => {
                            let split_y = frame.min_y + frame.height() * ratio;
                            let mut frames = children[0].leaf_frames(PaneFrame {
                                max_y: split_y,
                                ..frame.clone()
                            });
                            frames.extend(children[1].leaf_frames(PaneFrame {
                                min_y: split_y,
                                ..frame
                            }));
                            frames
                        }
                    };
                }

                let child_count = children.len() as f64;
                children
                    .iter()
                    .enumerate()
                    .flat_map(|(index, child)| {
                        let start = index as f64 / child_count;
                        let end = (index + 1) as f64 / child_count;
                        match direction {
                            SplitDirection::Vertical => child.leaf_frames(PaneFrame {
                                min_x: frame.min_x + frame.width() * start,
                                max_x: frame.min_x + frame.width() * end,
                                ..frame.clone()
                            }),
                            SplitDirection::Horizontal => child.leaf_frames(PaneFrame {
                                min_y: frame.min_y + frame.height() * start,
                                max_y: frame.min_y + frame.height() * end,
                                ..frame.clone()
                            }),
                        }
                    })
                    .collect()
            }
        }
    }
}

impl Serialize for PaneTreeNode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("PaneTreeNode", 6)?;
        state.serialize_field("node_id", &self.node_id)?;
        state.serialize_field("kind", &self.kind)?;
        if self.direction.is_some() {
            state.serialize_field("direction", &self.direction)?;
        }
        if self.kind == PaneTreeKind::Split {
            state.serialize_field("ratio", &self.split_ratio())?;
        }
        if self.pane_id.is_some() {
            state.serialize_field("pane_id", &self.pane_id)?;
        }
        if self.children.is_some() {
            state.serialize_field("children", &self.children)?;
        }
        state.end()
    }
}

impl<'de> Deserialize<'de> for PaneTreeNode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawPaneTreeNode {
            node_id: String,
            kind: PaneTreeKind,
            direction: Option<SplitDirection>,
            ratio: Option<f64>,
            pane_id: Option<String>,
            children: Option<Vec<PaneTreeNode>>,
        }

        let raw = RawPaneTreeNode::deserialize(deserializer)?;
        match raw.kind {
            PaneTreeKind::Pane => Ok(Self::pane(raw.node_id, raw.pane_id.unwrap_or_default())),
            PaneTreeKind::Split => {
                let ratio = raw.ratio.ok_or_else(|| de::Error::missing_field("ratio"))?;
                Ok(Self::split_with_ratio(
                    raw.node_id,
                    raw.direction.unwrap_or(SplitDirection::Horizontal),
                    ratio,
                    raw.children.unwrap_or_default(),
                ))
            }
        }
    }
}

/// Outcome from resizing a split tree node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneTreeNodeResizeOutcome {
    /// The requested node was found and changed.
    Changed,
    /// The requested node was not found.
    Unchanged,
}

/// Result from resizing a split tree node.
#[derive(Debug, Clone, PartialEq)]
pub struct PaneTreeNodeResizeResult {
    /// Updated tree.
    pub node: PaneTreeNode,
    /// Whether the target split was found.
    pub outcome: PaneTreeNodeResizeOutcome,
}

#[derive(Debug, Clone)]
struct PaneFrame {
    pane_id: String,
    min_x: f64,
    max_x: f64,
    min_y: f64,
    max_y: f64,
}

impl PaneFrame {
    fn unit() -> Self {
        Self {
            pane_id: String::new(),
            min_x: 0.0,
            max_x: 1.0,
            min_y: 0.0,
            max_y: 1.0,
        }
    }

    fn with_pane_id(&self, pane_id: &str) -> Self {
        Self {
            pane_id: pane_id.to_string(),
            ..self.clone()
        }
    }

    fn width(&self) -> f64 {
        (self.max_x - self.min_x).max(0.0)
    }

    fn height(&self) -> f64 {
        (self.max_y - self.min_y).max(0.0)
    }

    fn mid_x(&self) -> f64 {
        (self.min_x + self.max_x) / 2.0
    }

    fn mid_y(&self) -> f64 {
        (self.min_y + self.max_y) / 2.0
    }

    fn is_adjacent_candidate(
        &self,
        candidate: &PaneFrame,
        direction: SpatialFocusDirection,
    ) -> bool {
        let epsilon = 0.000_001;
        if self.perpendicular_overlap(candidate, direction) <= epsilon {
            return false;
        }

        match direction {
            SpatialFocusDirection::Left => candidate.max_x <= self.min_x + epsilon,
            SpatialFocusDirection::Right => candidate.min_x >= self.max_x - epsilon,
            SpatialFocusDirection::Up => candidate.max_y <= self.min_y + epsilon,
            SpatialFocusDirection::Down => candidate.min_y >= self.max_y - epsilon,
        }
    }

    fn compare_candidates(
        &self,
        lhs: &PaneFrame,
        rhs: &PaneFrame,
        direction: SpatialFocusDirection,
    ) -> Ordering {
        let epsilon = 0.000_001;
        let lhs_distance = self.primary_distance(lhs, direction);
        let rhs_distance = self.primary_distance(rhs, direction);
        if (lhs_distance - rhs_distance).abs() > epsilon {
            return lhs_distance
                .partial_cmp(&rhs_distance)
                .unwrap_or(Ordering::Equal);
        }

        let lhs_overlap = self.perpendicular_overlap(lhs, direction);
        let rhs_overlap = self.perpendicular_overlap(rhs, direction);
        if (lhs_overlap - rhs_overlap).abs() > epsilon {
            return rhs_overlap
                .partial_cmp(&lhs_overlap)
                .unwrap_or(Ordering::Equal);
        }

        let lhs_center = self.perpendicular_center_distance(lhs, direction);
        let rhs_center = self.perpendicular_center_distance(rhs, direction);
        if (lhs_center - rhs_center).abs() > epsilon {
            return lhs_center
                .partial_cmp(&rhs_center)
                .unwrap_or(Ordering::Equal);
        }

        lhs.pane_id.cmp(&rhs.pane_id)
    }

    fn primary_distance(&self, candidate: &PaneFrame, direction: SpatialFocusDirection) -> f64 {
        match direction {
            SpatialFocusDirection::Left => (self.min_x - candidate.max_x).max(0.0),
            SpatialFocusDirection::Right => (candidate.min_x - self.max_x).max(0.0),
            SpatialFocusDirection::Up => (self.min_y - candidate.max_y).max(0.0),
            SpatialFocusDirection::Down => (candidate.min_y - self.max_y).max(0.0),
        }
    }

    fn perpendicular_overlap(
        &self,
        candidate: &PaneFrame,
        direction: SpatialFocusDirection,
    ) -> f64 {
        match direction {
            SpatialFocusDirection::Left | SpatialFocusDirection::Right => {
                (self.max_y.min(candidate.max_y) - self.min_y.max(candidate.min_y)).max(0.0)
            }
            SpatialFocusDirection::Up | SpatialFocusDirection::Down => {
                (self.max_x.min(candidate.max_x) - self.min_x.max(candidate.min_x)).max(0.0)
            }
        }
    }

    fn perpendicular_center_distance(
        &self,
        candidate: &PaneFrame,
        direction: SpatialFocusDirection,
    ) -> f64 {
        match direction {
            SpatialFocusDirection::Left | SpatialFocusDirection::Right => {
                (self.mid_y() - candidate.mid_y()).abs()
            }
            SpatialFocusDirection::Up | SpatialFocusDirection::Down => {
                (self.mid_x() - candidate.mid_x()).abs()
            }
        }
    }
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
