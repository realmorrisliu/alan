//! Alan — a programmable personal computing environment.

mod cli;
#[allow(dead_code)]
mod host_mounts;
mod legacy_state;
mod system_store;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::Path;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "alan",
    about = "Alan — a programmable personal computing environment",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Host lifecycle, migration, and native integration operations
    Host {
        #[command(subcommand)]
        action: HostAction,
    },
    /// Manage connection profiles and credentials
    Connection {
        #[command(subcommand)]
        action: ConnectionAction,
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
enum HostAction {
    /// Inspect, migrate, or clean state created by retired Host-directory contracts
    LegacyState {
        #[command(subcommand)]
        action: LegacyStateAction,
    },
}

#[derive(Subcommand)]
enum LegacyStateAction {
    /// Report fixed generated, migratable, and possibly authored paths
    Inspect {
        /// Explicit project roots to inspect; no other Host directories are scanned
        #[arg(long = "source-root")]
        source_roots: Vec<PathBuf>,
        /// Emit structured JSON
        #[arg(long)]
        json: bool,
    },
    /// Migrate connections and remove only recognized generated paths
    Cleanup {
        /// Explicit project roots whose fixed legacy paths may be cleaned
        #[arg(long = "source-root")]
        source_roots: Vec<PathBuf>,
        /// Emit structured JSON
        #[arg(long)]
        json: bool,
    },
    /// Explicitly import authored legacy content into an owning System Store
    Import {
        #[arg(value_enum)]
        kind: LegacyImportKind,
        /// Existing Host directory to import
        source: PathBuf,
        /// Installed name in the destination store
        #[arg(long)]
        name: String,
        /// Delete the source only after byte-for-byte tree verification
        #[arg(long)]
        delete_source: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum LegacyImportKind {
    AgentDefinition,
    Skill,
    MemoryStore,
}

#[derive(Subcommand)]
enum ConnectionAction {
    /// List configured connection profiles
    List,
    /// Show one connection profile and credential status
    Show { profile_id: String },
    /// Show the effective connection selection state
    Current,
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
    /// Manage the default profile for future Agent Processes
    Default {
        #[command(subcommand)]
        action: ConnectionDefaultAction,
    },
    /// Validate one connection profile
    Test { profile_id: Option<String> },
    /// Remove a connection profile
    Remove { profile_id: String },
}

#[derive(Subcommand)]
enum ConnectionDefaultAction {
    /// Set the default profile for future Agent Processes
    Set { profile_id: String },
    /// Clear the default profile for future Agent Processes
    Clear,
}

#[derive(clap::ValueEnum, Clone, Copy, Default)]
enum ConnectionLoginMode {
    #[default]
    Browser,
    Device,
}

#[derive(Subcommand)]
enum SkillsAction {
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

fn parse_cli() -> Cli {
    let args = std::env::args_os().collect::<Vec<_>>();
    match Cli::try_parse_from(&args) {
        Ok(cli) => cli,
        Err(error) => {
            if is_retired_workspace_invocation(&args) {
                eprintln!(
                    "Workspace runtime commands were removed. Authorize Host files with an explicit Host Mount, then use Alan Shell operations inside Alan OS."
                );
            }
            error.exit()
        }
    }
}

fn is_retired_workspace_invocation(args: &[std::ffi::OsString]) -> bool {
    let args = args
        .iter()
        .skip(1)
        .filter_map(|argument| argument.to_str())
        .collect::<Vec<_>>();
    matches!(args.first().copied(), Some("init" | "workspace"))
        || matches!(args.as_slice(), ["connection", "pin" | "unpin", ..])
        || args.iter().any(|argument| {
            *argument == "--agent" || argument.starts_with("--agent=") || *argument == "--workspace"
        })
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = parse_cli();

    match cli.command {
        Some(Commands::Host { action }) => match action {
            HostAction::LegacyState { action } => {
                let channel = alan_agent_engine::InstallChannel::detect_current();
                match action {
                    LegacyStateAction::Inspect { source_roots, json } => {
                        let Some(paths) = legacy_state::LegacyStatePaths::detect(channel)? else {
                            anyhow::bail!("cannot determine Host home directory");
                        };
                        let source_roots = canonical_existing_roots(source_roots)?;
                        let report = legacy_state::inspect_legacy_state(&paths, &source_roots)?;
                        print_legacy_inspection(&report, json)?;
                    }
                    LegacyStateAction::Cleanup { source_roots, json } => {
                        let Some(paths) = legacy_state::LegacyStatePaths::detect(channel)? else {
                            anyhow::bail!("cannot determine Host home directory");
                        };
                        let source_roots = canonical_existing_roots(source_roots)?;
                        let system = system_store::SystemStorePaths::detect(channel)?;
                        let host = system_store::HostStorePaths::detect(channel)?;
                        let report = legacy_state::cleanup_legacy_state(
                            &paths,
                            &system,
                            &host,
                            &source_roots,
                        )?;
                        print_legacy_cleanup(&report, json)?;
                    }
                    LegacyStateAction::Import {
                        kind,
                        source,
                        name,
                        delete_source,
                    } => {
                        let source = std::fs::canonicalize(&source).with_context(|| {
                            format!("failed to resolve import source {}", source.display())
                        })?;
                        let system = system_store::SystemStorePaths::detect(channel)?;
                        let kind = match kind {
                            LegacyImportKind::AgentDefinition => {
                                legacy_state::AuthoredImportKind::AgentDefinition
                            }
                            LegacyImportKind::Skill => legacy_state::AuthoredImportKind::Skill,
                            LegacyImportKind::MemoryStore => {
                                legacy_state::AuthoredImportKind::MemoryStore
                            }
                        };
                        let report = legacy_state::import_authored_content(
                            kind,
                            &source,
                            &name,
                            delete_source,
                            &system,
                        )?;
                        println!("imported: {}", report.destination.display());
                        if report.source_deleted {
                            println!(
                                "source deleted after verification: {}",
                                report.source.display()
                            );
                        }
                    }
                }
            }
        },
        Some(Commands::Connection { action }) => match action {
            ConnectionAction::List => {
                cli::connection::run_connection_list().await?;
            }
            ConnectionAction::Show { profile_id } => {
                cli::connection::run_connection_show(&profile_id).await?;
            }
            ConnectionAction::Current => {
                cli::connection::run_connection_current().await?;
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
                ConnectionDefaultAction::Set { profile_id } => {
                    cli::connection::run_connection_default_set(&profile_id).await?;
                }
                ConnectionDefaultAction::Clear => {
                    cli::connection::run_connection_default_clear().await?;
                }
            },
            ConnectionAction::Test { profile_id } => {
                cli::connection::run_connection_test(profile_id).await?;
            }
            ConnectionAction::Remove { profile_id } => {
                cli::connection::run_connection_remove(&profile_id).await?;
            }
        },
        Some(Commands::Skills { action }) => match action {
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
            let (mut config, controller) = prepare_file_backed_tui_config().await?;
            config.require_interactive_terminal = false;
            let run_result = alan_tui::run_file_backed(config).await;
            let shutdown_result = controller.shutdown().await;
            run_result?;
            shutdown_result?;
        }
    }

    Ok(())
}

async fn prepare_file_backed_tui_config() -> Result<(
    alan_tui::FileBackedRunConfig,
    alan_agent_engine::RuntimeController,
)> {
    let channel = alan_agent_engine::InstallChannel::detect_current();
    let system_store = system_store::SystemStorePaths::detect(channel)?;
    let host_store = system_store::HostStorePaths::detect(channel)?;
    if let Some(legacy_paths) = legacy_state::LegacyStatePaths::detect(channel)? {
        legacy_state::cleanup_legacy_state(&legacy_paths, &system_store, &host_store, &[])?;
    }
    let mut namespace = alan_kernel::Namespace::new();
    let store_bindings = system_store.agent_runtime_bindings()?;
    let memory_store_backing = system_store.memory_stores()?.join("default");
    std::fs::create_dir_all(&memory_store_backing)
        .context("failed to prepare Memory Store backing")?;
    let memory_store =
        alan_hostfs::HostDirFs::new(&memory_store_backing, alan_hostfs::HostDirAccess::ReadWrite)
            .context("failed to open Memory Store backing")?;
    namespace.mount(
        "/memory",
        alan_ap::InProcessTransport::new(std::sync::Arc::new(memory_store)),
        alan_kernel::Access::ReadWrite,
    );
    let mut runtime_config = alan_agent_engine::AgentProcessConfig::from(
        alan_agent_engine::Config::load_with_metadata()?,
    );
    runtime_config.launch_context = alan_agent_engine::ProcessLaunchContext::new(
        namespace,
        alan_kernel::Credentials::user("local-user"),
        "/",
    )?
    .with_descriptor(
        alan_agent_engine::MEMORY_STORE_DESCRIPTOR,
        alan_agent_engine::ProcessDescriptor::new("/memory")?,
    );
    runtime_config.store_bindings = Some(store_bindings);
    runtime_config.memory_store_backing = Some(memory_store_backing);
    runtime_config.connection_store = Some(system_store.connection_bindings(&host_store)?);
    runtime_config.chatgpt_auth_storage_path = Some(host_store.managed_auth.clone());
    // Approved request_mount grants must reach the live namespace from bare
    // `alan` too; without the factory
    // the runtime falls back to "live namespace mount applicator unavailable".
    runtime_config.mount_grant_applicator_factory = Some(std::sync::Arc::new(
        crate::host_mounts::LiveNamespaceMountGrantApplicatorFactory,
    ));

    let mut launch = alan_agent_engine::spawn_with_namespace_surface(runtime_config)
        .await
        .context("failed to spawn file-backed runtime surface")?;
    launch
        .controller
        .wait_until_ready()
        .await
        .context("file-backed runtime failed before becoming ready")?;

    let mut config = alan_tui::FileBackedRunConfig::new(
        launch.surface.root_transport(),
        launch.surface.agent_path().to_string(),
    );
    config.history_path = Some(system_store.agent_runtime()?.metadata.join("tui_history"));

    Ok((config, launch.controller))
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

fn canonical_existing_roots(paths: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
    paths
        .into_iter()
        .map(|path| {
            std::fs::canonicalize(&path)
                .with_context(|| format!("failed to resolve source root {}", path.display()))
        })
        .collect()
}

fn print_legacy_inspection(report: &legacy_state::LegacyInspection, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }
    for path in &report.generated_paths {
        println!("generated: {}", path.display());
    }
    for path in &report.migratable_paths {
        println!("migratable: {}", path.display());
    }
    for root in &report.authored_roots {
        println!("authored {:?}: {}", root.kind, root.path.display());
    }
    if report.generated_paths.is_empty()
        && report.migratable_paths.is_empty()
        && report.authored_roots.is_empty()
    {
        println!("no recognized legacy state");
    }
    Ok(())
}

fn print_legacy_cleanup(report: &legacy_state::LegacyCleanupReport, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }
    if report.connection_migration.metadata_migrated {
        println!("migrated and verified: connection metadata");
    }
    if report.connection_migration.credential_file_migrated {
        println!("migrated and verified: Host credential file");
    }
    if report.connection_migration.managed_auth_migrated {
        println!("migrated and verified: managed auth file");
    }
    for path in &report.removed_generated_paths {
        println!("removed generated: {}", path.display());
    }
    for root in &report.authored_roots {
        println!(
            "preserved authored {:?}: {} (use `alan host legacy-state import` explicitly)",
            root.kind,
            root.path.display()
        );
    }
    if report == &legacy_state::LegacyCleanupReport::default() {
        println!("no recognized legacy state");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::Parser;

    #[test]
    fn hidden_tui_backend_flag_is_unavailable() {
        let err = Cli::try_parse_from(["alan", "--tui-backend", "network"])
            .map(|_| ())
            .unwrap_err();
        assert!(err.to_string().contains("--tui-backend"));
    }
}
