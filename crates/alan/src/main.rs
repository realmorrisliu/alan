//! alan — AI Turing Machine CLI & daemon.
//!
//! This is the unified entry point for all alan operations:
//! - `alan daemon start` — run the workspace daemon
//! - `alan init` — initialize a workspace
//! - `alan workspace` — manage workspaces

mod cli;
mod daemon;
mod host_config;
pub mod registry;
mod skill_catalog;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::Level;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "alan", about = "alan — AI Turing Machine", version)]
struct Cli {
    /// Select a named agent root on top of the base workspace/global agent
    #[arg(long, global = true)]
    agent: Option<String>,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage connection profiles and credentials
    Connection {
        #[command(subcommand)]
        action: ConnectionAction,
    },
    /// Start or manage the daemon server
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Initialize a directory as a workspace
    Init {
        /// Path to initialize (defaults to current directory)
        #[arg(long)]
        path: Option<PathBuf>,
        /// Workspace alias (defaults to directory name)
        #[arg(long)]
        name: Option<String>,
        /// Suppress output (used by install script)
        #[arg(long, hide = true)]
        silent: bool,
    },
    /// Manage workspaces
    Workspace {
        #[command(subcommand)]
        action: WorkspaceAction,
    },
    /// Inspect resolved skills, packages, and exposure state
    Skills {
        #[command(subcommand)]
        action: SkillsAction,
    },
    /// Control a local `alan shell` host via IPC
    Shell {
        #[command(subcommand)]
        action: ShellAction,
    },
}

#[derive(Subcommand)]
enum DaemonAction {
    /// Start the daemon server (default: detach to background)
    Start {
        /// Run in foreground instead of detaching
        #[arg(long)]
        foreground: bool,
    },
    /// Stop the daemon
    Stop,
    /// Show daemon status
    Status,
    /// Emit the daemon API contract for generated clients
    #[command(name = "api-contract", hide = true)]
    ApiContract,
}

