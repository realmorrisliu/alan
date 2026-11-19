use super::*;

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
                    let mut config = interactive_config_with_posture(
                        attachment.root,
                        root_model,
                        candidate_cwd,
                        &store,
                        trust_posture,
                    )?;
                    config.initial_attachments = initial_attachments;
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
