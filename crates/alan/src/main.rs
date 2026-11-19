//! Alan — a programmable personal computing environment.

mod cli;
mod reports;
use reports::{
    canonical_existing_roots, print_host_mount_state, print_host_status, print_legacy_cleanup,
    print_legacy_inspection, wait_for_host_stop,
};
mod foreground;
mod legacy_cli;
mod legacy_state;

use alan_os_host::installation::LegacyInstallation;
use alan_os_host::{AlanOsHost, HostBootConfig, HostEndpointPaths, LocalAttachment};
use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::{
    io::{IsTerminal, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Parser)]
#[command(
    name = "alan",
    about = "Alan — a programmable personal computing environment",
    version
)]
struct Cli {
    /// Restore the latest selected Root Agent rollout (or explicit rollout id) for this invocation.
    #[arg(long, value_name = "ROLLOUT_ID", num_args = 0..=1, default_missing_value = "")]
    resume: Option<String>,
    /// Do not read or overwrite the durable root rollout pointer for this invocation.
    #[arg(long)]
    ephemeral: bool,
    /// Initial project directory candidate offered to bare `alan`.
    #[arg(long, value_name = "PATH")]
    cwd: Option<std::path::PathBuf>,
    /// Record typed input advice using this evaluator profile; never auto-run advice.
    #[arg(long, value_parser = clap::builder::NonEmptyStringValueParser::new())]
    shadow_evaluator: Option<String>,
    #[command(flatten)]
    trust: cli::trust::BareTrustArgs,
    /// Submit one non-interactive prompt and print the response to stdout.
    #[arg(short = 'p', long = "print", value_name = "PROMPT")]
    print: Option<String>,
    /// Stream one-shot execution as newline-delimited JSON events (`--print` or piped stdin).
    #[arg(long)]
    json: bool,
    /// Attach one or more local image files (png, jpg/jpeg, gif, webp) to the prompt.
    #[arg(long = "image", value_name = "PATH")]
    images: Vec<PathBuf>,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
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
    Q(cli::q::QArgs),
}

#[derive(Debug, PartialEq, Eq)]
enum BareRunMode {
    Interactive {
        images: Vec<PathBuf>,
    },
    OneShot {
        prompt: Option<String>,
        json: bool,
        images: Vec<PathBuf>,
    },
}

fn bare_run_mode(
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

#[derive(Subcommand)]
enum HostAction {
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
        action: cli::host::HostGrantAction,
    },
    /// Inspect, migrate, or clean state created by retired Host-directory contracts
    LegacyState {
        #[command(subcommand)]
        action: LegacyStateAction,
    },
}

