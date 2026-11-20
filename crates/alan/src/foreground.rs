use super::*;

mod login;

#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "legacy helper retained for existing foreground tests"
    )
)]
fn interactive_config(
    root: alan_ap::InProcessTransport,
    root_model: Option<String>,
    project_candidate: Option<PathBuf>,
    store: &alan_os_host::SystemStorePaths,
) -> Result<alan_tui::FileBackedRunConfig> {
    interactive_config_with_posture(
        root,
        root_model,
        project_candidate,
        store,
        alan_agent_engine::TrustPosture::Governed,
    )
}

fn interactive_config_with_posture(
    root: alan_ap::InProcessTransport,
    root_model: Option<String>,
    project_candidate: Option<PathBuf>,
    store: &alan_os_host::SystemStorePaths,
    trust_posture: alan_agent_engine::TrustPosture,
) -> Result<alan_tui::FileBackedRunConfig> {
    let mut config = alan_tui::FileBackedRunConfig::new(root, "/agent/root");
    config.connection_configured = Some(root_model.is_some());
    config.effective_model = root_model;
    config.project_candidate = project_candidate;
    config.history_path = Some(store.service("shell-ui")?.join("composer-history"));
    config.sandbox_backend = Some(if trust_posture.is_unsandboxed_host_account() {
        "unsandboxed host-account".to_string()
    } else {
        alan_agent_engine::tools::active_backend_name().to_string()
    });
    config.trust_posture_label = trust_posture.tui_badge_label().map(str::to_string);
    Ok(config)
}

#[cfg(test)]
#[path = "foreground_tests.rs"]
mod tests;

pub(super) fn foreground_runtime_dir() -> Result<(PathBuf, bool)> {
    if let Some(runtime_dir) = std::env::var_os(cli::host::INSTANCE_RUNTIME_DIR_ENV) {
        return Ok((PathBuf::from(runtime_dir), false));
    }

    Ok((
        generated_foreground_runtime_dir(&std::env::temp_dir())?,
        true,
    ))
}

pub(super) fn generated_foreground_runtime_dir(temp_root: &Path) -> Result<PathBuf> {
    let instance_name = format!("alan-{}", uuid::Uuid::new_v4());
    let runtime_dir = temp_root.join(&instance_name);
    if HostEndpointPaths::from_runtime_dir(&runtime_dir).is_ok() {
        return Ok(runtime_dir);
    }

    // macOS's sockaddr path is short; its default TMPDIR can exceed that limit.
    let runtime_dir = Path::new("/tmp").join(instance_name);
    HostEndpointPaths::from_runtime_dir(&runtime_dir)?;
    Ok(runtime_dir)
}

