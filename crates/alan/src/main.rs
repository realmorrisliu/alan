//! Alan — a programmable personal computing environment.

mod cli;
mod legacy_state;
mod shell_command;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
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
        action: shell_command::ShellAction,
    },
}

#[derive(Subcommand)]
enum HostAction {
    /// Start the matching dedicated Alan OS Host
    Start {
        /// Emit structured JSON
        #[arg(long)]
        json: bool,
    },
    /// Report the matching Alan OS Host lifecycle state
    Status {
        /// Emit structured JSON
        #[arg(long)]
        json: bool,
    },
    /// Stop the matching dedicated Alan OS Host
    Stop {
        /// Emit structured JSON
        #[arg(long)]
        json: bool,
    },
    /// Inspect or answer Host Mount Service requests
    Mount {
        #[command(subcommand)]
        action: HostMountAction,
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
            HostAction::Start { json } => {
                let channel = alan_agent_engine::InstallChannel::detect_current();
                let attachment = cli::host::attach_or_start_host(channel).await?;
                print_host_status(&attachment.status, json)?;
            }
            HostAction::Status { json } => {
                let channel = alan_agent_engine::InstallChannel::detect_current();
                let paths = alan_os_host::HostEndpointPaths::detect(channel.descriptor().id)?;
                let status = paths
                    .read_status()
                    .context("Alan OS Host status is unavailable; run `alan host start`")?;
                print_host_status(&status, json)?;
            }
            HostAction::Stop { json } => {
                let channel = alan_agent_engine::InstallChannel::detect_current();
                let paths = alan_os_host::HostEndpointPaths::detect(channel.descriptor().id)?;
                let status = request_platform_host_stop(channel, &paths).await?;
                wait_for_host_stop(&paths).await?;
                print_host_status(&status, json)?;
            }
            HostAction::Mount { action } => {
                let channel = alan_agent_engine::InstallChannel::detect_current();
                let attached = cli::host::attach_or_start_host(channel).await?;
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
                        let grant =
                            alan_os_host::HostCommandPlane::detect(channel.descriptor().id)?
                                .approve_host_mount(request_id, host_path)
                                .await?;
                        println!("grant_id: {}", grant.id);
                        println!("namespace_path: {}", grant.namespace_path);
                    }
                    HostMountAction::Revoke { grant_id } => {
                        alan_os_host::HostCommandPlane::detect(channel.descriptor().id)?
                            .revoke_host_mount(&grant_id)
                            .await?;
                        println!("Revoked Host Mount grant {grant_id}.");
                    }
                }
            }
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
                        let system =
                            alan_os_host::SystemStorePaths::detect(channel.descriptor().id)?;
                        let host = alan_os_host::HostStorePaths::detect(channel.descriptor().id)?;
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
                        let source = std::path::absolute(&source).with_context(|| {
                            format!(
                                "failed to make import source absolute: {}",
                                source.display()
                            )
                        })?;
                        let system =
                            alan_os_host::SystemStorePaths::detect(channel.descriptor().id)?;
                        let kind = match kind {
                            LegacyImportKind::AgentDefinition => {
                                legacy_state::AuthoredImportKind::AgentDefinition
                            }
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
        Some(Commands::Shell { action }) => shell_command::run(action)?,
        None => {
            let channel = alan_agent_engine::InstallChannel::detect_current();
            let attachment = cli::host::attach_or_start_host(channel).await?;
            let shell = alan_shell::Shell::new(attachment.root);
            alan_shell::StdioDriver::new(shell)
                .run(
                    tokio::io::BufReader::new(tokio::io::stdin()),
                    tokio::io::stdout(),
                )
                .await?;
        }
    }

    Ok(())
}

#[cfg(target_os = "macos")]
async fn request_platform_host_stop(
    channel: alan_agent_engine::InstallChannel,
    paths: &alan_os_host::HostEndpointPaths,
) -> Result<alan_os_host::HostStatus> {
    let mut status = paths.read_status()?;
    let label = cli::host::os_host_launch_label(channel);
    let result = std::process::Command::new("/bin/launchctl")
        .arg("remove")
        .arg(&label)
        .status()
        .with_context(|| format!("request launchd stop for dedicated Host {label}"))?;
    anyhow::ensure!(
        result.success(),
        "launchd failed to remove Host {label}: {result}"
    );
    status.readiness = alan_os_host::HostReadiness::Stopping;
    Ok(status)
}

#[cfg(not(target_os = "macos"))]
async fn request_platform_host_stop(
    _channel: alan_agent_engine::InstallChannel,
    paths: &alan_os_host::HostEndpointPaths,
) -> Result<alan_os_host::HostStatus> {
    alan_os_host::request_host_stop(paths).await
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
        println!("channel: {}", status.channel_id);
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
        if root.kind == legacy_state::AuthoredRootKind::Skills {
            println!(
                "preserved authored Skills: {} (install through `q` in Alan Shell)",
                root.path.display()
            );
        } else {
            println!(
                "preserved authored {:?}: {} (use `alan host legacy-state import` explicitly)",
                root.kind,
                root.path.display()
            );
        }
    }
    if report == &legacy_state::LegacyCleanupReport::default() {
        println!("no recognized legacy state");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Cli;
    #[cfg(target_os = "macos")]
    use super::cli::host::os_host_launch_label;
    use super::cli::host::sibling_executable;
    #[cfg(target_os = "macos")]
    use alan_agent_engine::InstallChannel;
    use clap::Parser;

    #[test]
    fn hidden_tui_backend_flag_is_unavailable() {
        let err = Cli::try_parse_from(["alan", "--tui-backend", "network"])
            .map(|_| ())
            .unwrap_err();
        assert!(err.to_string().contains("--tui-backend"));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn alan_os_host_launch_labels_are_channel_isolated() {
        assert_eq!(
            os_host_launch_label(InstallChannel::Stable),
            "app.alanworks.macos.os-host"
        );
        assert_eq!(
            os_host_launch_label(InstallChannel::Dev),
            "app.alanworks.macos.dev.os-host"
        );
    }

    #[cfg(unix)]
    #[test]
    fn sibling_executable_resolves_the_real_cli_behind_a_symlink() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let bundle_bin = root.path().join("Alan.app/Contents/Resources/bin");
        std::fs::create_dir_all(&bundle_bin).unwrap();
        let cli = bundle_bin.join("alan");
        let host = bundle_bin.join("alan-os-host");
        std::fs::write(&cli, []).unwrap();
        std::fs::write(&host, []).unwrap();
        let link = root.path().join("installed-alan");
        symlink(&cli, &link).unwrap();

        assert_eq!(
            sibling_executable(&link, "alan-os-host").unwrap(),
            host.canonicalize().unwrap()
        );
    }
}
