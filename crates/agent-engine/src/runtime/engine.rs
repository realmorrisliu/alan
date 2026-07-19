//! Agent Runtime - Core execution engine.

use super::controller::{AgentMachineDurabilityState, RuntimeController, RuntimeStartupMetadata};
use super::launch_config::AgentProcessConfig;
use super::transition::{
    DeferredRuntimeActionExit, TransitionCompletion, accepts_inband_submissions,
    advance_accepted_submission, run_deferred_runtime_action_with_cancel,
};
use super::turn_driver::{
    NAMESPACE_PENDING_RESPONSE_POLL_INTERVAL, TurnInputBroker, is_turn_inband_submission,
    namespace_pending_resume_submission,
};
use super::{NamespaceRuntimeEnvironment, RuntimeLoopState};
use crate::agent_machine::AgentMachine;
use alan_agent_protocol::{InputMode, Submission};
use anyhow::Result;
use std::collections::VecDeque;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

/// Queues for managing submissions.
///
/// There are two submission queues in the agent runtime:
/// Requeue leftover inband submissions from turn state and broker to the outer queue.
async fn requeue_leftover_inband_submissions(
    broker: &TurnInputBroker,
    machine: &mut AgentMachine,
    queued_submissions: &mut VecDeque<QueuedRuntimeItem>,
) -> usize {
    let broker_drained = broker.drain().await;
    let turn_drained = machine.drain_buffered_inband_submissions();
    let count = broker_drained.len() + turn_drained.len();
    for submission in turn_drained {
        push_submission_ahead_of_deferred(queued_submissions, submission);
    }
    for submission in broker_drained {
        push_submission_ahead_of_deferred(queued_submissions, submission);
    }
    count
}

/// 1. The `outer_queue` - cross-turn queue for submissions that are not in the active turn.
/// 2. The `active_turn_broker` - channel for in-turn submissions during active turn execution.
enum QueuedRuntimeItem {
    Submission(Submission),
    Deferred(crate::agent_machine::DeferredRuntimeAction),
}

fn push_submission_ahead_of_deferred(
    outer_queue: &mut VecDeque<QueuedRuntimeItem>,
    submission: Submission,
) {
    let insertion_index = outer_queue
        .iter()
        .position(|item| matches!(item, QueuedRuntimeItem::Deferred(_)))
        .unwrap_or(outer_queue.len());
    outer_queue.insert(insertion_index, QueuedRuntimeItem::Submission(submission));
}

fn should_requeue_deferred_action(
    requeue_requested: bool,
    exit: DeferredRuntimeActionExit,
) -> bool {
    requeue_requested && matches!(exit, DeferredRuntimeActionExit::Cancelled)
}

async fn read_pending_namespace_resume_submission(
    state: &RuntimeLoopState,
) -> Option<Result<Submission>> {
    if !state.machine.has_pending_interaction() {
        return None;
    }

    match namespace_pending_resume_submission(state).await {
        Ok(Some(submission)) => Some(Ok(submission)),
        Ok(None) => None,
        Err(err) => Some(Err(err)),
    }
}

async fn read_pending_namespace_control_submission(
    namespace: &super::NamespaceRuntimeEnvironment,
) -> Option<Result<Submission>> {
    match namespace.read_next_machine_control_submission().await {
        Ok(Some(submission)) => Some(Ok(submission)),
        Ok(None) => None,
        Err(err) => Some(Err(err)),
    }
}

#[derive(Default)]
struct RuntimeSubmissionQueues {
    /// Cross-turn queue for submissions.
    outer_queue: VecDeque<QueuedRuntimeItem>,
    /// The broker that queues in-turn submissions.
    active_turn_broker: TurnInputBroker,
}

impl RuntimeSubmissionQueues {
    fn pop_outer(&mut self) -> Option<QueuedRuntimeItem> {
        self.outer_queue.pop_front()
    }

    fn pop_outer_deferred(&mut self) -> Option<QueuedRuntimeItem> {
        let deferred_index = self
            .outer_queue
            .iter()
            .position(|item| matches!(item, QueuedRuntimeItem::Deferred(_)))?;
        self.outer_queue.remove(deferred_index)
    }

    fn push_outer_submission(&mut self, submission: Submission) {
        push_submission_ahead_of_deferred(&mut self.outer_queue, submission);
    }