#[allow(
    clippy::too_many_arguments,
    reason = "Foreground CLI entrypoint passes parsed invocation options directly without bundling mutable state"
)]
pub(super) async fn run_bare_in_foreground_instance(
    paths: HostEndpointPaths,
    mode: BareRunMode,
    resume_root: bool,
    resume_rollout_id: Option<String>,
    ephemeral: bool,
    cwd: Option<std::path::PathBuf>,
    no_auto_mount: bool,
    shadow_evaluator: Option<String>,
    trust_options: cli::trust::ResolvedBareTrustOptions,
) -> Result<i32> {
    anyhow::ensure!(
        !(ephemeral && resume_root),
        "`--ephemeral` cannot be combined with `--resume`"
    );
    let trust_posture = trust_options.posture;
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .context("listen for Alan foreground interrupt")?;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .context("listen for Alan instance shutdown")?;
    let shadow_surface = match mode {
        BareRunMode::Interactive { .. } => {
            alan_agent_engine::runtime::EvaluationSurface::Interactive
        }
        BareRunMode::OneShot { .. } => alan_agent_engine::runtime::EvaluationSurface::Redirected,
    };
    let config = cli::trust::apply_boot_trust_options(
        HostBootConfig::product_with_root_resume(resume_root)?,
        trust_options,
        resume_rollout_id,
        ephemeral,
        shadow_evaluator,
        shadow_surface,
    )?;
    let is_json_mode = matches!(&mode, BareRunMode::OneShot { json: true, .. });
    let host = match AlanOsHost::boot(config, paths.clone()).await {
        Ok(host) => host,
        Err(error) => {
            if is_json_mode {
                println!(
                    "{}",
                    serde_json::json!({
                        "type": "error",
                        "message": format!("{error:#}"),
                    })
                );
            }
            return Err(error);
        }
    };
    let project_boot = host.status().boot_id;
    let root_model = host.root_model().map(str::to_string);
    let (shutdown, shutdown_requested) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        host.serve_until(async move {
            let _ = shutdown_requested.await;
        })
        .await
    });

    let run_result: Result<i32> = tokio::select! {
        result = async {
            let attachment = LocalAttachment::new(paths.clone()).connect().await?;
            match mode {
                BareRunMode::Interactive { images } => {
                    let store = alan_os_host::SystemStorePaths::detect()?;
                    let candidate_cwd = cwd.or_else(|| std::env::current_dir().ok());
                    let initial_attachments = cli::attachments::collect_cli_image_attachments(
                        None,
                        candidate_cwd.as_deref(),
                        &images,
                    )?;
                    let initial_project_mount = if !no_auto_mount
                        && let Some(candidate) = candidate_cwd.as_deref()
                        && should_auto_mount_cwd(candidate)
                    {
                        auto_mount_initial_project(
                            &paths,
                            project_boot,
                            candidate,
                            attachment.root.clone(),
                        )
                        .await
                    } else {
                        None
                    };
                    let is_unconfigured = root_model.is_none();
                    let mut config = interactive_config_with_posture(
                        attachment.root,
                        root_model,
                        candidate_cwd,
                        &store,
                        trust_posture,
                    )?;
                    config.initial_attachments = initial_attachments;
                    config.initial_project_mount = initial_project_mount;
                    config.update_notice =
                        cli::update::prepare_passive_tui_update_notice(&store, ephemeral);
                    config.login_control = Some(login::build_login_control_handler(paths.clone()));
                    config.auto_setup_on_unconfigured =
                        is_unconfigured && std::env::var_os("ALAN_NO_SETUP").is_none();
                    let settings_paths = paths.clone();
                    config.settings_control = Some(Arc::new(move |command| {
                        let paths = settings_paths.clone();
                        Box::pin(async move {
                            match command {
                                alan_tui::SettingsControl::Reset { scope } => {
                                    let host_scope = match scope {
                                        alan_tui::SettingsResetScope::Settings => {
                                            alan_os_host::ResetScope::Settings
                                        }
                                        alan_tui::SettingsResetScope::All => {
                                            alan_os_host::ResetScope::All
                                        }
                                    };
                                    let system = alan_os_host::SystemStorePaths::detect()?;
                                    let host = alan_os_host::HostStorePaths::detect()?;
                                    let report = alan_os_host::reset_alan_stores(
                                        &system,
                                        &host,
                                        host_scope,
                                    )?;
                                    alan_os_host::HostCommandPlane::new(paths)
                                        .reload_connections()
                                        .await?;
                                    Ok(alan_tui::SettingsResult::ResetCompleted {
                                        scope,
                                        removed_count: report.removed_paths.len(),
                                    })
                                }
                            }
                        })
                    }));
                    let project_paths = paths.clone();
                    let project_control: alan_tui::ProjectControlHandler = Arc::new(move |command| {
                        let paths = project_paths.clone();
                        Box::pin(async move {
                            let host = alan_os_host::HostCommandPlane::new(paths);
                            match command {
                                alan_tui::ProjectControl::Mount { operation_id, host_path, access } => {
                                    let access = match access {
                                        alan_tui::ProjectAccess::ReadOnly => {
                                            alan_service_manager::HostMountAccess::ReadOnly
                                        }
                                        alan_tui::ProjectAccess::ReadWrite => {
                                            alan_service_manager::HostMountAccess::ReadWrite
                                        }
                                    };
                                    let mounted = match host.mount_project(uuid::Uuid::parse_str(&operation_id)?, project_boot, host_path, access).await {
                                        Ok(mounted) => mounted,
                                        Err(error) => return Ok(if let Some(rejected) = error.downcast_ref::<alan_os_host::ProjectMountRejected>() {
                                            alan_tui::ProjectControlResult::MountRejected { message: rejected.to_string() }
                                        } else {
                                            alan_tui::ProjectControlResult::MountUncertain { message: error.to_string() }
                                        }),
                                    };
                                    let label = mounted
                                        .host_path
                                        .file_name()
                                        .and_then(|name| name.to_str())
                                        .unwrap_or("project")
                                        .to_string();
                                    Ok(alan_tui::ProjectControlResult::Mounted {
                                        receipt: alan_tui::ProjectMountReceipt {
                                            grant_id: mounted.grant.id,
                                            namespace_path: mounted.grant.namespace_path,
                                            label,
                                            access: match access {
                                                alan_service_manager::HostMountAccess::ReadOnly => {
                                                    alan_tui::ProjectAccess::ReadOnly
                                                }
                                                alan_service_manager::HostMountAccess::ReadWrite => {
                                                    alan_tui::ProjectAccess::ReadWrite
                                                }
                                            },
                                        },
                                        completion_root: mounted.host_path,
                                    })
                                }
                                alan_tui::ProjectControl::Revoke { grant_id } => {
                                    host.revoke_host_mount(grant_id).await?;
                                    Ok(alan_tui::ProjectControlResult::Revoked)
                                }
                            }
                        })
                    });
                    config.project_control = Some(project_control);
                    tokio::select! {
                        result = alan_tui::run_file_backed(config) => {
                            result?;
                            Ok(0)
                        }
                        _ = interrupt.recv() => Ok(130),
                    }
                }
                BareRunMode::OneShot { prompt, json, images } => {
                    let input = if let Some(prompt) = prompt {
                        prompt
                    } else {
                        let (input_tx, input_rx) = tokio::sync::oneshot::channel();
                        // A detached OS thread keeps cancelled stdin reads out of Tokio's blocking pool.
                        std::thread::spawn(move || {
                            let input = (|| -> Result<String> {
                                let mut input = Vec::new();
                                std::io::stdin()
                                    .read_to_end(&mut input)
                                    .context("read Agent task from stdin")?;
                                String::from_utf8(input).context("stdin task is not valid UTF-8")
                            })();
                            let _ = input_tx.send(input);
                        });
                        tokio::select! {
                            biased;
                            _ = interrupt.recv() => return Ok(130),
                            input = input_rx => input.context("stdin reader stopped")??,
                        }
                    };
                    let base_dir = cwd.or_else(|| std::env::current_dir().ok());
                    let parts = cli::attachments::collect_cli_image_attachments(
                        Some(&input),
                        base_dir.as_deref(),
                        &images,
                    )?;
                    let exit_code = alan_tui::run_stdio_task_with_options(
                        attachment.root,
                        "/agent/root",
                        &input,
                        alan_tui::StdioTaskOptions { parts, json },
                        async {
                            interrupt.recv().await;
                            Ok::<(), anyhow::Error>(())
                        },
                    )
                    .await?;
                    Ok(exit_code)
                }
            }
        }
        => result,
        _ = terminate.recv() => Ok(143),
    };

    let _ = shutdown.send(());
    let server_result = server
        .await
        .context("Alan OS foreground instance task failed")?;
    let exit_code = run_result?;
    server_result?;
    Ok(exit_code)
}

