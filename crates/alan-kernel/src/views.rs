use crate::{ArtifactId, BufferId, CommandId, EvidenceId, ObjectId, TaskId, ViewId, ViewKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Renderer-independent semantic snapshot for one view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewSnapshot {
    /// View being snapshotted.
    pub view_id: ViewId,
    /// Buffer backing the view.
    pub buffer_id: BufferId,
    /// Monotonic semantic snapshot version.
    pub version: u64,
    /// View class.
    pub kind: ViewKind,
    /// Typed semantic model.
    pub model: ViewModel,
    /// Commands exposed by this snapshot.
    pub actions: Vec<ViewAction>,
    /// Diagnostics relevant to semantic content.
    pub diagnostics: Vec<ViewDiagnostic>,
    /// Semantic selection state.
    pub selection: Option<ViewSelection>,
    /// Semantic focus state.
    pub focus: Option<ViewFocus>,
    /// Additional semantic state shared across hosts.
    pub semantic_state: ViewSemanticState,
}

/// Host-neutral command action exposed by a snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ViewAction {
    /// Command to invoke.
    pub command_id: CommandId,
    /// User-visible label.
    pub label: String,
    /// Whether the action is currently enabled.
    pub enabled: bool,
}

/// Semantic diagnostic for a view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ViewDiagnostic {
    /// Diagnostic severity.
    pub severity: ViewDiagnosticSeverity,
    /// Diagnostic message.
    pub message: String,
    /// Optional target path or semantic anchor.
    pub target: Option<String>,
}

/// Semantic diagnostic severity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewDiagnosticSeverity {
    /// Informational diagnostic.
    Info,
    /// Warning diagnostic.
    Warning,
    /// Error diagnostic.
    Error,
}

/// Semantic selection state that another host or agent can inspect.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ViewSelection {
    /// Selected semantic anchor.
    pub anchor: String,
    /// Optional selection extent.
    pub extent: Option<String>,
}

/// Semantic focus state that another host or agent can inspect.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ViewFocus {
    /// Focused semantic element id.
    pub element_id: String,
    /// Optional focus mode.
    pub mode: Option<String>,
}

/// Semantic view state shared across renderer hosts.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ViewSemanticState {
    /// Optional filter text.
    pub filter: Option<String>,
    /// Optional scroll anchor that has semantic meaning.
    pub scroll_anchor: Option<String>,
    /// Expanded semantic node ids.
    pub expanded: Vec<String>,
    /// Active semantic mode.
    pub mode: Option<String>,
}

/// Built-in and dynamic semantic view models.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ViewModel {
    /// Conversation model.
    Conversation(ConversationViewModel),
    /// Task tree model.
    TaskTree(TaskTreeViewModel),
    /// Command palette model.
    CommandPalette(CommandPaletteViewModel),
    /// Form or approval model.
    Form(FormViewModel),
    /// Object list model.
    ObjectList(ObjectListViewModel),
    /// Text document read or review model.
    TextDocument(TextDocumentViewModel),
    /// Diff model.
    Diff(DiffViewModel),
    /// Log stream model.
    LogStream(LogStreamViewModel),
    /// Schema-versioned dynamic extension model.
    Dynamic(DynamicViewPayload),
}

/// Conversation semantic model.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConversationViewModel {
    /// Ordered conversation blocks.
    pub blocks: Vec<ConversationBlock>,
}

/// Conversation block.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConversationBlock {
    /// Stable block id.
    pub id: String,
    /// Block kind.
    pub kind: ConversationBlockKind,
    /// Text content or summary.
    pub text: String,
    /// Linked task, if any.
    pub task_id: Option<TaskId>,
    /// Linked artifact, if any.
    pub artifact_id: Option<ArtifactId>,
    /// Linked evidence records.
    pub evidence_ids: Vec<EvidenceId>,
}

/// Conversation block kind.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationBlockKind {
    /// User input.
    User,
    /// Assistant output.
    Assistant,
    /// Thinking or reasoning summary.
    Thinking,
    /// Tool summary.
    Tool,
    /// Yield or pending input.
    Yield,
    /// Error summary.
    Error,
    /// Artifact reference.
    Artifact,
}

