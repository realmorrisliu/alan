//! Top-level CLI argument definitions, validation, and subcommand dispatch.

use alan_os_host::installation::LegacyInstallation;
use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

use super::{
    about, connection, host, login, man, memory, q, skill_authoring, skills, skills_list, trust,
    update, workflow,
};

/// Top-level `alan` command-line interface.
#[derive(Parser)]
#[command(
    name = "alan",
    about = "Alan — a programmable personal computing environment",
    version
)]
pub struct Cli {
    /// Restore the latest selected Root Agent rollout (or explicit rollout id) for this invocation.
    #[arg(long, value_name = "ROLLOUT_ID", num_args = 0..=1, default_missing_value = "")]
    pub resume: Option<String>,
    /// Do not read or overwrite the durable root rollout pointer for this invocation.
    #[arg(long)]
    pub ephemeral: bool,
    /// Initial project directory candidate offered to bare `alan`.
    #[arg(long, value_name = "PATH")]
    pub cwd: Option<PathBuf>,
    /// Do not automatically mount the launch project directory into `/mnt`.
    #[arg(long)]
    pub no_auto_mount: bool,
    /// Record typed input advice using this evaluator profile; never auto-run advice.
    #[arg(long, value_parser = clap::builder::NonEmptyStringValueParser::new())]
    pub shadow_evaluator: Option<String>,
    #[command(flatten)]
    pub trust: trust::BareTrustArgs,
    /// Submit one non-interactive prompt and print the response to stdout.
    #[arg(short = 'p', long = "print", value_name = "PROMPT")]
    pub print: Option<String>,
    /// Stream one-shot execution as newline-delimited JSON events (`--print` or piped stdin).
    #[arg(long)]
    pub json: bool,
    /// Attach one or more local image files (png, jpg/jpeg, gif, webp) to the prompt.
    #[arg(long = "image", value_name = "PATH")]
    pub images: Vec<PathBuf>,
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Supported top-level subcommands.
#[derive(Subcommand)]
pub enum Commands {
    /// Inspect or explicitly adopt historical installation data
    LegacyState {
        #[command(subcommand)]
        action: LegacyStateAction,
    },
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
    /// Authenticate and configure a first-party LLM provider profile
    Login {
        /// Provider id: chatgpt, openrouter, vertex, magpie, or openai
        provider: Option<String>,
        /// Print authorization URL instead of launching a web browser
        #[arg(long = "no-browser")]
        no_browser: bool,
        /// Use device-code flow instead of browser callback (ChatGPT)
        #[arg(long)]
        device: bool,
        /// Explicit connection profile id to create or update
        #[arg(long)]
        profile: Option<String>,
    },
    /// Inspect resolved skills, packages, and exposure state
    Skills {
        #[command(subcommand)]
        action: SkillsAction,
    },
    /// Display the Alan card and Standard Namespace overview
    About,
    /// Read alan9 UNIX manual pages (/man)
    Man {
        /// Optional manual topic or section/name (e.g. `alan9`, `1/q`)
        topic: Option<String>,
    },
    /// Quartermaster (`q`) package hub: install, list, upgrade, uninstall, sync
    Q(q::QArgs),
    /// Inspect, search, edit, and revert domain-scoped Memory Stores
    Memory(memory::MemoryArgs),
    /// Manage and run scheduled personal workflows
    Workflow(workflow::WorkflowArgs),
    /// Check for or install standalone `alan` CLI updates
    Update(update::UpdateArgs),
}

/// Execution mode selected for bare `alan` (without subcommands).
#[derive(Debug, PartialEq, Eq)]
pub enum BareRunMode {
    Interactive {
        images: Vec<PathBuf>,
    },
    OneShot {
        prompt: Option<String>,
        json: bool,
        images: Vec<PathBuf>,
    },
}

/// Host lifecycle subcommands (`alan host ...`).
#[derive(Subcommand)]
pub enum HostAction {
    /// Report the instance selected by ALAN_INSTANCE_RUNTIME_DIR
    Status {
        /// Emit structured JSON
        #[arg(long)]
        json: bool,
    },
    /// Stop the instance selected by ALAN_INSTANCE_RUNTIME_DIR
    Stop {
        /// Emit structured JSON
        #[arg(long)]
        json: bool,
    },
    /// Inspect or answer mount requests in the explicitly selected instance
    Mount {
        #[command(subcommand)]
        action: HostMountAction,
    },
    /// Save, list, or revoke standing project delegation grants in System Store
    Grant {
        #[command(subcommand)]
        action: host::HostGrantAction,
    },
    /// Inspect, migrate, or clean state created by retired Host-directory contracts
    LegacyState {
        #[command(subcommand)]
        action: LegacyStateAction,
    },
}

/// Host Mount inspection and approval subcommands (`alan host mount ...`).
#[derive(Subcommand)]
pub enum HostMountAction {
    /// List logical requests and Alan OS-visible grants
    List,
    /// Approve a pending request with a native Host directory
    Approve {
        request_id: String,
        host_path: PathBuf,
    },
    /// Revoke an active grant
    Revoke { grant_id: String },
}

/// Historical installation inspection and adoption subcommands.
#[derive(Subcommand)]
pub enum LegacyStateAction {
    /// Adopt one historical store pair; stop every Alan invocation first. Sources are retained.
    MigrateInstallation {
        #[arg(long, value_name = "stable|dev")]
        from: LegacyInstallation,
        /// Validate offline without creating stores or a migration journal
        #[arg(long, conflicts_with = "rollback")]
        dry_run: bool,
        /// Undo this adoption only while canonical data is unchanged
        #[arg(long)]
        rollback: bool,
        #[arg(long)]
        json: bool,
    },
    /// Report fixed generated, migratable, and possibly authored paths
    Inspect {
        /// Select old Host-directory data; omit to inspect installation pairs only
        #[arg(long, value_name = "stable|dev")]
        from: Option<LegacyInstallation>,
        /// Explicit project roots to inspect; no other Host directories are scanned
        #[arg(long = "source-root")]
        source_roots: Vec<PathBuf>,
        /// Emit structured JSON
        #[arg(long)]
        json: bool,
    },
    /// Migrate connections and remove only recognized generated paths
    Cleanup {
        /// Historical Host-directory source to consume
        #[arg(long, value_name = "stable|dev")]
        from: LegacyInstallation,
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

/// Supported legacy authored content import kinds.
#[derive(Clone, Copy, ValueEnum)]
pub enum LegacyImportKind {
    AgentDefinition,
    MemoryStore,
}

/// Connection profile management subcommands (`alan connection ...`).
#[derive(Subcommand)]
pub enum ConnectionAction {
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

/// Default connection profile subcommands.
#[derive(Subcommand)]
pub enum ConnectionDefaultAction {
    /// Set the default profile for future Agent Processes
    Set { profile_id: String },
    /// Clear the default profile for future Agent Processes
    Clear,
}

/// Login flow mode for managed OAuth connection profiles.
#[derive(clap::ValueEnum, Clone, Copy, Default)]
pub enum ConnectionLoginMode {
    #[default]
    Browser,
    Device,
}

/// Skill inspection and authoring subcommands (`alan skills ...`).
#[derive(Subcommand)]
pub enum SkillsAction {
    /// List preinstalled and installed skill packages
    List {
        /// Emit structured JSON instead of a table
        #[arg(long)]
        json: bool,
    },
    /// Scaffold a new skill package from a first-party template
    Init {
        /// Directory to create for the new skill package
        path: PathBuf,
        /// Template shape to generate
        #[arg(long, value_enum, default_value_t = skill_authoring::SkillTemplateKind::Inline)]
        template: skill_authoring::SkillTemplateKind,
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

/// Hidden arguments for `alan skills swebench-lite-prepare-workspaces`.
#[derive(Args)]
pub struct SwebenchLitePrepareWorkspacesArgs {
    #[arg(long = "instance-ids-file")]
    pub instance_ids_file: PathBuf,
    #[arg(long = "dataset-file")]
    pub dataset_files: Vec<PathBuf>,
    #[arg(long = "dataset-name")]
    pub dataset_name: Option<String>,
    #[arg(long, default_value = "test")]
    pub split: String,
    #[arg(long = "workspace-root")]
    pub workspace_root: PathBuf,
    #[arg(long = "repo-cache-root")]
    pub repo_cache_root: Option<PathBuf>,
    #[arg(long = "github-root", default_value = "https://github.com")]
    pub github_root: String,
    #[arg(long = "workspace-map-output")]
    pub workspace_map_output: Option<PathBuf>,
    #[arg(long = "skip-mirror-fetch", default_value_t = false)]
    pub skip_mirror_fetch: bool,
    #[arg(long = "reuse-existing-workspaces", default_value_t = false)]
    pub reuse_existing_workspaces: bool,
}

/// Hidden arguments for `alan skills swebench-lite-materialize-subset`.
#[derive(Args)]
pub struct SwebenchLiteMaterializeSubsetArgs {
    #[arg(long = "instance-ids-file")]
    pub instance_ids_file: PathBuf,
    #[arg(long = "dataset-file")]
    pub dataset_files: Vec<PathBuf>,
    #[arg(long = "dataset-name")]
    pub dataset_name: Option<String>,
    #[arg(long, default_value = "test")]
    pub split: String,
    #[arg(long = "workspace-root")]
    pub workspace_root: Option<PathBuf>,
    #[arg(long = "workspace-map-file")]
    pub workspace_map_file: Option<PathBuf>,
    #[arg(long = "output-dir")]
    pub output_dir: PathBuf,
    #[arg(long = "suite-name", default_value = "swebench_lite_pilot_v1")]
    pub suite_name: String,
    #[arg(long = "dataset-label", default_value = "SWE-bench Lite")]
    pub dataset_label: String,
    #[arg(
        long = "scoring-dataset-name",
        default_value = "princeton-nlp/SWE-bench_Lite"
    )]
    pub scoring_dataset_name: String,
    #[arg(long = "max-workers", default_value_t = 4)]
    pub max_workers: usize,
    #[arg(long = "timeout-secs", default_value_t = 1800)]
    pub timeout_secs: u64,
    #[arg(long = "allow-missing-workspaces", default_value_t = false)]
    pub allow_missing_workspaces: bool,
}

/// Parse CLI args while emitting clear guidance for retired background/workspace invocations.
pub fn parse_cli() -> Cli {
    let args = std::env::args_os().collect::<Vec<_>>();
    match Cli::try_parse_from(&args) {
        Ok(cli) => cli,
        Err(error) => {
            if is_retired_host_start_invocation(&args) {
                eprintln!(
                    "`alan host start` was removed; run bare `alan` to start a foreground instance."
                );
            } else if is_retired_workspace_invocation(&args) {
                eprintln!(
                    "Workspace runtime commands were removed. Authorize Host files with an explicit Host Mount, then use Alan Shell operations inside Alan OS."
                );
            }
            error.exit()
        }
    }
}

/// Determine bare `alan` execution mode from stdio TTY checks and scripting flags.
pub fn bare_run_mode(
    stdin_is_terminal: bool,
    stdout_is_terminal: bool,
    print: Option<String>,
    json: bool,
    images: Vec<PathBuf>,
) -> Result<BareRunMode> {
    if print.is_some() || !stdin_is_terminal {
        return Ok(BareRunMode::OneShot {
            prompt: print,
            json,
            images,
        });
    }
    if json {
        anyhow::bail!("`--json` requires `--print <PROMPT>` or piped stdin input");
    }
    match (stdin_is_terminal, stdout_is_terminal) {
        (true, true) => Ok(BareRunMode::Interactive { images }),
        (true, false) => anyhow::bail!(
            "bare `alan` needs terminal stdout for interactive input; pipe a task on stdin for one-shot mode"
        ),
        (false, _) => unreachable!("handled above"),
    }
}

/// Reject `--resume` when combined with a subcommand.
pub fn validate_resume_scope(resume: bool, has_subcommand: bool) -> Result<()> {
    anyhow::ensure!(
        !resume || !has_subcommand,
        "`--resume` only applies to bare `alan`"
    );
    Ok(())
}

/// Validate bare invocation flags (`--resume`, `--ephemeral`, `--cwd`).
pub fn validate_bare_flags(
    resume: bool,
    ephemeral: bool,
    cwd: Option<&std::path::Path>,
    has_subcommand: bool,
) -> Result<Option<PathBuf>> {
    validate_resume_scope(resume, has_subcommand)?;
    anyhow::ensure!(
        !ephemeral || !has_subcommand,
        "`--ephemeral` only applies to bare `alan`"
    );
    anyhow::ensure!(
        !(ephemeral && resume),
        "`--ephemeral` cannot be combined with `--resume`"
    );
    anyhow::ensure!(
        cwd.is_none() || !has_subcommand,
        "`--cwd` only applies to bare `alan`"
    );
    cwd.map(|raw| {
        let meta = std::fs::metadata(raw)
            .with_context(|| format!("inspect --cwd path `{}`", raw.display()))?;
        anyhow::ensure!(
            meta.is_dir(),
            "`--cwd` path is not a directory: `{}`",
            raw.display()
        );
        std::fs::canonicalize(raw)
            .with_context(|| format!("canonicalize --cwd path `{}`", raw.display()))
    })
    .transpose()
}

/// Whether the parsed command accesses product stores and should retain `InstallationAccess`.
pub fn uses_product_stores(command: Option<&Commands>) -> bool {
    matches!(
        command,
        None | Some(Commands::Connection { .. })
            | Some(Commands::Login { .. })
            | Some(Commands::Q(_))
            | Some(Commands::Memory(_))
            | Some(Commands::Workflow(_))
            | Some(Commands::LegacyState {
                action: LegacyStateAction::Cleanup { .. } | LegacyStateAction::Import { .. }
            })
            | Some(Commands::Host {
                action: HostAction::Grant { .. }
                    | HostAction::LegacyState {
                        action: LegacyStateAction::Cleanup { .. }
                            | LegacyStateAction::Import { .. },
                    },
            })
    )
}

/// Dispatch a parsed subcommand, returning `Some(LegacyStateAction)` when the caller
/// must invoke the binary-only legacy migration handler.
pub async fn dispatch_subcommand(command: Commands) -> Result<Option<LegacyStateAction>> {
    match command {
        Commands::LegacyState { action } => return Ok(Some(action)),
        Commands::Host { action } => {
            if let Some(legacy) = dispatch_host(action).await? {
                return Ok(Some(legacy));
            }
        }
        Commands::Connection { action } => dispatch_connection(action).await?,
        Commands::Login {
            provider,
            no_browser,
            device,
            profile,
        } => login::run_login(provider, no_browser, device, profile).await?,
        Commands::Skills { action } => dispatch_skills(action)?,
        Commands::About => about::run()?,
        Commands::Man { topic } => man::run(topic.as_deref())?,
        Commands::Q(args) => q::run(args).await?,
        Commands::Memory(args) => memory::run(args).await?,
        Commands::Workflow(args) => workflow::run(args).await?,
        Commands::Update(args) => update::run(args).await?,
    }
    Ok(None)
}

async fn dispatch_host(action: HostAction) -> Result<Option<LegacyStateAction>> {
    match action {
        HostAction::Status { json } => {
            let paths = host::explicit_instance_paths()?;
            let status = paths
                .read_status()
                .context("selected Alan instance status is unavailable")?;
            print_host_status(&status, json)?;
        }
        HostAction::Stop { json } => {
            let paths = host::explicit_instance_paths()?;
            let status = alan_os_host::request_host_stop(&paths).await?;
            wait_for_host_stop(&paths).await?;
            print_host_status(&status, json)?;
        }
        HostAction::Mount { action } => {
            let paths = host::explicit_instance_paths()?;
            let attached = alan_os_host::LocalAttachment::new(paths.clone())
                .connect()
                .await?;
            match action {
                HostMountAction::List => {
                    let shell = alan_shell::Shell::new(attached.root);
                    print_host_mount_state(&shell).await?;
                }
                HostMountAction::Approve {
                    request_id,
                    host_path,
                } => {
                    let host_path = host_path.canonicalize().with_context(|| {
                        format!("resolve Host directory {}", host_path.display())
                    })?;
                    anyhow::ensure!(host_path.is_dir(), "Host Mount path is not a directory");
                    let grant = alan_os_host::HostCommandPlane::new(paths)
                        .approve_host_mount(request_id, host_path)
                        .await?;
                    println!("grant_id: {}", grant.id);
                    println!("namespace_path: {}", grant.namespace_path);
                }
                HostMountAction::Revoke { grant_id } => {
                    alan_os_host::HostCommandPlane::new(paths)
                        .revoke_host_mount(&grant_id)
                        .await?;
                    println!("Revoked Host Mount grant {grant_id}.");
                }
            }
        }
        HostAction::Grant { action } => {
            host::run_host_grant_action(action).await?;
        }
        HostAction::LegacyState { action } => return Ok(Some(action)),
    }
    Ok(None)
}

async fn dispatch_connection(action: ConnectionAction) -> Result<()> {
    match action {
        ConnectionAction::List => connection::run_connection_list().await?,
        ConnectionAction::Show { profile_id } => {
            connection::run_connection_show(&profile_id).await?
        }
        ConnectionAction::Current => connection::run_connection_current().await?,
        ConnectionAction::Add {
            provider,
            profile,
            label,
            credential,
            settings,
            default,
            activate,
        } => {
            connection::run_connection_add(
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
            connection::run_connection_edit(&profile_id, label, credential, &settings).await?;
        }
        ConnectionAction::SetSecret { profile_id, value } => {
            connection::run_connection_set_secret(&profile_id, value).await?;
        }
        ConnectionAction::Login {
            profile_id,
            mode,
            no_browser,
        } => {
            connection::run_connection_login(
                &profile_id,
                matches!(mode, ConnectionLoginMode::Device),
                !no_browser,
            )
            .await?;
        }
        ConnectionAction::Logout { profile_id } => {
            connection::run_connection_logout(&profile_id).await?;
        }
        ConnectionAction::Default { action } => match action {
            ConnectionDefaultAction::Set { profile_id } => {
                connection::run_connection_default_set(&profile_id).await?;
            }
            ConnectionDefaultAction::Clear => {
                connection::run_connection_default_clear().await?;
            }
        },
        ConnectionAction::Test { profile_id } => {
            connection::run_connection_test(profile_id).await?;
        }
        ConnectionAction::Remove { profile_id } => {
            connection::run_connection_remove(&profile_id).await?;
        }
    }
    Ok(())
}

fn dispatch_skills(action: SkillsAction) -> Result<()> {
    match action {
        SkillsAction::List { json } => skills_list::run_list_skills(json)?,
        SkillsAction::Init {
            path,
            template,
            name,
            description,
            short_description,
            force,
        } => {
            skills::run_init_skill_package(
                path,
                template,
                name.as_deref(),
                description.as_deref(),
                short_description.as_deref(),
                force,
            )?;
        }
        SkillsAction::Validate { path, json, strict } => {
            let passed = skills::run_validate_skill_package(path, json, strict)?;
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
            let passed = skills::run_eval_skill_package(path, manifest, output_dir, require_hook)?;
            if !passed {
                std::process::exit(1);
            }
        }
        SkillsAction::AggregateBenchmark { run_dir } => {
            let path = skill_authoring::regenerate_skill_eval_benchmark(&run_dir)?;
            println!("{}", path.display());
        }
        SkillsAction::GenerateReview { run_dir } => {
            let path = skill_authoring::regenerate_skill_eval_review_bundle(&run_dir)?;
            println!("{}", path.display());
        }
        SkillsAction::SwebenchLitePrepareWorkspaces(args) => {
            let passed = skills::run_prepare_swebench_lite_workspaces(
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
            skills::run_materialize_swebench_lite_subset(
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
    }
    Ok(())
}

async fn wait_for_host_stop(paths: &alan_os_host::HostEndpointPaths) -> Result<()> {
    // Runtime shutdown can use ten seconds for graceful drain plus five seconds to abort.
    for _ in 0..400 {
        if !paths.status.exists() && !paths.socket.exists() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    anyhow::bail!("Alan OS Host did not stop within twenty seconds")
}

fn print_host_status(status: &alan_os_host::HostStatus, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(status)?);
    } else {
        println!("state: {:?}", status.readiness);
        println!("boot: {}", status.boot_id);
        println!("host pid: {}", status.pid);
        println!("attachment: {}", status.socket.display());
    }
    Ok(())
}

async fn print_host_mount_state(shell: &alan_shell::Shell) -> Result<()> {
    let request_ids = shell
        .ls("/mnt/host-mount/requests")
        .await?
        .into_iter()
        .filter(|entry| !matches!(entry.as_str(), "clone" | "events"))
        .collect::<Vec<_>>();
    println!("requests:");
    if request_ids.is_empty() {
        println!("  (none)");
    }
    for request_id in request_ids {
        let base = format!("/mnt/host-mount/requests/{request_id}");
        let status = String::from_utf8(shell.cat(&format!("{base}/status")).await?)?;
        let request = String::from_utf8(shell.cat(&format!("{base}/request")).await?)?;
        println!("  {request_id} [{}] {request}", status.trim());
    }

    let grant_ids = shell.ls("/mnt/host-mount/grants").await?;
    println!("grants:");
    if grant_ids.is_empty() {
        println!("  (none)");
    }
    for grant_id in grant_ids {
        let record = String::from_utf8(
            shell
                .cat(&format!("/mnt/host-mount/grants/{grant_id}/record"))
                .await?,
        )?;
        println!("  {grant_id} {record}");
    }
    Ok(())
}

fn is_retired_host_start_invocation(args: &[std::ffi::OsString]) -> bool {
    let args = args
        .iter()
        .skip(1)
        .filter_map(|argument| argument.to_str())
        .collect::<Vec<_>>();
    matches!(args.as_slice(), ["host", "start", ..])
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
