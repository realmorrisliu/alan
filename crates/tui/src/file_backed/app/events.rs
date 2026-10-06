//! Renderer notifications correlated to the owning Process where required.
use super::*;

pub(in crate::file_backed) enum FileBackedEvent {
    ActionDetails {
        path: String,
        generation: u64,
        ids: Result<Vec<String>, String>,
        id: Option<String>,
        rows: Vec<Line<'static>>,
    },
    ModelSelectionWritten {
        owner: String,
        id: String,
        success: bool,
        owner_current: bool,
    },
    ObservationRead {
        revision: u64,
        owner: String,
        snapshot: super::super::observation_io::Snapshot,
    },
    ProjectHostCompleted {
        owner: String,
        command: ProjectControl,
        result: Result<super::super::ProjectControlResult, String>,
    },
    ProjectCwdWritten {
        owner: String,
        id: String,
        owner_current: bool,
        result: Result<(), String>,
    },
    ProjectGrantObserved {
        grant_id: String,
        active: Option<bool>,
    },
    Terminal(TerminalEvent),
    Output(String),
    ResumeWriteCompleted {
        request_id: String,
        retry_input: String,
        result: Result<(), String>,
    },
    ControlWriteCompleted {
        success_notice: String,
        error_prefix: String,
        result: Result<(), String>,
    },
    RootAgentPidRefresh(Result<Option<u64>, String>),
    QueueChanged {
        owner: String,
    },
    QueueUnavailable {
        owner: String,
    },
    ModelReceiptsUnavailable {
        owner: String,
    },
    ModelReceipt {
        owner: String,
        event: UiEvent,
    },
    ModelChanged {
        owner: String,
    },
    ModelUnavailable {
        owner: String,
    },
    SkillsChanged {
        owner: String,
    },
    SkillsUnavailable {
        owner: String,
    },
    RequestsChanged,
    ActionsChanged {
        agent_path: String,
        action_id: String,
    },
    Ui(UiEvent),
    Tape(TapeRecordV1),
    Error(String),
    TerminalError(String),
}
