//! Renderer notifications correlated to the owning Process where required.
use super::*;

#[derive(Debug)]
pub(in crate::file_backed) enum FileBackedAction {
    Submit(alan_agent_protocol::UserInputRecord),
    Resume {
        request_id: String,
        response: String,
        retry_input: String,
    },
    MachineCtl {
        command: String,
        success_notice: String,
    },
    SelectModel {
        owner: String,
        id: String,
        op: alan_agent_protocol::Op,
    },
    Project(ProjectControl),
    RevokeCurrentProject,
    OpenExternalEditor,
    ReadTape,
    ReadMan {
        topic: String,
        generation: u64,
    },
    ReadHistoryCatalog {
        generation: u64,
    },
    SwitchRollout {
        rollout_id: Option<String>,
    },
    InspectProjectSummary,
    EditProjectSummary {
        text: String,
        expected_revision: Option<u64>,
    },
    DeleteProjectSummary {
        expected_revision: Option<u64>,
    },
    LoginListProviders {
        generation: u64,
    },
    LoginSubmit {
        generation: u64,
        submission: super::super::login::LoginSubmission,
        progress_rx:
            Option<tokio::sync::mpsc::UnboundedReceiver<super::super::login::LoginProgress>>,
    },
    CancelLogin,
    Suspend,
    Interrupt,
    Quit,
}

pub(in crate::file_backed) enum FileBackedEvent {
    PlanDetails {
        path: String,
        generation: u64,
        entries: Vec<super::super::plan_detail_io::PlanEntry>,
    },
    TapeDetails {
        path: String,
        generation: u64,
        rows: Vec<Line<'static>>,
    },
    ManDetails {
        generation: u64,
        owner: String,
        content: String,
    },
    HistoryCatalogLoaded {
        generation: u64,
        result: Result<super::super::resume_picker::RolloutHistoryCatalogView, String>,
    },
    HistorySwitchCompleted {
        rollout_id: Option<String>,
        result: Result<(), String>,
    },
    ProjectSummaryResult {
        action_label: Option<&'static str>,
        result: Result<super::super::summary_view::ProjectSummaryViewDto, String>,
    },
    LoginProvidersLoaded {
        generation: u64,
        result: Result<super::super::login::LoginResult, String>,
    },
    LoginProgress {
        generation: u64,
        progress: super::super::login::LoginProgress,
    },
    LoginStepCompleted {
        generation: u64,
        result: Result<super::super::login::LoginResult, String>,
    },
    ActionDetails {
        path: String,
        generation: u64,
        actions: Result<Vec<super::super::action_detail_io::ActionEntry>, String>,
        selected: Option<super::super::action_detail_io::ActionEntry>,
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
    ProjectGrantLocated {
        owner: String,
        cwd: std::path::PathBuf,
        result: Result<Option<String>, String>,
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
    GitStatusUpdated {
        root: std::path::PathBuf,
        status: Option<super::super::git_status::GitStatus>,
    },
    FileIndexUpdated {
        root: std::path::PathBuf,
        candidates: Vec<crate::completion::CompletionCandidate>,
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
    UsageChanged {
        owner: String,
    },
    UsageUnavailable {
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