    fn push_outer_deferred(&mut self, action: crate::agent_machine::DeferredRuntimeAction) {
        self.outer_queue
            .push_back(QueuedRuntimeItem::Deferred(action));
    }

    async fn requeue_active_turn_leftovers(&mut self, machine: &mut AgentMachine) -> usize {
        requeue_leftover_inband_submissions(
            &self.active_turn_broker,
            machine,
            &mut self.outer_queue,
        )
        .await
    }
}

struct AgentMachineStartupOutcome {
    machine: AgentMachine,
    metadata: RuntimeStartupMetadata,
}

fn best_effort_durability_warning(err: &anyhow::Error) -> String {
    format!("AgentMachine is running without persistent recorder; using in-memory mode: {err}")
}

#[cfg(test)]
pub(crate) fn runtime_host_capabilities(
    _config: &AgentProcessConfig,
    tools: &crate::tools::ToolRegistry,
) -> crate::skills::SkillHostCapabilities {
    runtime_host_capabilities_for_tools(tools.list_tools().into_iter().map(str::to_string))
}

pub(crate) fn runtime_host_capabilities_for_tools(
    tools: impl IntoIterator<Item = String>,
) -> crate::skills::SkillHostCapabilities {
    let path_dirs = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    crate::skills::build_skill_host_capabilities_with_path_dirs(tools, path_dirs, true)
}

#[cfg(test)]
fn runtime_host_capabilities_with_path_dirs<I, P>(
    _config: &AgentProcessConfig,
    tools: &crate::tools::ToolRegistry,
    path_dirs: I,
) -> crate::skills::SkillHostCapabilities
where
    I: IntoIterator<Item = P>,
    P: AsRef<std::path::Path>,
{
    crate::skills::build_skill_host_capabilities_with_path_dirs(
        tools.list_tools().into_iter().map(str::to_string),
        path_dirs,
        true,
    )
}

async fn create_persistent_machine(
    process_path: &str,
    model: &str,
    rollouts_dir: Option<&std::path::PathBuf>,
    rollout_cwd: Option<&std::path::Path>,
    reasoning_effort: Option<alan_agent_protocol::ReasoningEffort>,
) -> anyhow::Result<AgentMachine> {
    AgentMachine::new_with_recorder_options(
        process_path,
        model,
        rollouts_dir.map(|dir| dir.as_path()),
        rollout_cwd,
        reasoning_effort,
    )
    .await
}

struct AgentMachineLaunchContext<'a> {
    process_path: &'a str,
    agent_path: &'a str,
    model: &'a str,
}

async fn initialize_agent_machine(
    launch: AgentMachineLaunchContext<'_>,
    recovery_rollout_path: Option<&std::path::PathBuf>,
    rollouts_dir: Option<&std::path::PathBuf>,
    durability_required: bool,
    rollout_cwd: Option<&std::path::Path>,
    request_controls: crate::ResolvedRequestControls,
) -> anyhow::Result<AgentMachineStartupOutcome> {
    let mut warnings = Vec::new();
    let reasoning_effort = request_controls.reasoning_effort();

    let machine = if let Some(path) = recovery_rollout_path {
        let load_result = AgentMachine::load_from_rollout_with_recorder_cwd(
            path,
            launch.process_path,
            launch.model,
            rollouts_dir.map(|dir| dir.as_path()),
            rollout_cwd,
            reasoning_effort,
        )
        .await;

        match load_result {
            Ok(machine) => machine,
            Err(err) => {
                if durability_required {
                    return Err(anyhow::anyhow!(
                        "Strict durability required: failed to load persisted machine from {}: {}",
                        path.display(),
                        err
                    ));
                }

                warn!(
                    error = %err,
                    path = %path.display(),
                    "Failed to load machine from rollout; creating fresh persistent machine"
                );
                match create_persistent_machine(
                    launch.process_path,
                    launch.model,
                    rollouts_dir,
                    rollout_cwd,
                    reasoning_effort,
                )
                .await
                {
                    Ok(machine) => machine,
                    Err(create_err) => {
                        warn!(
                            error = %create_err,
                            "Failed to create a persistent machine after rollout recovery; using an in-memory machine"
                        );
                        warnings.push(best_effort_durability_warning(&create_err));
                        AgentMachine::new()
                    }
                }
            }
        }
    } else {
        match create_persistent_machine(
            launch.process_path,
            launch.model,
            rollouts_dir,
            rollout_cwd,
            reasoning_effort,
        )
        .await
        {
            Ok(machine) => machine,
            Err(err) => {
                if durability_required {
                    return Err(anyhow::anyhow!(
                        "Strict durability required: failed to create persistent machine: {}",
                        err
                    ));
                }

                warn!(error = %err, "Failed to create persistent machine; using in-memory machine");
                warnings.push(best_effort_durability_warning(&err));
                AgentMachine::new()
            }
        }
    };

    Ok(AgentMachineStartupOutcome {
        metadata: RuntimeStartupMetadata::ready(
            launch.process_path.to_string(),
            launch.agent_path.to_string(),
            machine.rollout_id().map(str::to_string),
            machine.rollout_path().cloned(),
            AgentMachineDurabilityState {
                durable: machine.is_durable(),
                required: durability_required,
            },
            request_controls,
            warnings,
        ),
        machine,
    })
}