/// Task tree semantic model.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TaskTreeViewModel {
    /// Root task nodes.
    pub roots: Vec<TaskTreeNode>,
}

/// Task tree node.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TaskTreeNode {
    /// Task id.
    pub task_id: TaskId,
    /// Display label.
    pub label: String,
    /// Status label supplied by projection.
    pub status: String,
    /// Child task nodes.
    pub children: Vec<TaskTreeNode>,
}

/// Command palette semantic model.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandPaletteViewModel {
    /// Current query text.
    pub query: String,
    /// Palette entries.
    pub entries: Vec<CommandPaletteEntry>,
}

/// Command palette entry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandPaletteEntry {
    /// Command id.
    pub command_id: CommandId,
    /// Display title.
    pub title: String,
    /// Optional subtitle.
    pub subtitle: Option<String>,
    /// Whether the command is enabled.
    pub enabled: bool,
}

/// Form semantic model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FormViewModel {
    /// Form title.
    pub title: String,
    /// Form fields.
    pub fields: Vec<FormField>,
    /// Submit command.
    pub submit_command_id: Option<CommandId>,
}

/// Form field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FormField {
    /// Stable field id.
    pub id: String,
    /// Field label.
    pub label: String,
    /// Field kind.
    pub kind: FormFieldKind,
    /// Current value.
    pub value: Value,
    /// Whether the field is required.
    pub required: bool,
}

/// Form field kind.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormFieldKind {
    /// Single-line text.
    Text,
    /// Multi-line text.
    TextArea,
    /// Boolean checkbox.
    Boolean,
    /// Single choice.
    Select,
    /// Structured JSON payload.
    Json,
}

/// Object list semantic model.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ObjectListViewModel {
    /// Listed objects.
    pub objects: Vec<ObjectListItem>,
}

/// Object list item.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ObjectListItem {
    /// Object id.
    pub object_id: ObjectId,
    /// Display title.
    pub title: String,
    /// Optional subtitle.
    pub subtitle: Option<String>,
}

/// Text document read or review model.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TextDocumentViewModel {
    /// Document title.
    pub title: String,
    /// Document text.
    pub text: String,
    /// Whether the document is editable through this view.
    pub editable: bool,
}

/// Diff semantic model.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiffViewModel {
    /// Files in the diff.
    pub files: Vec<DiffFile>,
}

/// Diff file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiffFile {
    /// File path.
    pub path: String,
    /// Diff hunks.
    pub hunks: Vec<DiffHunk>,
}

/// Diff hunk.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiffHunk {
    /// Optional hunk header.
    pub header: Option<String>,
    /// Lines in the hunk.
    pub lines: Vec<DiffLine>,
}

/// Diff line.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiffLine {
    /// Context line.
    Context { text: String },
    /// Added line.
    Added { text: String },
    /// Removed line.
    Removed { text: String },
}

/// Log stream semantic model.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LogStreamViewModel {
    /// Ordered log entries.
    pub entries: Vec<LogEntry>,
}

/// Log entry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    /// Timestamp in Unix milliseconds.
    pub timestamp_ms: u64,
    /// Log level.
    pub level: String,
    /// Log message.
    pub message: String,
}

/// Schema-versioned dynamic extension payload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DynamicViewPayload {
    /// Stable schema id.
    pub schema_id: String,
    /// Schema version.
    pub schema_version: u16,
    /// Bounded dynamic payload.
    pub payload: Value,
}

#[cfg(test)]
mod tests {
    use super::{
        CommandPaletteEntry, CommandPaletteViewModel, ConversationBlock, ConversationBlockKind,
        ConversationViewModel, DynamicViewPayload, FormField, FormFieldKind, FormViewModel,
        TaskTreeNode, TaskTreeViewModel, ViewModel, ViewSemanticState, ViewSnapshot,
    };
    use crate::{BufferId, CommandId, ViewId, ViewKind};