#[derive(Subcommand)]
enum ConnectionAction {
    /// List configured connection profiles
    List,
    /// Show one connection profile and credential status
    Show { profile_id: String },
    /// Show the effective connection selection state
    Current {
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Add a new connection profile
    Add {
        provider: String,
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        label: Option<String>,
        #[arg(long)]
        credential: Option<String>,
        #[arg(long = "setting")]
        settings: Vec<String>,
        #[arg(long)]
        default: bool,
        #[arg(long, hide = true)]
        activate: bool,
    },
    /// Edit an existing connection profile
    Edit {
        profile_id: String,
        #[arg(long)]
        label: Option<String>,
        #[arg(long)]
        credential: Option<String>,
        #[arg(long = "setting")]
        settings: Vec<String>,
    },
    /// Store or replace a secret credential for a profile
    SetSecret {
        profile_id: String,
        #[arg(long)]
        value: Option<String>,
    },
    /// Log in to a managed provider profile
    Login {
        profile_id: String,
        #[arg(value_enum, default_value_t = ConnectionLoginMode::Browser)]
        mode: ConnectionLoginMode,
        #[arg(long = "no-browser")]
        no_browser: bool,
    },
    /// Remove stored credentials for a profile
    Logout { profile_id: String },
    /// Manage the default profile for future sessions
    Default {
        #[command(subcommand)]
        action: ConnectionDefaultAction,
    },
    /// Pin the effective profile in an agent config file
    Pin {
        profile_id: String,
        #[arg(long, value_enum, default_value_t = ConnectionPinScopeArg::Global)]
        scope: ConnectionPinScopeArg,
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Remove a profile pin from an agent config file
    Unpin {
        #[arg(long, value_enum, default_value_t = ConnectionPinScopeArg::Global)]
        scope: ConnectionPinScopeArg,
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Validate one connection profile
    Test { profile_id: Option<String> },
    /// Remove a connection profile
    Remove { profile_id: String },
    /// Deprecated alias for `connection default set`
    #[command(hide = true)]
    Activate { profile_id: String },
    /// Deprecated alias for `connection default set`
    #[command(hide = true)]
    Use { profile_id: String },
    /// Deprecated alias for `connection show/current`
    #[command(hide = true)]
    Status {
        profile_id: Option<String>,
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum ConnectionDefaultAction {
    /// Set the default profile for future sessions
    Set {
        profile_id: String,
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Clear the default profile for future sessions
    Clear {
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
}

#[derive(clap::ValueEnum, Clone, Copy, Default)]
enum ConnectionLoginMode {
    #[default]
    Browser,
    Device,
}

#[derive(clap::ValueEnum, Clone, Copy, Default)]
enum ConnectionPinScopeArg {
    #[default]
    Global,
    Workspace,
}

impl From<ConnectionPinScopeArg> for crate::daemon::connection_control::ConnectionPinScope {
    fn from(value: ConnectionPinScopeArg) -> Self {
        match value {
            ConnectionPinScopeArg::Global => Self::Global,
            ConnectionPinScopeArg::Workspace => Self::Workspace,
        }
    }
}

#[derive(Subcommand)]
enum WorkspaceAction {
    /// List all registered workspaces
    List,
    /// Register an existing workspace directory
    Add {
        /// Path to the workspace directory (must contain .alan/)
        path: PathBuf,
        /// Workspace alias
        #[arg(long)]
        name: Option<String>,
    },
    /// Unregister a workspace (does not delete files)
    Remove {
        /// Workspace alias, short ID, or path
        workspace: String,
    },
    /// Show workspace details
    Info {
        /// Workspace alias, short ID, or path
        workspace: String,
    },
}

#[derive(Subcommand)]
enum SkillsAction {
    /// List exposed skills for the resolved workspace/agent
    List {
        /// Workspace directory to inspect (defaults to current directory)
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// List resolved capability packages and their exported skills
    Packages {
        /// Workspace directory to inspect (defaults to current directory)
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Scaffold a new skill package from a first-party template
    Init {
        /// Directory to create for the new skill package
        path: PathBuf,
        /// Template shape to generate
        #[arg(long, value_enum, default_value_t = cli::skill_authoring::SkillTemplateKind::Inline)]
        template: cli::skill_authoring::SkillTemplateKind,
        /// Human-facing skill name written into SKILL.md
        #[arg(long)]
        name: Option<String>,
        /// Skill description written into SKILL.md
        #[arg(long)]
        description: Option<String>,
        /// Short UI-facing description
        #[arg(long = "short-description")]
        short_description: Option<String>,
        /// Overwrite an existing non-empty directory
        #[arg(long)]
        force: bool,
    },
    /// Validate a skill package against alan's current package contract
    Validate {
        /// Skill package directory (defaults to current directory)
        path: Option<PathBuf>,
        /// Emit structured JSON instead of human-readable text
        #[arg(long)]
        json: bool,
        /// Treat warnings as failures
        #[arg(long)]
        strict: bool,
    },
    /// Run explicit package-local evaluation hooks for a skill package
    Eval {
        /// Skill package directory (defaults to current directory)
        path: Option<PathBuf>,
        /// Structured eval manifest path (defaults to evals/evals.json when present)
        #[arg(long)]
        manifest: Option<PathBuf>,
        /// Output directory for structured eval artifacts
        #[arg(long = "output-dir")]
        output_dir: Option<PathBuf>,
        /// Fail if the package does not define an eval hook
        #[arg(long)]
        require_hook: bool,
    },
    /// Recompute benchmark.json for an existing structured eval run directory
    AggregateBenchmark {
        /// Eval run directory containing run.json
        run_dir: PathBuf,
    },
    /// Rebuild the static review bundle for an existing structured eval run directory
    GenerateReview {
        /// Eval run directory containing run.json
        run_dir: PathBuf,
    },
    /// Prepare SWE-bench Lite workspaces for a curated instance list
    #[command(name = "swebench-lite-prepare-workspaces", hide = true)]
    SwebenchLitePrepareWorkspaces(SwebenchLitePrepareWorkspacesArgs),
    /// Materialize a SWE-bench Lite subset suite manifest
    #[command(name = "swebench-lite-materialize-subset", hide = true)]
    SwebenchLiteMaterializeSubset(SwebenchLiteMaterializeSubsetArgs),
}

#[derive(Args)]
struct SwebenchLitePrepareWorkspacesArgs {
    #[arg(long = "instance-ids-file")]
    instance_ids_file: PathBuf,
    #[arg(long = "dataset-file")]
    dataset_files: Vec<PathBuf>,
    #[arg(long = "dataset-name")]
    dataset_name: Option<String>,
    #[arg(long, default_value = "test")]
    split: String,
    #[arg(long = "workspace-root")]
    workspace_root: PathBuf,
    #[arg(long = "repo-cache-root")]
    repo_cache_root: Option<PathBuf>,
    #[arg(long = "github-root", default_value = "https://github.com")]
    github_root: String,
    #[arg(long = "workspace-map-output")]
    workspace_map_output: Option<PathBuf>,
    #[arg(long = "skip-mirror-fetch", default_value_t = false)]
    skip_mirror_fetch: bool,
    #[arg(long = "reuse-existing-workspaces", default_value_t = false)]
    reuse_existing_workspaces: bool,
}

#[derive(Args)]
struct SwebenchLiteMaterializeSubsetArgs {
    #[arg(long = "instance-ids-file")]
    instance_ids_file: PathBuf,
    #[arg(long = "dataset-file")]
    dataset_files: Vec<PathBuf>,
    #[arg(long = "dataset-name")]
    dataset_name: Option<String>,
    #[arg(long, default_value = "test")]
    split: String,
    #[arg(long = "workspace-root")]
    workspace_root: Option<PathBuf>,
    #[arg(long = "workspace-map-file")]
    workspace_map_file: Option<PathBuf>,
    #[arg(long = "output-dir")]
    output_dir: PathBuf,
    #[arg(long = "suite-name", default_value = "swebench_lite_pilot_v1")]
    suite_name: String,
    #[arg(long = "dataset-label", default_value = "SWE-bench Lite")]
    dataset_label: String,
    #[arg(
        long = "scoring-dataset-name",
        default_value = "princeton-nlp/SWE-bench_Lite"
    )]
    scoring_dataset_name: String,
    #[arg(long = "max-workers", default_value_t = 4)]
    max_workers: usize,
    #[arg(long = "timeout-secs", default_value_t = 1800)]
    timeout_secs: u64,
    #[arg(long = "allow-missing-workspaces", default_value_t = false)]
    allow_missing_workspaces: bool,
}

#[derive(Args, Clone)]
struct ShellTargetArgs {
    /// Explicit `alan shell` socket path
    #[arg(long)]
    socket: Option<PathBuf>,
    /// Explicit `alan shell` control directory
    #[arg(long = "control-dir")]
    control_dir: Option<PathBuf>,
    /// Window id used to derive the local alan shell control directory
    #[arg(long)]
    window: Option<String>,
    /// Timeout for IPC requests in milliseconds
    #[arg(long, default_value_t = 3000)]
    timeout_ms: u64,
}

#[derive(Subcommand)]
enum ShellAction {
    /// Print the canonical shell state snapshot
    State {
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Operate on shell spaces
    Space {
        #[command(subcommand)]
        action: ShellSpaceAction,
    },
    /// Operate on shell tabs
    Tab {
        #[command(subcommand)]
        action: ShellTabAction,
    },
    /// Operate on shell panes
    Pane {
        #[command(subcommand)]
        action: ShellPaneAction,
    },
    /// Operate on terminal content
    Terminal {
        #[command(subcommand)]
        action: ShellTerminalAction,
    },
    /// Attention inbox and overrides
    Attention {
        #[command(subcommand)]
        action: ShellAttentionAction,
    },
    /// Rank candidate panes for shell routing
    Routing {
        #[command(subcommand)]
        action: ShellRoutingAction,
    },
    /// Read shell events or follow the event stream
    Events {
        /// Resume after this event id
        #[arg(long = "after-event-id")]
        after_event_id: Option<String>,
        /// Maximum number of events per read
        #[arg(long)]
        limit: Option<u64>,
        /// Keep polling and emit NDJSON as new events arrive
        #[arg(long)]
        follow: bool,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
}

#[derive(Subcommand)]
enum ShellSpaceAction {
    /// List spaces
    List {
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Create a new space
    Create {
        /// Optional title for the space
        #[arg(long)]
        title: Option<String>,
        /// Optional working directory for the initial pane
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Open a new alan space directly
    OpenAlan {
        /// Optional title for the space
        #[arg(long)]
        title: Option<String>,
        /// Optional working directory for the initial pane
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
}

#[derive(Subcommand)]
enum ShellTabAction {
    /// List tabs
    List {
        /// Restrict to a specific space
        #[arg(long)]
        space: Option<String>,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Open a new tab
    Open {
        /// Target space id
        #[arg(long)]
        space: Option<String>,
        /// Optional tab title
        #[arg(long)]
        title: Option<String>,
        /// Optional working directory for the initial pane
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Close a tab
    Close {
        /// Tab id to close
        #[arg(long)]
        tab: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
}

#[derive(Subcommand)]
enum ShellPaneAction {
    /// List panes
    List {
        /// Restrict to a specific tab
        #[arg(long)]
        tab: Option<String>,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Print a single pane snapshot
    Snapshot {
        /// Pane id to inspect
        #[arg(long)]
        pane: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Split a pane
    Split {
        /// Pane id to split
        #[arg(long)]
        pane: String,
        /// Split direction
        #[arg(long, value_parser = ["horizontal", "vertical"])]
        direction: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Close a pane
    Close {
        /// Pane id to close
        #[arg(long)]
        pane: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Move a pane into its own tab
    Lift {
        /// Pane id to lift
        #[arg(long)]
        pane: String,
        /// Optional title for the new tab
        #[arg(long)]
        title: Option<String>,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Move a pane into another existing tab
    Move {
        /// Pane id to move
        #[arg(long)]
        pane: String,
        /// Target tab id
        #[arg(long)]
        tab: String,
        /// Split direction used when attaching onto the destination tab
        #[arg(long, default_value = "vertical", value_parser = ["horizontal", "vertical"])]
        direction: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Move a pane inside its current tab
    MoveWithinTab {
        /// Pane id to move
        #[arg(long)]
        pane: String,
        /// Placement relative to the adjacent pane
        #[arg(long, value_parser = ["left", "right", "up", "down"])]
        placement: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Focus a pane
    Focus {
        /// Pane id to focus
        #[arg(long)]
        pane: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Focus an adjacent pane by spatial direction
    SpatialFocus {
        /// Spatial direction to focus
        #[arg(long, value_parser = ["left", "right", "up", "down"])]
        direction: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Resize a split node
    ResizeSplit {
        /// Split node id to resize
        #[arg(long)]
        split_node: String,
        /// Resulting split ratio
        #[arg(long)]
        ratio: f64,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Equalize split ratios in a tab
    EqualizeSplits {
        /// Optional tab id; defaults to the selected tab
        #[arg(long)]
        tab: Option<String>,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Zoom a split pane
    Zoom {
        /// Pane id to zoom
        #[arg(long)]
        pane: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Unzoom a tab
    Unzoom {
        /// Optional pane id whose tab should unzoom
        #[arg(long)]
        pane: Option<String>,
        /// Optional tab id; defaults to the selected tab
        #[arg(long)]
        tab: Option<String>,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Deprecated alias for `terminal send-text --pane`
    #[command(hide = true)]
    SendText {
        /// Pane id to target
        #[arg(long)]
        pane: String,
        /// Text to send
        #[arg(long)]
        text: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
}

#[derive(Subcommand)]
enum ShellTerminalAction {
    /// Send text to terminal content
    SendText {
        /// PaneSlot id to resolve to terminal content
        #[arg(long, conflicts_with = "content")]
        pane: Option<String>,
        /// Terminal content id to target directly
        #[arg(long, conflicts_with = "pane")]
        content: Option<String>,
        /// Text to send
        #[arg(long)]
        text: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
}

#[derive(Subcommand)]
enum ShellAttentionAction {
    /// List panes that currently require attention
    Inbox {
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Override a pane attention state
    Set {
        /// Pane id to target
        #[arg(long)]
        pane: String,
        /// Attention state
        #[arg(long, value_parser = ["idle", "active", "awaiting_user", "notable"])]
        state: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
}

#[derive(Subcommand)]
enum ShellRoutingAction {
    /// Rank candidate panes for intent routing
    Candidates {
        /// Optional preferred pane id
        #[arg(long)]
        pane: Option<String>,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let agent_name = alan_runtime::normalize_agent_name(cli.agent.as_deref()).map(str::to_owned);

    match cli.command {
        Some(Commands::Connection { action }) => match action {
            ConnectionAction::List => {
                cli::connection::run_connection_list().await?;
            }
            ConnectionAction::Show { profile_id } => {
                cli::connection::run_connection_show(&profile_id).await?;
            }
            ConnectionAction::Current { workspace } => {
                cli::connection::run_connection_current(workspace).await?;
            }
            ConnectionAction::Add {
                provider,
                profile,
                label,
                credential,
                settings,
                default,
                activate,
            } => {
                cli::connection::run_connection_add(
                    &provider,
                    profile,
                    label,
                    credential,
                    &settings,
                    default || activate,
                )
                .await?;
            }
            ConnectionAction::Edit {
                profile_id,
                label,
                credential,
                settings,
            } => {
                cli::connection::run_connection_edit(&profile_id, label, credential, &settings)
                    .await?;
            }
            ConnectionAction::SetSecret { profile_id, value } => {
                cli::connection::run_connection_set_secret(&profile_id, value).await?;
            }
            ConnectionAction::Login {
                profile_id,
                mode,
                no_browser,
            } => {
                cli::connection::run_connection_login(
                    &profile_id,
                    matches!(mode, ConnectionLoginMode::Device),
                    !no_browser,
                )
                .await?;
            }
            ConnectionAction::Logout { profile_id } => {
                cli::connection::run_connection_logout(&profile_id).await?;
            }
            ConnectionAction::Default { action } => match action {
                ConnectionDefaultAction::Set {
                    profile_id,
                    workspace,
                } => {
                    cli::connection::run_connection_default_set(&profile_id, workspace).await?;
                }
                ConnectionDefaultAction::Clear { workspace } => {
                    cli::connection::run_connection_default_clear(workspace).await?;
                }
            },
            ConnectionAction::Pin {
                profile_id,
                scope,
                workspace,
            } => {
                cli::connection::run_connection_pin(&profile_id, scope.into(), workspace).await?;
            }
            ConnectionAction::Unpin { scope, workspace } => {
                cli::connection::run_connection_unpin(scope.into(), workspace).await?;
            }
            ConnectionAction::Test { profile_id } => {
                cli::connection::run_connection_test(profile_id).await?;
            }
            ConnectionAction::Remove { profile_id } => {
                cli::connection::run_connection_remove(&profile_id).await?;
            }
            ConnectionAction::Activate { profile_id } | ConnectionAction::Use { profile_id } => {
                cli::connection::run_connection_default_set(&profile_id, None).await?;
            }
            ConnectionAction::Status {
                profile_id,
                workspace,
            } => {
                if let Some(profile_id) = profile_id {
                    cli::connection::run_connection_show(&profile_id).await?;
                } else {
                    cli::connection::run_connection_current(workspace).await?;
                }
            }
        },
        Some(Commands::Daemon { action }) => match action {
            DaemonAction::Start { foreground } => {
                if foreground {
                    // Run in foreground (blocking)
                    init_tracing();
                    let loaded_config = cli::load_agent_config_metadata_with_notice()?;
                    daemon::server::run_server_with_loaded_config(loaded_config).await?;
                } else {
                    // Detach to background
                    cli::daemon::start_daemon_background().await?;
                }
            }
            DaemonAction::Stop => {
                cli::daemon::stop_daemon().await?;
            }
            DaemonAction::Status => {
                cli::daemon::daemon_status().await?;
            }
            DaemonAction::ApiContract => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&daemon::api_contract::endpoint_manifest())?
                );
            }
        },
        Some(Commands::Init { path, name, silent }) => {
            cli::init::run_init(path, name, silent)?;
        }
        Some(Commands::Workspace { action }) => match action {
            WorkspaceAction::List => {
                cli::workspace::list_workspaces()?;
            }
            WorkspaceAction::Add { path, name } => {
                cli::workspace::add_workspace(path, name)?;
            }
            WorkspaceAction::Remove { workspace } => {
                cli::workspace::remove_workspace(&workspace)?;
            }
            WorkspaceAction::Info { workspace } => {
                cli::workspace::workspace_info(&workspace)?;
            }
        },
        Some(Commands::Skills { action }) => match action {
            SkillsAction::List { workspace } => {
                cli::skills::run_list_skills(workspace, agent_name.as_deref())?;
            }
            SkillsAction::Packages { workspace } => {
                cli::skills::run_list_packages(workspace, agent_name.as_deref())?;
            }
            SkillsAction::Init {
                path,
                template,
                name,
                description,
                short_description,
                force,
            } => {
                cli::skills::run_init_skill_package(
                    path,
                    template,
                    name.as_deref(),
                    description.as_deref(),
                    short_description.as_deref(),
                    force,
                )?;
            }
            SkillsAction::Validate { path, json, strict } => {
                let passed = cli::skills::run_validate_skill_package(path, json, strict)?;
                if !passed {
                    std::process::exit(1);
                }
            }
            SkillsAction::Eval {
                path,
                manifest,
                output_dir,
                require_hook,
            } => {
                let passed =
                    cli::skills::run_eval_skill_package(path, manifest, output_dir, require_hook)?;
                if !passed {
                    std::process::exit(1);
                }
            }
            SkillsAction::AggregateBenchmark { run_dir } => {
                let path = cli::skill_authoring::regenerate_skill_eval_benchmark(&run_dir)?;
                println!("{}", path.display());
            }
            SkillsAction::GenerateReview { run_dir } => {
                let path = cli::skill_authoring::regenerate_skill_eval_review_bundle(&run_dir)?;
                println!("{}", path.display());
            }
            SkillsAction::SwebenchLitePrepareWorkspaces(args) => {
                let passed = cli::skills::run_prepare_swebench_lite_workspaces(
                    alan_swebench_tooling::PrepareSwebenchLiteWorkspacesOptions {
                        instance_ids_file: args.instance_ids_file,
                        dataset_files: args.dataset_files,
                        dataset_name: args.dataset_name,
                        split: args.split,
                        workspace_root: args.workspace_root,
                        repo_cache_root: args.repo_cache_root,
                        github_root: args.github_root,
                        workspace_map_output: args.workspace_map_output,
                        skip_mirror_fetch: args.skip_mirror_fetch,
                        reuse_existing_workspaces: args.reuse_existing_workspaces,
                    },
                )?;
                if !passed {
                    std::process::exit(1);
                }
            }
            SkillsAction::SwebenchLiteMaterializeSubset(args) => {
                cli::skills::run_materialize_swebench_lite_subset(
                    alan_swebench_tooling::MaterializeSwebenchLiteSubsetOptions {
                        instance_ids_file: args.instance_ids_file,
                        dataset_files: args.dataset_files,
                        dataset_name: args.dataset_name,
                        split: args.split,
                        workspace_root: args.workspace_root,
                        workspace_map_file: args.workspace_map_file,
                        output_dir: args.output_dir,
                        suite_name: args.suite_name,
                        dataset_label: args.dataset_label,
                        scoring_dataset_name: args.scoring_dataset_name,
                        max_workers: args.max_workers,
                        timeout_secs: args.timeout_secs,
                        allow_missing_workspaces: args.allow_missing_workspaces,
                    },
                )?;
            }
        },
        Some(Commands::Shell { action }) => match action {
            ShellAction::State { target } => {
                cli::shell::run_shell_state(shell_target_options(target))?;
            }
            ShellAction::Space { action } => match action {
                ShellSpaceAction::List { target } => {
                    cli::shell::run_shell_space_list(shell_target_options(target))?;
                }
                ShellSpaceAction::Create { title, cwd, target } => {
                    cli::shell::run_shell_space_create(
                        title.as_deref(),
                        cwd.as_ref().map(|path| path_to_string(path)).as_deref(),
                        shell_target_options(target),
                    )?;
                }
                ShellSpaceAction::OpenAlan { title, cwd, target } => {
                    cli::shell::run_shell_space_open_alan(
                        title.as_deref(),
                        cwd.as_ref().map(|path| path_to_string(path)).as_deref(),
                        shell_target_options(target),
                    )?;
                }
            },
            ShellAction::Tab { action } => match action {
                ShellTabAction::List { space, target } => {
                    cli::shell::run_shell_tab_list(space.as_deref(), shell_target_options(target))?;
                }
                ShellTabAction::Open {
                    space,
                    title,
                    cwd,
                    target,
                } => {
                    cli::shell::run_shell_tab_open(
                        space.as_deref(),
                        title.as_deref(),
                        cwd.as_ref().map(|path| path_to_string(path)).as_deref(),
                        shell_target_options(target),
                    )?;
                }
                ShellTabAction::Close { tab, target } => {
                    cli::shell::run_shell_tab_close(&tab, shell_target_options(target))?;
                }
            },
            ShellAction::Pane { action } => match action {
                ShellPaneAction::List { tab, target } => {
                    cli::shell::run_shell_pane_list(tab.as_deref(), shell_target_options(target))?;
                }
                ShellPaneAction::Snapshot { pane, target } => {
                    cli::shell::run_shell_pane_snapshot(&pane, shell_target_options(target))?;
                }
                ShellPaneAction::Split {
                    pane,
                    direction,
                    target,
                } => {
                    cli::shell::run_shell_pane_split(
                        &pane,
                        &direction,
                        shell_target_options(target),
                    )?;
                }
                ShellPaneAction::Close { pane, target } => {
                    cli::shell::run_shell_pane_close(&pane, shell_target_options(target))?;
                }
                ShellPaneAction::Lift {
                    pane,
                    title,
                    target,
                } => {
                    cli::shell::run_shell_pane_lift(
                        &pane,
                        title.as_deref(),
                        shell_target_options(target),
                    )?;
                }
                ShellPaneAction::Move {
                    pane,
                    tab,
                    direction,
                    target,
                } => {
                    cli::shell::run_shell_pane_move(
                        &pane,
                        &tab,
                        &direction,
                        shell_target_options(target),
                    )?;
                }
                ShellPaneAction::MoveWithinTab {
                    pane,
                    placement,
                    target,
                } => {
                    cli::shell::run_shell_pane_move_within_tab(
                        &pane,
                        &placement,
                        shell_target_options(target),
                    )?;
                }
                ShellPaneAction::Focus { pane, target } => {
                    cli::shell::run_shell_pane_focus(&pane, shell_target_options(target))?;
                }
                ShellPaneAction::SpatialFocus { direction, target } => {
                    cli::shell::run_shell_pane_spatial_focus(
                        &direction,
                        shell_target_options(target),
                    )?;
                }
                ShellPaneAction::ResizeSplit {
                    split_node,
                    ratio,
                    target,
                } => {
                    cli::shell::run_shell_pane_resize_split(
                        &split_node,
                        ratio,
                        shell_target_options(target),
                    )?;
                }
                ShellPaneAction::EqualizeSplits { tab, target } => {
                    cli::shell::run_shell_pane_equalize_splits(
                        tab.as_deref(),
                        shell_target_options(target),
                    )?;
                }
                ShellPaneAction::Zoom { pane, target } => {
                    cli::shell::run_shell_pane_zoom(&pane, shell_target_options(target))?;
                }
                ShellPaneAction::Unzoom { pane, tab, target } => {
                    cli::shell::run_shell_pane_unzoom(
                        pane.as_deref(),
                        tab.as_deref(),
                        shell_target_options(target),
                    )?;
                }
                ShellPaneAction::SendText { pane, text, target } => {
                    cli::shell::run_shell_pane_send_text(
                        &pane,
                        &text,
                        shell_target_options(target),
                    )?;
                }
            },
            ShellAction::Terminal { action } => match action {
                ShellTerminalAction::SendText {
                    pane,
                    content,
                    text,
                    target,
                } => {
                    if pane.is_none() && content.is_none() {
                        anyhow::bail!("terminal send-text requires --pane or --content");
                    }
                    cli::shell::run_shell_terminal_send_text(
                        pane.as_deref(),
                        content.as_deref(),
                        &text,
                        shell_target_options(target),
                    )?;
                }
            },
            ShellAction::Attention { action } => match action {
                ShellAttentionAction::Inbox { target } => {
                    cli::shell::run_shell_attention_inbox(shell_target_options(target))?;
                }
                ShellAttentionAction::Set {
                    pane,
                    state,
                    target,
                } => {
                    cli::shell::run_shell_attention_set(
                        &pane,
                        &state,
                        shell_target_options(target),
                    )?;
                }
            },
            ShellAction::Routing { action } => match action {
                ShellRoutingAction::Candidates { pane, target } => {
                    cli::shell::run_shell_routing_candidates(
                        pane.as_deref(),
                        shell_target_options(target),
                    )?;
                }
            },
            ShellAction::Events {
                after_event_id,
                limit,
                follow,
                target,
            } => {
                cli::shell::run_shell_events(
                    after_event_id.as_deref(),
                    limit,
                    follow,
                    shell_target_options(target),
                )?;
            }
        },
        None => {
            if !alan_tui::terminal::is_interactive_terminal() {
                anyhow::bail!("{}", alan_tui::terminal::terminal_capability_error());
            }
            let mut config = prepare_tui_config(agent_name).await?;
            config.require_interactive_terminal = false;
            alan_tui::run(config).await?;
        }
    }

    Ok(())
}

async fn prepare_tui_config(agent_name: Option<String>) -> Result<alan_tui::RunConfig> {
    let endpoints = Arc::new(AlanEndpointContract);
    let agentd_url_override = host_config::daemon_url_env_override();

    let base_url = if let Some(remote_url) = agentd_url_override {
        let client =
            alan_tui::daemon_client::DaemonClient::new(remote_url.clone(), endpoints.clone());
        client.health().await?;
        remote_url
    } else {
        cli::load_agent_config_with_notice()?;
        cli::daemon::ensure_daemon_running_with_state().await?;
        cli::daemon::daemon_url()
    };

    let mut config = alan_tui::RunConfig::new(base_url, endpoints);
    config.agent_name = agent_name;
    Ok(config)
}

#[derive(Debug)]
struct AlanEndpointContract;

impl alan_tui::daemon_client::EndpointContract for AlanEndpointContract {
    fn health(&self) -> &'static str {
        daemon::api_contract::paths::HEALTH
    }

    fn sessions(&self) -> &'static str {
        daemon::api_contract::paths::sessions()
    }

    fn session_read(&self, session_id: &str) -> String {
        daemon::api_contract::paths::session_read(session_id)
    }

    fn session_reconnect_snapshot(&self, session_id: &str) -> String {
        daemon::api_contract::paths::session_reconnect_snapshot(session_id)
    }

    fn session_history(&self, session_id: &str) -> String {
        daemon::api_contract::paths::session_history(session_id)
    }

    fn session_events_read(&self, session_id: &str) -> String {
        daemon::api_contract::paths::session_events_read(session_id)
    }

    fn session_events(&self, session_id: &str) -> String {
        daemon::api_contract::paths::session_events(session_id)
    }

    fn session_submit(&self, session_id: &str) -> String {
        daemon::api_contract::paths::session_submit(session_id)
    }

    fn session_resume(&self, session_id: &str) -> String {
        daemon::api_contract::paths::session_resume(session_id)
    }

    fn session_rollback(&self, session_id: &str) -> String {
        daemon::api_contract::paths::session_rollback(session_id)
    }

    fn session_compact(&self, session_id: &str) -> String {
        daemon::api_contract::paths::session_compact(session_id)
    }

    fn connections_current(&self) -> &'static str {
        daemon::api_contract::paths::CONNECTIONS_CURRENT
    }

    fn skills_catalog(&self) -> &'static str {
        daemon::api_contract::paths::SKILLS_CATALOG
    }
}

fn shell_target_options(args: ShellTargetArgs) -> cli::shell::ShellTargetOptions {
    cli::shell::ShellTargetOptions {
        socket: args.socket,
        control_dir: args.control_dir,
        window: args.window,
        timeout_ms: args.timeout_ms,
    }
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive(Level::INFO.into())
                .add_directive("alan=debug".parse().unwrap()),
        )
        .init();
}

#[cfg(test)]
mod tests {
    use super::AlanEndpointContract;
    use alan_tui::daemon_client::EndpointContract;

    #[test]
    fn endpoint_contract_uses_daemon_api_contract_paths() {
        let endpoints = AlanEndpointContract;
        assert_eq!(endpoints.health(), "/health");
        assert_eq!(endpoints.sessions(), "/api/v1/sessions");
        assert_eq!(
            endpoints.session_submit("session/id"),
            "/api/v1/sessions/session%2Fid/submit"
        );
    }
}