/// Start Agent Execution Engine over an already-assembled Process namespace.
///
/// This entry point does not create Kernel, `/proc`, `/srv`, AgentFS, or system
/// services. Alan OS Host owns those and supplies the complete environment.
pub fn spawn_with_namespace_environment(
    config: AgentProcessConfig,
    namespace: super::NamespaceRuntimeEnvironment,
    host_capabilities: crate::skills::SkillHostCapabilities,
    generation_capabilities: crate::llm::ProviderCapabilities,
) -> Result<RuntimeController> {
    spawn_with_prepared_runtime_environment(
        config,
        namespace,
        host_capabilities,
        generation_capabilities,
    )
}

fn spawn_with_prepared_runtime_environment(
    config: AgentProcessConfig,
    environment: NamespaceRuntimeEnvironment,
    host_capabilities: crate::skills::SkillHostCapabilities,
    _generation_capabilities: crate::llm::ProviderCapabilities,
) -> Result<RuntimeController> {
    let (sub_tx, mut sub_rx) = mpsc::channel::<Submission>(32);
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
    let (ready_tx, ready_rx) =
        oneshot::channel::<std::result::Result<RuntimeStartupMetadata, String>>();

    let resolved_launch = config.resolve_runtime_launch()?;
    let resolved_agent_definition = resolved_launch.agent_definition;
    let core_config = resolved_launch.core_config;
    let mut runtime_config = resolved_launch.runtime_config;
    runtime_config.policy_engine =
        if let Some(tree) = resolved_agent_definition.descriptor_tree.as_ref() {
            crate::policy::PolicyEngine::load_for_governance_from_file_tree(
                tree,
                &runtime_config.governance,
            )
        } else {
            crate::policy::PolicyEngine::load_for_governance_with_default_policy_path(
                resolved_agent_definition.root_dir.as_deref(),
                resolved_agent_definition.policy_path.as_deref(),
                &runtime_config.governance,
            )
        };
    let prompt_cache_persona_dirs = resolved_agent_definition.persona_dirs.clone();
    if core_config.memory.enabled
        && let Some(memory_dir) = core_config.memory.store_dir.as_deref()
        && let Err(err) = crate::prompts::ensure_memory_store_layout_at(memory_dir)
    {
        warn!(
            path = %memory_dir.display(),
            error = %err,
            "Failed to initialize Memory Store layout; continuing without bootstrap writes"
        );
    }
    let rollouts_dir = config
        .store_bindings
        .as_ref()
        .map(|stores| stores.rollouts.clone());
    let rollout_cwd = std::path::PathBuf::from(&config.launch_context.cwd);
    let recovery_rollout_path = config.recovery_rollout_path;
    let generation_capabilities = crate::provider_capabilities_for_config(&core_config);
    let mut prompt_cache =
        super::prompt_cache::PromptAssemblyCache::with_fixed_capability_view_and_overrides(
            resolved_agent_definition.capability_view.clone(),
            resolved_agent_definition.skill_overrides.clone(),
            prompt_cache_persona_dirs.clone(),
            host_capabilities,
        );
    prompt_cache.set_fixed_definition_persona_section(resolved_agent_definition.persona_context);
    prompt_cache.set_memory_store_dir(
        core_config
            .memory
            .enabled
            .then(|| core_config.memory.store_dir.clone())
            .flatten(),
    );

    // Spawn the main runtime task
    let task_handle = tokio::spawn(async move {
        let process_path = match environment.process_path() {
            Ok(path) => path,
            Err(err) => {
                let _ = ready_tx.send(Err(format!("{:#}", err)));
                return;
            }
        };
        let agent_path = environment.agent_path().to_string();
        let model = core_config.effective_model().to_string();
        let machine_request_controls = match crate::resolve_runtime_request_controls(
            &core_config,
            generation_capabilities,
            runtime_config.request_control_intent,
        ) {
            Ok(controls) => controls,
            Err(err) => {
                let _ = ready_tx.send(Err(format!("{:#}", err)));
                return;
            }
        };
        let startup = match initialize_agent_machine(
            AgentMachineLaunchContext {
                process_path: &process_path,
                agent_path: &agent_path,
                model: &model,
            },
            recovery_rollout_path.as_ref(),
            rollouts_dir.as_ref(),
            runtime_config.durability_required,
            Some(rollout_cwd.as_path()),
            machine_request_controls,
        )
        .await
        {
            Ok(startup) => startup,
            Err(err) => {
                let _ = ready_tx.send(Err(format!("{:#}", err)));
                return;
            }
        };
        let machine = startup.machine;

        // Build the transition context owned by this Process loop.
        let mut state = RuntimeLoopState {
            machine,
            environment,
            core_config,
            runtime_config,
            definition_persona_dirs: prompt_cache_persona_dirs.clone(),
            prompt_cache,
        };
        match super::ui_surfaces::initialize(state.namespace_environment()).await {
            Ok(()) => {}
            Err(err) => {
                let _ = ready_tx.send(Err(format!("{:#}", err)));
                return;
            }
        };

        info!(
            process_path = %state.process_path(),
            agent_path = %state.agent_path(),
            "Agent runtime started"
        );
        let _ = ready_tx.send(Ok(startup.metadata));

        // Main event loop with graceful shutdown support and interruptible submissions.
        let mut submissions_closed = false;
        let mut shutdown_requested = false;

        let mut queues = RuntimeSubmissionQueues::default();
        let input_environment = state.namespace_environment().clone();
        let (namespace_input_tx, mut namespace_input_rx) = mpsc::channel(1);
        let namespace_input_task = tokio::spawn(async move {
            loop {
                let submission = input_environment
                    .read_next_input_submission(InputMode::FollowUp)
                    .await;
                if namespace_input_tx.send(submission).await.is_err() {
                    break;
                }
            }
        });

        loop {
            let queued_item = if shutdown_requested {
                queues.pop_outer_deferred()
            } else if let Some(queued_item) = queues.pop_outer() {
                Some(queued_item)
            } else if let Some(namespace_resume) =
                read_pending_namespace_resume_submission(&state).await
            {
                match namespace_resume {
                    Ok(submission) => Some(QueuedRuntimeItem::Submission(submission)),
                    Err(err) => {
                        error!(
                            error = %format!("{err:#}"),
                            "Failed to read namespace answered request response"
                        );
                        None
                    }
                }
            } else if let Some(namespace_control) =
                read_pending_namespace_control_submission(state.namespace_environment()).await
            {
                match namespace_control {
                    Ok(submission) => Some(QueuedRuntimeItem::Submission(submission)),
                    Err(err) => {
                        error!(
                            error = %format!("{err:#}"),
                            "Failed to read namespace machine/ctl command"
                        );
                        None
                    }
                }
            } else if submissions_closed {
                None
            } else {
                let namespace_control = state.namespace_environment().clone();
                let poll_pending_namespace_response = state.machine.has_pending_interaction();
                tokio::select! {
                    submission = sub_rx.recv() => submission.map(QueuedRuntimeItem::Submission),
                    namespace_submission = namespace_input_rx.recv() => {
                        match namespace_submission {
                            Some(Ok(submission)) => Some(QueuedRuntimeItem::Submission(submission)),
                            Some(Err(err)) => {
                                error!(error = %format!("{err:#}"), "Failed to read namespace io/input frame");
                                None
                            }
                            None => None,
                        }
                    }
                    _ = tokio::time::sleep(NAMESPACE_PENDING_RESPONSE_POLL_INTERVAL) => {
                        match read_pending_namespace_control_submission(&namespace_control).await {
                            Some(Ok(submission)) => Some(QueuedRuntimeItem::Submission(submission)),
                            Some(Err(err)) => {
                                error!(
                                    error = %format!("{err:#}"),
                                    "Failed to read namespace machine/ctl command"
                                );
                                None
                            }
                            None if poll_pending_namespace_response => {
                                match read_pending_namespace_resume_submission(&state).await {
                                    Some(Ok(submission)) => Some(QueuedRuntimeItem::Submission(submission)),
                                    Some(Err(err)) => {
                                        error!(
                                            error = %format!("{err:#}"),
                                            "Failed to read namespace answered request response"
                                        );
                                        None
                                    }
                                    None => None,
                                }
                            }
                            None => None,
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        shutdown_requested = true;
                        submissions_closed = true;
                        None
                    }
                }
            };

            let Some(queued_item) = queued_item else {
                if shutdown_requested || submissions_closed {
                    if shutdown_requested {
                        info!(
                            process_path = %state.namespace_environment().agent_path(),
                            "Shutdown signal received, stopping runtime"
                        );
                    }
                    break;
                }
                continue;
            };

            match queued_item {
                QueuedRuntimeItem::Submission(submission) => {
                    debug!(?submission.id, "Received submission");
                    let accepts_inband = accepts_inband_submissions(&submission.op);

                    let cancel = CancellationToken::new();

                    let broker_for_submission = queues.active_turn_broker.clone();
                    let namespace_control = state.namespace_environment().clone();
                    let namespace_heartbeat = state.namespace_environment().clone();
                    let mut submission_fut = Box::pin(advance_accepted_submission(
                        &mut state,
                        submission,
                        &broker_for_submission,
                        &cancel,
                    ));
                    let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(5));
                    heartbeat_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

                    loop {
                        tokio::select! {
                            outcome = &mut submission_fut => {
                                drop(submission_fut);
                                let terminal_ui_result = match &outcome.result {
                                    Ok(TransitionCompletion::Paused) => Ok(()),
                                    Ok(TransitionCompletion::Completed) => super::ui_surfaces::turn_completed(
                                        &namespace_heartbeat,
                                        false,
                                    )
                                    .await,
                                    Err(err) => super::ui_surfaces::turn_failed(
                                        &namespace_heartbeat,
                                        &format!("Error handling submission: {err}"),
                                    )
                                    .await,
                                };
                                if let Err(err) = terminal_ui_result {
                                    warn!(error = %err, "Failed to write terminal runtime state");
                                }
                                if outcome.requeue_inband_submissions {
                                    let _ = queues
                                        .requeue_active_turn_leftovers(&mut state.machine)
                                        .await;
                                }
                                if let Err(e) = &outcome.result {
                                    let error_msg = format!("Error handling submission: {}", e);
                                    error!(error = %error_msg);
                                }
                                queues.outer_queue.extend(
                                    outcome
                                        .deferred_actions
                                        .into_iter()
                                        .map(QueuedRuntimeItem::Deferred),
                                );
                                break;
                            }
                            incoming = sub_rx.recv(), if !submissions_closed => {
                                match incoming {
                                    Some(incoming) => {
                                        if matches!(incoming.op, alan_agent_protocol::Op::Interrupt) {
                                            cancel.cancel();
                                        } else if accepts_inband
                                            && is_turn_inband_submission(&incoming.op)
                                        {
                                            if !queues.active_turn_broker.push(incoming.clone()).await {
                                                queues.push_outer_submission(incoming);
                                            }
                                        } else {
                                            queues.push_outer_submission(incoming);
                                        }
                                    }
                                    None => {
                                        submissions_closed = true;
                                        cancel.cancel();
                                    }
                                }
                            }
                            namespace_submission = namespace_input_rx.recv() => {
                                match namespace_submission {
                                    Some(Ok(incoming)) => {
                                        if accepts_inband && is_turn_inband_submission(&incoming.op) {
                                            if !queues.active_turn_broker.push(incoming.clone()).await {
                                                queues.push_outer_submission(incoming);
                                            }
                                        } else {
                                            queues.push_outer_submission(incoming);
                                        }
                                    }
                                    Some(Err(err)) => {
                                        let error_msg = format!("Failed to read namespace io/input frame: {err:#}");
                                        error!(error = %error_msg);
                                        let _ = super::ui_surfaces::warning(
                                            &namespace_heartbeat,
                                            error_msg,
                                        ).await;
                                    }
                                    None => {}
                                }
                            }
                            _ = tokio::time::sleep(NAMESPACE_PENDING_RESPONSE_POLL_INTERVAL) => {
                                match read_pending_namespace_control_submission(&namespace_control).await {
                                    Some(Ok(incoming)) => {
                                        // A machine/ctl interrupt must cancel the
                                        // running generation/tool immediately, like
                                        // an Op::Interrupt arriving on sub_rx.
                                        if matches!(incoming.op, alan_agent_protocol::Op::Interrupt) {
                                            cancel.cancel();
                                        } else if accepts_inband && is_turn_inband_submission(&incoming.op) {
                                            if !queues.active_turn_broker.push(incoming.clone()).await {
                                                queues.push_outer_submission(incoming);
                                            }
                                        } else {
                                            queues.push_outer_submission(incoming);
                                        }
                                    }
                                    Some(Err(err)) => {
                                        let error_msg = format!("Failed to read namespace machine/ctl command: {err:#}");
                                        error!(error = %error_msg);
                                        let _ = super::ui_surfaces::warning(
                                            &namespace_heartbeat,
                                            error_msg,
                                        ).await;
                                    }
                                    None => {}
                                }
                            }
                            _ = heartbeat_interval.tick() => {
                                if let Err(err) = super::ui_surfaces::heartbeat(&namespace_heartbeat).await {
                                    warn!(error = %err, "Failed to write runtime activity heartbeat");
                                }
                            }
                            _ = shutdown_rx.recv() => {
                                shutdown_requested = true;
                                submissions_closed = true;
                                cancel.cancel();
                            }
                        }

                        if shutdown_requested {
                            continue;
                        }
                    }
                }
                QueuedRuntimeItem::Deferred(action) => {
                    let action_for_requeue = action.clone();
                    let mut requeue_if_cancelled = false;
                    let cancel = CancellationToken::new();
                    let namespace_control = state.namespace_environment().clone();
                    let mut action_fut = Box::pin(run_deferred_runtime_action_with_cancel(
                        &mut state, action, &cancel,
                    ));

                    loop {
                        tokio::select! {
                            exit = &mut action_fut => {
                                drop(action_fut);
                                if should_requeue_deferred_action(requeue_if_cancelled, exit) {
                                    queues.push_outer_deferred(action_for_requeue);
                                }
                                break;
                            }
                            incoming = sub_rx.recv(), if !submissions_closed => {
                                match incoming {
                                    Some(incoming) => {
                                        if matches!(incoming.op, alan_agent_protocol::Op::Interrupt) {
                                            cancel.cancel();
                                        } else {
                                            requeue_if_cancelled = true;
                                            cancel.cancel();
                                            queues.push_outer_submission(incoming);
                                        }
                                    }
                                    None => {
                                        submissions_closed = true;
                                    }
                                }
                            }
                            namespace_submission = namespace_input_rx.recv() => {
                                match namespace_submission {
                                    Some(Ok(incoming)) => {
                                        requeue_if_cancelled = true;
                                        cancel.cancel();
                                        queues.push_outer_submission(incoming);
                                    }
                                    Some(Err(err)) => {
                                        error!(
                                            error = %format!("{err:#}"),
                                            "Failed to read namespace io/input frame during deferred action"
                                        );
                                    }
                                    None => {}
                                }
                            }
                            _ = tokio::time::sleep(NAMESPACE_PENDING_RESPONSE_POLL_INTERVAL) => {
                                match read_pending_namespace_control_submission(&namespace_control).await {
                                    Some(Ok(incoming)) => {
                                        // Mirror the sub_rx arm: a machine/ctl
                                        // interrupt just cancels the deferred
                                        // action; other control ops preempt and
                                        // requeue it.
                                        if matches!(incoming.op, alan_agent_protocol::Op::Interrupt) {
                                            cancel.cancel();
                                        } else {
                                            requeue_if_cancelled = true;
                                            cancel.cancel();
                                            queues.push_outer_submission(incoming);
                                        }
                                    }
                                    Some(Err(err)) => {
                                        error!(
                                            error = %format!("{err:#}"),
                                            "Failed to read namespace machine/ctl command during deferred action"
                                        );
                                    }
                                    None => {}
                                }
                            }
                            _ = shutdown_rx.recv() => {
                                shutdown_requested = true;
                                submissions_closed = true;
                            }
                        }
                    }
                }
            }
        }

        namespace_input_task.abort();
        let _ = namespace_input_task.await;
        info!(
            process_path = %state.namespace_environment().agent_path(),
            "Agent runtime stopped"
        );
        state.machine.flush().await;
    });

    Ok(RuntimeController::spawned(
        sub_tx,
        shutdown_tx,
        task_handle,
        ready_rx,
    ))
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