/// Whether `candidate` is an ordinary project directory eligible for default `ReadWrite`
/// auto-mount (excluding `/`, `$HOME`, and system `/tmp` / `TMPDIR` roots).
pub(super) fn should_auto_mount_cwd(candidate: &Path) -> bool {
    let canonical = std::fs::canonicalize(candidate).unwrap_or_else(|_| candidate.to_path_buf());
    if !canonical.is_dir() {
        return false;
    }
    if canonical == Path::new("/") {
        return false;
    }
    if let Some(home) = dirs::home_dir() {
        let canonical_home = std::fs::canonicalize(&home).unwrap_or(home);
        if canonical == canonical_home {
            return false;
        }
    }
    for blocked_tmp in [PathBuf::from("/tmp"), std::env::temp_dir()] {
        let canonical_tmp = std::fs::canonicalize(&blocked_tmp).unwrap_or(blocked_tmp);
        if canonical == canonical_tmp {
            return false;
        }
    }
    true
}

async fn auto_mount_initial_project(
    paths: &HostEndpointPaths,
    project_boot: uuid::Uuid,
    candidate: &Path,
    root_transport: alan_ap::InProcessTransport,
) -> Option<(alan_tui::ProjectMountReceipt, PathBuf)> {
    let canonical = match std::fs::canonicalize(candidate) {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!(%error, path = %candidate.display(), "failed to canonicalize initial project cwd");
            return None;
        }
    };
    let host = alan_os_host::HostCommandPlane::new(paths.clone());
    let mounted = match host
        .mount_project(
            uuid::Uuid::new_v4(),
            project_boot,
            canonical,
            alan_service_manager::HostMountAccess::ReadWrite,
        )
        .await
    {
        Ok(mounted) => mounted,
        Err(error) => {
            tracing::warn!(%error, "auto-mount of initial cwd skipped");
            return None;
        }
    };
    let label = mounted
        .host_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project")
        .to_string();
    let command = format!(
        "project-cwd-v1 {}",
        serde_json::json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "path": mounted.grant.namespace_path,
        })
    );
    if let Err(error) = alan_shell::Shell::new(root_transport)
        .write("/agent/root/machine/ctl", command.as_bytes())
        .await
    {
        tracing::warn!(%error, "failed to write initial project-cwd-v1 control");
    }
    Some((
        alan_tui::ProjectMountReceipt {
            grant_id: mounted.grant.id,
            namespace_path: mounted.grant.namespace_path,
            label,
            access: alan_tui::ProjectAccess::ReadWrite,
        },
        mounted.host_path,
    ))
}
