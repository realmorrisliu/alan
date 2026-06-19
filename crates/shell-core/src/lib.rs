//! Platform-neutral shell workspace domain core.
//!
//! `alan-shell-core` owns reusable shell workspace domain semantics that can be
//! shared by macOS, Linux, and future desktop clients. The crate intentionally
//! avoids platform UI, terminal renderer, daemon hosting, filesystem location,
//! clipboard, file picker, and privileged OS executor dependencies.
//!
//! The public API starts with coarse-grained request/response envelopes and a
//! parity fixture harness. Typed workspace model, reducer, manifest, action,
//! control, Terminal Profile, and settings-summary modules are added behind the
//! same platform-neutral boundary as each migration slice lands.

mod actions;
mod control;
mod envelope;
mod fixtures;
mod manifest;
mod model;
mod reducer;
mod settings_summary;
mod terminal_profile;

pub use actions::{
    ShellActionAvailability, ShellActionDescriptor, ShellActionEffect, ShellActionExecutionResult,
    ShellActionId, ShellActionModifier, ShellActionRegistry, ShellActionRegistryError,
    ShellActionShortcut, ShellActionShortcutContext, ShellActionTarget, ShellActionTargetKind,
    ShellKeyboardAction, ShellResolvedAction, ShellResolvedActionTarget, ShellWorkspaceCommand,
};
pub use control::{
    ShellControlCommand, ShellControlCommandKind, ShellControlResponse, ShellControlResult,
    ShellControlRuntimeIntent, TerminalControlKey,
};
pub use envelope::{
    EnvelopeVersion, ShellCoreErrorCode, ShellCoreErrorEnvelope, ShellCoreRequestEnvelope,
    ShellCoreResponseEnvelope,
};
pub use fixtures::{FixtureCase, FixtureCorpus, FixtureError, FixtureKind, FixtureSource};
pub use manifest::{
    ShellContentRestoreRecord, ShellContentTabRestoreSnapshot, ShellContentWorkspaceManifest,
    ShellContentWorkspaceSpaceRecord, ShellContentWorkspaceTabRecord, ShellPaneRestoreRecord,
    ShellPaneSlotRestoreRecord, ShellQuickTerminalRestoreRecord, ShellTabRestoreSnapshot,
    ShellWorkspaceManifest, ShellWorkspaceSpaceRecord, ShellWorkspaceTabRecord,
};
pub use model::{
    ContentCapability, ContentInstance, ContentKind, ContentLifecycleState, PaneSlot, PaneTreeKind,
    PaneTreeNode, PaneTreeNodeResizeOutcome, PaneTreeNodeResizeResult, ShellAttentionState,
    ShellContentPayload, ShellLaunchTarget, ShellQuickTerminalPresentation,
    ShellQuickTerminalState, ShellTabActiveTaskState, ShellTerminalContentPayload, Space,
    SpatialFocusDirection, SplitDirection, SplitPlacement, Tab, TabKind, TabOrganizationSection,
    TerminalActivityAgentMetadata, TerminalActivityDisplay, TerminalActivityFreshness,
    TerminalActivityPriority, TerminalActivitySnapshot, TerminalActivitySource,
    TerminalActivitySourceKind, TerminalActivityStatus, TerminalRuntimeMetadata, WorkspaceState,
};
pub use reducer::{
    DomainEvent, ManifestSyncHint, ReducerChangedIds, ReducerError, ReducerErrorCode, ReducerFocus,
    ReducerOperation, ReducerResult, RuntimeIntent,
};
pub use settings_summary::{
    ManagedTerminalAccountSettingsSummary, ShellSettingsCapabilitiesSummary,
    ShellSettingsDiagnosticsSummary, ShellSettingsLocalSummary, ShellSettingsRowMutability,
    ShellSettingsRowSummary, ShellSettingsSkillSummary, ShellSettingsSummaryRows,
    ShellSettingsWorkspaceContext, ShellSettingsWorkspaceContextInput,
    ShellSettingsWorkspaceRegistryEntry, TerminalProfileSettingsSummary,
};
pub use terminal_profile::{
    ManagedTerminalAccountApplyResult, ManagedTerminalAccountFakeExecutor,
    ManagedTerminalAccountIdentifierValidator, ManagedTerminalAccountPlan,
    ManagedTerminalAccountPlanStatus, ManagedTerminalAccountPlanStep,
    ManagedTerminalAccountPlanStepKind, ManagedTerminalAccountPlanner,
    ManagedTerminalAccountProfileHandoff, ManagedTerminalAccountProfileState,
    ManagedTerminalAccountRecord, ManagedTerminalAccountRequest, ManagedTerminalAccountState,
    ManagedTerminalAccountSudoersRule, ManagedTerminalAccountSudoersState,
    ManagedTerminalAccountValidationError, ManagedTerminalAccountVerificationStatus,
    ManagedTerminalAccountVerificationStep, TerminalExecutableAvailability,
    TerminalLaunchEnvironment, TerminalLaunchIntent, TerminalLaunchStrategy,
    TerminalProfileDefinition, TerminalProfileDocument, TerminalProfileDocumentEditorResult,
    TerminalProfileEditor, TerminalProfileEditorDraft, TerminalProfileEditorResult,
    TerminalProfileLaunch, TerminalProfileLaunchKind, TerminalProfilePresentation,
    TerminalProfileResolutionState, TerminalProfileValidationError,
    TerminalProfileValidationResult, TerminalProfileValidator, shell_quoted,
    should_capture_global_default_terminal_profile,
};