#[derive(Subcommand)]
enum HostMountAction {
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

#[derive(Subcommand)]
enum LegacyStateAction {
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

#[derive(Clone, Copy, ValueEnum)]
enum LegacyImportKind {
    AgentDefinition,
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

fn parse_cli() -> Cli {
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

fn validate_resume_scope(resume: bool, has_subcommand: bool) -> Result<()> {
    anyhow::ensure!(
        !resume || !has_subcommand,
        "`--resume` only applies to bare `alan`"
    );
    Ok(())
}

fn validate_bare_flags(
    resume: bool,
    ephemeral: bool,
    cwd: Option<&std::path::Path>,
    has_subcommand: bool,
) -> Result<Option<std::path::PathBuf>> {
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

#[tokio::main]
async fn main() -> Result<()> {
    alan_tui::terminal::install_panic_hook();
    let cli = parse_cli();
    alan_os_host::installation::validate_current_invocation()?;
    let (resume, resume_rollout_id) =
        cli::trust::parse_resume_arg(cli.resume.as_deref(), cli.command.is_some())?;
    let cwd_override = validate_bare_flags(
        resume,
        cli.ephemeral,
        cli.cwd.as_deref(),
        cli.command.is_some(),
    )?;
    let trust_options = cli::trust::resolve_bare_trust_options(&cli.trust, cli.command.is_some())?;
    anyhow::ensure!(
        cli.shadow_evaluator.is_none() || cli.command.is_none(),
        "`--shadow-evaluator` only applies to bare `alan`"
    );
    cli::attachments::validate_cli_scripting_flags(
        cli.print.as_deref(),
        cli.json,
        &cli.images,
        cli.command.is_some(),
    )?;

    let uses_product_stores = matches!(
        &cli.command,
        None | Some(Commands::Connection { .. })
            | Some(Commands::Q(_))
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
    );
    let _installation_access = uses_product_stores
        .then(|| alan_os_host::installation::InstallationPaths::detect()?.access())
        .transpose()?;

    match cli.command {
        Some(Commands::LegacyState { action }) => legacy_cli::run(action).await?,
        Some(Commands::Host { action }) => match action {
            HostAction::Status { json } => {
                let paths = cli::host::explicit_instance_paths()?;
                let status = paths
                    .read_status()
                    .context("selected Alan instance status is unavailable")?;
                print_host_status(&status, json)?;
            }
            HostAction::Stop { json } => {
                let paths = cli::host::explicit_instance_paths()?;
                let status = alan_os_host::request_host_stop(&paths).await?;
                wait_for_host_stop(&paths).await?;
                print_host_status(&status, json)?;
            }
            HostAction::Mount { action } => {
                let paths = cli::host::explicit_instance_paths()?;
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
                cli::host::run_host_grant_action(action).await?;
            }
            HostAction::LegacyState { action } => legacy_cli::run(action).await?,
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
            SkillsAction::List { json } => {
                cli::skills_list::run_list_skills(json)?;
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
        Some(Commands::About) => {
            cli::about::run()?;
        }
        Some(Commands::Man { topic }) => {
            cli::man::run(topic.as_deref())?;
        }
        Some(Commands::Q(args)) => {
            cli::q::run(args).await?;
        }
        None => {
            let mode = bare_run_mode(
                std::io::stdin().is_terminal(),
                std::io::stdout().is_terminal(),
                cli.print,
                cli.json,
                cli.images,
            )?;

            let (runtime_dir, remove_runtime_dir) = foreground::foreground_runtime_dir()?;
            let paths = HostEndpointPaths::from_runtime_dir(&runtime_dir)?;
            let result = foreground::run_bare_in_foreground_instance(
                paths,
                mode,
                resume,
                resume_rollout_id,
                cli.ephemeral,
                cwd_override,
                cli.shadow_evaluator,
                trust_options,
            )
            .await;
            if remove_runtime_dir
                && let Err(error) = std::fs::remove_dir_all(&runtime_dir)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(%error, path = %runtime_dir.display(), "failed to remove temporary Alan instance runtime directory");
            }
            let exit_code = result?;
            if exit_code != 0 {
                std::process::exit(if (1..=255).contains(&exit_code) {
                    exit_code
                } else {
                    1
                });
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use super::HostEndpointPaths;
    #[cfg(target_os = "macos")]
    use super::foreground::generated_foreground_runtime_dir;
    use super::{BareRunMode, Cli, bare_run_mode, validate_resume_scope};
    use clap::Parser;

    #[test]
    fn hidden_tui_backend_flag_is_unavailable() {
        let err = Cli::try_parse_from(["alan", "--tui-backend", "network"])
            .map(|_| ())
            .unwrap_err();
        assert!(err.to_string().contains("--tui-backend"));
    }

    #[test]
    fn bare_mode_uses_redirected_stdin_as_the_one_shot_boundary() {
        assert_eq!(
            bare_run_mode(true, true, None, false, vec![]).unwrap(),
            BareRunMode::Interactive { images: vec![] }
        );
        assert_eq!(
            bare_run_mode(false, true, None, false, vec![]).unwrap(),
            BareRunMode::OneShot {
                prompt: None,
                json: false,
                images: vec![]
            }
        );
        assert_eq!(
            bare_run_mode(true, true, Some("hi".into()), true, vec![]).unwrap(),
            BareRunMode::OneShot {
                prompt: Some("hi".into()),
                json: true,
                images: vec![]
            }
        );
        let err = bare_run_mode(true, false, None, false, vec![]).unwrap_err();
        assert!(err.to_string().contains("needs terminal stdout"));
        assert!(bare_run_mode(true, true, None, true, vec![]).is_err());
    }

    #[test]
    fn shadow_evaluator_requires_an_explicit_nonempty_profile() {
        assert!(
            Cli::try_parse_from(["alan"])
                .unwrap()
                .shadow_evaluator
                .is_none()
        );
        assert_eq!(
            Cli::try_parse_from(["alan", "--shadow-evaluator", "evaluation"])
                .unwrap()
                .shadow_evaluator
                .as_deref(),
            Some("evaluation")
        );
        assert!(Cli::try_parse_from(["alan", "--shadow-evaluator", ""]).is_err());
    }

    #[test]
    fn bare_resume_is_explicit_and_cannot_be_combined_with_a_subcommand() {
        assert!(Cli::try_parse_from(["alan"]).unwrap().resume.is_none());
        assert_eq!(
            Cli::try_parse_from(["alan", "--resume"])
                .unwrap()
                .resume
                .as_deref(),
            Some("")
        );
        assert_eq!(
            Cli::try_parse_from(["alan", "--resume", "rollout-abc"])
                .unwrap()
                .resume
                .as_deref(),
            Some("rollout-abc")
        );
        assert!(validate_resume_scope(true, true).is_err());
        assert!(validate_resume_scope(true, false).is_ok());
    }

    #[test]
    fn trust_posture_flags_enforce_explicit_invocation_only_hierarchy() {
        let defaults = Cli::try_parse_from(["alan"]).unwrap();
        let resolved_default =
            super::cli::trust::resolve_bare_trust_options(&defaults.trust, false).unwrap();
        assert_eq!(
            resolved_default.posture,
            alan_agent_engine::TrustPosture::Governed
        );

        let confined =
            Cli::try_parse_from(["alan", "--trust", "--guardian-profile", "fast"]).unwrap();
        let resolved_confined =
            super::cli::trust::resolve_bare_trust_options(&confined.trust, false).unwrap();
        assert_eq!(
            resolved_confined.posture,
            alan_agent_engine::TrustPosture::ConfinedTrusted
        );
        assert_eq!(resolved_confined.guardian_profile.as_deref(), Some("fast"));

        let unsafe_only = Cli::try_parse_from(["alan", "--unsafe-host-account"]).unwrap();
        assert!(super::cli::trust::resolve_bare_trust_options(&unsafe_only.trust, false).is_err());

        let unsafe_with_trust =
            Cli::try_parse_from(["alan", "--trust", "--unsafe-host-account"]).unwrap();
        assert_eq!(
            super::cli::trust::resolve_bare_trust_options(&unsafe_with_trust.trust, false)
                .unwrap()
                .posture,
            alan_agent_engine::TrustPosture::HostAccountTrusted
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn foreground_runtime_falls_back_when_tempdir_exceeds_socket_path_limit() {
        let long_tempdir = std::path::Path::new("/").join("x".repeat(100));
        let runtime_dir = generated_foreground_runtime_dir(&long_tempdir).unwrap();

        assert_eq!(runtime_dir.parent(), Some(std::path::Path::new("/tmp")));
        assert!(HostEndpointPaths::from_runtime_dir(&runtime_dir).is_ok());
    }

    #[test]
    fn host_start_is_rejected_as_a_retired_background_command() {
        let error = Cli::try_parse_from(["alan", "host", "start"])
            .map(|_| ())
            .unwrap_err();
        assert!(error.to_string().contains("unrecognized subcommand"));
    }
}