    #[test]
    fn conversation_form_task_tree_and_command_palette_snapshots_round_trip() {
        let buffer_id = BufferId::new();
        let snapshots = vec![
            ViewSnapshot {
                view_id: ViewId::new(),
                buffer_id,
                version: 1,
                kind: ViewKind::Conversation,
                model: ViewModel::Conversation(ConversationViewModel {
                    blocks: vec![ConversationBlock {
                        id: "b1".to_string(),
                        kind: ConversationBlockKind::Assistant,
                        text: "Done".to_string(),
                        task_id: None,
                        artifact_id: None,
                        evidence_ids: Vec::new(),
                    }],
                }),
                actions: Vec::new(),
                diagnostics: Vec::new(),
                selection: None,
                focus: None,
                semantic_state: ViewSemanticState::default(),
            },
            ViewSnapshot {
                view_id: ViewId::new(),
                buffer_id,
                version: 1,
                kind: ViewKind::Form,
                model: ViewModel::Form(FormViewModel {
                    title: "Approval".to_string(),
                    fields: vec![FormField {
                        id: "approve".to_string(),
                        label: "Approve".to_string(),
                        kind: FormFieldKind::Boolean,
                        value: serde_json::json!(false),
                        required: true,
                    }],
                    submit_command_id: Some(CommandId::new()),
                }),
                actions: Vec::new(),
                diagnostics: Vec::new(),
                selection: None,
                focus: None,
                semantic_state: ViewSemanticState::default(),
            },
            ViewSnapshot {
                view_id: ViewId::new(),
                buffer_id,
                version: 1,
                kind: ViewKind::TaskTree,
                model: ViewModel::TaskTree(TaskTreeViewModel {
                    roots: vec![TaskTreeNode {
                        task_id: crate::TaskId::new(),
                        label: "Run".to_string(),
                        status: "running".to_string(),
                        children: Vec::new(),
                    }],
                }),
                actions: Vec::new(),
                diagnostics: Vec::new(),
                selection: None,
                focus: None,
                semantic_state: ViewSemanticState::default(),
            },
            ViewSnapshot {
                view_id: ViewId::new(),
                buffer_id,
                version: 1,
                kind: ViewKind::CommandPalette,
                model: ViewModel::CommandPalette(CommandPaletteViewModel {
                    query: "open".to_string(),
                    entries: vec![CommandPaletteEntry {
                        command_id: CommandId::new(),
                        title: "Open".to_string(),
                        subtitle: None,
                        enabled: true,
                    }],
                }),
                actions: Vec::new(),
                diagnostics: Vec::new(),
                selection: None,
                focus: None,
                semantic_state: ViewSemanticState::default(),
            },
        ];

        for snapshot in snapshots {
            let json = serde_json::to_string(&snapshot).expect("serialize snapshot");
            let decoded: ViewSnapshot = serde_json::from_str(&json).expect("deserialize snapshot");
            assert_eq!(decoded, snapshot);
        }
    }

    #[test]
    fn dynamic_view_payload_is_schema_versioned() {
        let model = ViewModel::Dynamic(DynamicViewPayload {
            schema_id: "groove.master.practice".to_string(),
            schema_version: 1,
            payload: serde_json::json!({"tempo": 96}),
        });

        let json = serde_json::to_value(&model).expect("serialize dynamic model");

        assert_eq!(json["schema_id"], "groove.master.practice");
        assert_eq!(json["schema_version"], 1);
    }

    #[test]
    fn snapshot_keeps_host_render_state_out_of_kernel_model() {
        let snapshot = ViewSnapshot {
            view_id: ViewId::new(),
            buffer_id: BufferId::new(),
            version: 1,
            kind: ViewKind::Conversation,
            model: ViewModel::Conversation(ConversationViewModel { blocks: Vec::new() }),
            actions: Vec::new(),
            diagnostics: Vec::new(),
            selection: None,
            focus: None,
            semantic_state: ViewSemanticState {
                filter: None,
                scroll_anchor: Some("block:b1".to_string()),
                expanded: Vec::new(),
                mode: None,
            },
        };

        let json = serde_json::to_string(&snapshot).expect("serialize snapshot");

        assert!(json.contains("scroll_anchor"));
        assert!(!json.contains("cell_cache"));
        assert!(!json.contains("pixel_geometry"));
        assert!(!json.contains("frame_timing"));
    }
}
