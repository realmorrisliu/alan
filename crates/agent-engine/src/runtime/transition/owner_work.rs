//! The explicit source-owner program runs inside the ordinary accepted Machine transition.
use super::*;
use crate::agent_machine::owner_work::{Evidence, FallbackEvidence, Outcome, Snapshot, digest};
use crate::runtime::tool_policy::{SandboxConfinement, ToolPolicyDecision, evaluate_tool_policy};
use crate::tape::{Message, ToolRequest};
use alan_agent_protocol::{ContentPart, InputIntent, InputMode, OwnerWorkRequest};
use anyhow::{Context, ensure};
use std::time::Duration;
use tokio::time::Instant;

#[derive(Debug, thiserror::Error)]
#[error("generation fallback lacks a reliable terminal result; never retry implicitly")]
struct FallbackUncertainty;

pub(super) async fn handle<E, F>(
    state: &mut RuntimeLoopState,
    submission: &Submission,
    emit: &mut E,
    cancel: &CancellationToken,
) -> Result<bool>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    if let Op::Resume { request_id, .. } = &submission.op
        && state.machine.owner_work.as_ref().is_some_and(|w| {
            w.owned_request.as_ref() == Some(request_id)
                && !matches!(w.outcome, Outcome::Waiting { .. })
        })
    {
        anyhow::bail!("owner work request is already terminal");
    }
    if let Op::Resume {
        request_id,
        content,
    } = &submission.op
        && let Some(mut work) = state.machine.owner_work.clone()
        && let Outcome::Waiting {
            request_id: owned, ..
        } = &work.outcome
        && owned == request_id
    {
        let owner = response_owner(content).and_then(|owner| {
            ensure!(
                work.request.candidates.iter().any(|c| c.id == owner),
                "owner response outside candidates"
            );
            Ok(owner)
        });
        ensure!(
            state.machine.pending_yield(request_id).is_some(),
            "owner response lacks an owned pending request"
        );
        // Admission is still correlated with the original work, never the delivery UUID.
        state.machine.accept_submission(work.work_id.clone());
        let result = match &owner {
            Ok(owner) => validate_owner(state, &work, owner, emit, cancel).await,
            Err(error) => Err(anyhow::anyhow!(error.to_string())),
        };
        work.outcome = if cancel.is_cancelled() {
            state.machine.mark_submission_cancelled();
            Outcome::Cancelled
        } else {
            match result {
                Ok(()) => Outcome::Completed {
                    owner: owner.unwrap(),
                },
                Err(error) => Outcome::Failed {
                    reason: error.to_string(),
                },
            }
        };
        let failure = if let Outcome::Failed { reason } = &work.outcome {
            Some(reason.clone())
        } else {
            None
        };
        settle(state, work, emit).await?;
        state.machine.take_pending(request_id);
        if let Some(reason) = failure {
            anyhow::bail!(reason);
        }
        return Ok(true);
    }
    let parts = match &submission.op {
        Op::Input { parts, .. } | Op::Turn { parts, .. } => parts,
        _ => return Ok(false),
    };
    let [ContentPart::Structured { data }] = parts.as_slice() else {
        return Ok(false);
    };
    let Some(request) = data.get("owner_work_v1") else {
        return Ok(false);
    };
    ensure!(
        submission.intent == InputIntent::ForceAgent
            && matches!(
                submission.op,
                Op::Input {
                    mode: InputMode::FollowUp,
                    ..
                }
            )
            && data.as_object().is_some_and(|o| o.len() == 1),
        "work requires explicit typed admission"
    );
    let request: OwnerWorkRequest = serde_json::from_value(request.clone())?;
    request.validate().map_err(anyhow::Error::msg)?;
    ensure!(
        !state.machine.has_pending_interaction(),
        "owner work cannot replace an owned wait"
    );
    let mut work = Snapshot {
        version: 1,
        work_id: submission.id.clone(),
        source_rollout_id: state
            .machine
            .rollout_id()
            .context("owner work needs durable storage")?
            .into(),
        request_sha256: digest(&serde_json::to_vec(&request)?),
        request,
        evidence: vec![],
        evaluator_calls: 0,
        generation_calls: 0,
        fallback: None,
        owned_request: None,
        outcome: Outcome::Started,
    };
    state.machine.persist_owner_work(work.clone()).await?;
    state
        .environment
        .publish_work(Some(work.projection()?))
        .await?;
    state.machine.add_user_message_parts(parts.clone());
    state.machine.set_turn_activity(TurnActivityState::Running);
    crate::runtime::ui_surfaces::turn_started(&state.agent_files()).await?;
    emit(Event::TurnStarted {}).await;
    let result = advance(state, &mut work, emit, cancel).await;
    if cancel.is_cancelled() {
        state.machine.mark_submission_cancelled();
    }
    // Cancellation never settles an operation whose terminal acknowledgement is uncertain.
    if let Err(error) = &result
        && (error
            .downcast_ref::<NamespaceEvaluationUncertainty>()
            .is_some()
            || error.downcast_ref::<FallbackUncertainty>().is_some())
    {
        return result.map(|_| true);
    }
    if cancel.is_cancelled() {
        work.outcome = Outcome::Cancelled;
    } else if let Err(error) = result {
        work.outcome = Outcome::Failed {
            reason: error.to_string(),
        };
    }
    let failure = if let Outcome::Failed { reason } = &work.outcome {
        Some(reason.clone())
    } else {
        None
    };
    settle(state, work, emit).await?;
    if let Some(reason) = failure {
        anyhow::bail!(reason);
    }
    Ok(true)
}

fn response_owner(content: &[ContentPart]) -> Result<String> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Direct {
        owner: String,
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Answer {
        question_id: String,
        value: String,
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Legacy {
        answers: Vec<Answer>,
    }
    let value = match content {
        [ContentPart::Text { text }] => return Ok(text.clone()),
        [ContentPart::Structured { data }] => data.clone(),
        _ => anyhow::bail!("owner response requires one value"),
    };
    if let Ok(direct) = serde_json::from_value::<Direct>(value.clone()) {
        return Ok(direct.owner);
    }
    let legacy: Legacy = serde_json::from_value(value)?;
    ensure!(
        legacy.answers.len() == 1 && legacy.answers[0].question_id == "owner",
        "owner response requires one owned question"
    );
    Ok(legacy.answers.into_iter().next().unwrap().value)
}

async fn advance<E, F>(
    state: &mut RuntimeLoopState,
    work: &mut Snapshot,
    emit: &mut E,
    cancel: &CancellationToken,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    for candidate in &work.request.candidates {
        for source in &candidate.sources {
            let content = read_source(state, source, emit, cancel).await?;
            ensure!(
                work.evidence.iter().map(|e| e.content.len()).sum::<usize>() + content.len()
                    <= 128 * 1024,
                "work source capture exceeds document budget"
            );
            work.evidence.push(Evidence {
                owner: candidate.id.clone(),
                source: source.clone(),
                sha256: digest(content.as_bytes()),
                content,
            });
        }
    }
    state.machine.persist_owner_work(work.clone()).await?;
    if let Some(owner) = literal_owner(work) {
        validate_owner(state, work, &owner, emit, cancel).await?;
        work.outcome = Outcome::Completed { owner };
        return Ok(());
    }
    let start = Instant::now();
    let expires = start + Duration::from_millis(30_000);
    let authority = state
        .environment
        .model_bindings
        .lock()
        .await
        .authority
        .clone()
        .context("evaluation authority unavailable")?;
    let captured = bounded(
        expires,
        cancel,
        authority.capture_evaluation(&work.request.evaluator_profile),
    )
    .await?;
    let candidates = work
        .request
        .candidates
        .iter()
        .map(|candidate| {
            let text = work
                .evidence
                .iter()
                .filter(|e| e.owner == candidate.id)
                .map(|e| {
                    format!(
                        "{}:{}-{}\n{}",
                        e.source.path, e.source.start_line, e.source.end_line, e.content
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let mut end = text.len().min(4096 - 32);
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            alan_llm::EvaluationCandidate {
                id: candidate.id.clone(),
                description: format!("truncated={}\n{}", end < text.len(), &text[..end]),
            }
        })
        .collect::<Vec<_>>();
    let request = alan_llm::ChoiceEvaluationRequest {
        input: work.request.question.clone(),
        candidates,
    };
    let observation = crate::agent_machine::evaluation::Observation {
        identity: crate::agent_machine::evaluation::Identity {
            source_rollout_id: work.source_rollout_id.clone(),
            submission_id: work.work_id.clone(),
            input_sha256: work.request_sha256.clone(),
            surface: crate::runtime::EvaluationSurface::MachineControl,
            operation_id: None,
            callable: captured.identity.clone(),
            schema: "choice.v1".into(),
            deadline_ms: 30_000,
            candidates: request
                .candidates
                .iter()
                .map(|c| crate::agent_machine::evaluation::Candidate {
                    id: c.id.clone(),
                    description: c.description.clone(),
                })
                .collect(),
        },
        outcome: crate::agent_machine::evaluation::Outcome::Started,
        elapsed_ms: None,
        usage: None,
    };
    work.evaluator_calls = 1;
    state.machine.persist_owner_work(work.clone()).await?;
    crate::runtime::shadow_evaluation::evaluate_captured_choice(
        &mut state.machine,
        &state.environment,
        NamespaceRuntimeEnvironment::new(
            captured.root,
            state.environment.agent_path(),
            captured.connection,
        ),
        request.clone(),
        observation,
        (start, expires),
        cancel,
    )
    .await?;
    let observation: crate::agent_machine::evaluation::Observation = serde_json::from_value({
        let mut value = state
            .machine
            .evaluation_observation
            .clone()
            .context("evaluation lacks acknowledged result")?;
        value.as_object_mut().unwrap().remove("cost_microusd");
        value
    })?;
    match observation.outcome {
        crate::agent_machine::evaluation::Outcome::Selected { candidate_id } => {
            validate_owner(state, work, &candidate_id, emit, cancel).await?;
            work.outcome = Outcome::Completed {
                owner: candidate_id,
            };
        }
        crate::agent_machine::evaluation::Outcome::NoMatch => {
            let mut fallback = alan_llm::GenerationRequest::new()
                .with_system_prompt("Choose an owner only from the supplied candidates and source evidence. Return exactly JSON {\"owner\":\"candidate-id\"}. No tools or prose.")
                .with_user_message(serde_json::json!({"input":request.input,"candidates":request.candidates.iter().map(|c| serde_json::json!({"id":c.id,"description":c.description})).collect::<Vec<_>>()}).to_string()).with_max_tokens(64);
            let binding = state
                .environment
                .active_binding
                .read()
                .expect("binding snapshot")
                .clone();
            if let Some((binding, _)) = &binding {
                fallback.reasoning = binding.request_controls.reasoning;
            }
            let fallback_expires = Instant::now() + Duration::from_millis(30_000);
            let quote = match &binding {
                Some((_, callable)) => {
                    bounded(
                        fallback_expires,
                        cancel,
                        authority.quote_generation(&callable.identity, &fallback),
                    )
                    .await?
                }
                None => None,
            };
            if quote.as_ref().is_some_and(|q| {
                q.cost_microusd <= 1000 && !q.provenance.is_empty() && q.provenance.len() <= 1024
            }) {
                ensure!(!cancel.is_cancelled(), "owner work cancelled");
                work.generation_calls = 1;
                work.fallback = Some(FallbackEvidence {
                    callable:binding.as_ref().unwrap().1.identity.clone(),
                    request_sha256:digest(serde_json::to_string(&serde_json::json!({"system_prompt":fallback.system_prompt,"messages":fallback.messages,"max_tokens":fallback.max_tokens,"reasoning":fallback.reasoning,"temperature":fallback.temperature,"extra_params":fallback.extra_params,"tools":fallback.tools})).unwrap().as_bytes()),
                    quote:quote.unwrap(),
                });
                bounded(
                    fallback_expires,
                    cancel,
                    state.machine.persist_owner_work(work.clone()),
                )
                .await?;
                let result = generate_fallback(state, &fallback, fallback_expires, cancel).await?;
                ensure!(
                    result.tool_calls.is_empty() && result.content.len() <= 8192,
                    "invalid owner generation result"
                );
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Selection {
                    owner: String,
                }
                let selected: Selection = serde_json::from_str(&result.content)?;
                validate_owner(state, work, &selected.owner, emit, cancel).await?;
                work.outcome = Outcome::Completed {
                    owner: selected.owner,
                };
            } else {
                wait(state, work, "generation_budget_unavailable", emit).await?;
            }
        }
        outcome => anyhow::bail!("evaluation did not select an owner: {outcome:?}"),
    }
    Ok(())
}

async fn read_source<E, F>(
    state: &mut RuntimeLoopState,
    source: &alan_agent_protocol::OwnerSourceRange,
    emit: &mut E,
    cancel: &CancellationToken,
) -> Result<String>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    ensure!(
        !cancel.is_cancelled(),
        "owner work cancelled before source dispatch"
    );
    let arguments = serde_json::json!({"path":source.path,"offset":source.start_line,"limit":source.end_line-source.start_line+1});
    let tools = state.tool_execution();
    let package = tools
        .discover_packages()
        .await?
        .into_iter()
        .find(|p| p.name == "read_file")
        .context("source read Tool unavailable")?;
    ensure!(
        tools.resolve_capability(&package, &arguments) == alan_agent_protocol::ToolCapability::Read,
        "source Tool is not read-only"
    );
    ensure!(
        matches!(
            evaluate_tool_policy(
                &state.runtime_config.policy_engine,
                &state.runtime_config.governance,
                "read_file",
                &arguments,
                alan_agent_protocol::ToolCapability::Read,
                tools.default_cwd().as_deref(),
                SandboxConfinement::detect()
            ),
            ToolPolicyDecision::Allow { .. }
        ),
        "source read requires additional authorization"
    );
    let call = NormalizedToolCall {
        id: uuid::Uuid::new_v4().to_string(),
        name: "read_file".into(),
        arguments: arguments.clone(),
    };
    state
        .machine
        .add_assistant_message_with_tool_calls_and_reasoning(
            "",
            vec![ToolRequest {
                id: call.id.clone(),
                name: call.name.clone(),
                arguments,
            }],
            None,
            None,
            &[],
        );
    let outcome = super::orchestrate_tool_call(
        &mut ToolLoopGuard::new(None, 1),
        state,
        &call,
        ToolOrchestratorInputs {
            explicit_command: false,
            cancel,
            steering_broker: None,
        },
        false,
        false,
        emit,
    )
    .await?;
    ensure!(
        matches!(outcome, ToolOrchestratorOutcome::ContinueToolBatch { .. }),
        "source read did not complete"
    );
    let value = state
        .machine
        .messages()
        .iter()
        .rev()
        .find_map(|m| {
            if let Message::Tool { responses } = m {
                responses
                    .iter()
                    .find(|r| r.id == call.id)
                    .and_then(|r| serde_json::from_str::<serde_json::Value>(&r.text_content()).ok())
            } else {
                None
            }
        })
        .context("source read result unavailable")?;
    ensure!(
        value["success"] == true
            && value["start_line"] == source.start_line
            && value["end_line"] == source.end_line,
        "source range unavailable or incomplete"
    );
    let content = value["content"]
        .as_str()
        .context("source bytes unavailable")?;
    ensure!(
        content.len() <= 64 * 1024
            && content.split('\n').count() == (source.end_line - source.start_line + 1) as usize,
        "source range exceeds capture bound or was projected"
    );
    Ok(content.into())
}

async fn validate_owner<E, F>(
    state: &mut RuntimeLoopState,
    work: &Snapshot,
    owner: &str,
    emit: &mut E,
    cancel: &CancellationToken,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let candidate = work
        .request
        .candidates
        .iter()
        .find(|c| c.id == owner)
        .context("selected owner outside captured candidates")?;
    for source in &candidate.sources {
        let evidence = work
            .evidence
            .iter()
            .find(|e| e.owner == owner && e.source == *source)
            .context("owner lacks captured evidence")?;
        ensure!(
            digest(read_source(state, source, emit, cancel).await?.as_bytes()) == evidence.sha256,
            "owner evidence changed or authority was revoked"
        );
    }
    Ok(())
}

// ponytail: this literal control recognizes line-oriented declarations; qualify a parsed-language adapter before broad arbitrary-source ownership support.
fn literal_owner(work: &Snapshot) -> Option<String> {
    let name = work
        .request
        .question
        .strip_prefix("Which crate defines ")?
        .strip_suffix('?')?;
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return None;
    }
    let mut owners = std::collections::HashSet::new();
    for evidence in &work.evidence {
        if evidence.content.lines().any(|line| {
            let mut words = line.split_whitespace();
            words.next() == Some("pub")
                && matches!(words.next(), Some("struct" | "enum" | "trait"))
                && words.next().is_some_and(|word| {
                    word.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                        .next()
                        == Some(name)
                })
        }) {
            owners.insert(evidence.owner.clone());
        }
    }
    (owners.len() == 1).then(|| owners.into_iter().next().unwrap())
}

async fn wait<E, F>(
    state: &mut RuntimeLoopState,
    work: &mut Snapshot,
    reason: &str,
    emit: &mut E,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let pending = work.waiting_request();
    let request_id = state
        .agent_files()
        .write_structured_input_request(&pending)
        .await?;
    work.owned_request = Some(request_id.clone());
    work.outcome = Outcome::Waiting {
        request_id: request_id.clone(),
        reason: reason.into(),
    };
    if let Err(error) = state.machine.persist_owner_wait(work.clone()).await {
        if let Some(live) = &mut state.machine.owner_work {
            live.outcome = Outcome::Interrupted;
        }
        state
            .agent_files()
            .cancel_request(&request_id)
            .await
            .context("cancel work request whose wait was not acknowledged")?;
        return Err(error);
    }
    state
        .machine
        .set_structured_input_for_request(&request_id, pending.clone());
    emit(Event::Yield {
        request_id,
        kind: alan_agent_protocol::YieldKind::StructuredInput,
        payload: serde_json::to_value(
            crate::runtime::interaction_tools::structured_input_yield_payload(
                pending.title,
                pending.prompt,
                pending.questions,
            ),
        )?,
    })
    .await;
    Ok(())
}

async fn settle<E, F>(state: &mut RuntimeLoopState, work: Snapshot, emit: &mut E) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    state.machine.persist_owner_work(work.clone()).await?;
    state
        .environment
        .publish_work(Some(work.projection()?))
        .await?;
    if matches!(work.outcome, Outcome::Waiting { .. }) {
        state.machine.set_turn_activity(TurnActivityState::Paused);
        crate::runtime::ui_surfaces::paused(&state.agent_files(), Some(&state.machine)).await?;
    } else {
        state.machine.set_turn_activity(TurnActivityState::Idle);
        crate::runtime::ui_surfaces::turn_completed(
            &state.agent_files(),
            matches!(work.outcome, Outcome::Cancelled),
        )
        .await?;
        emit(Event::TurnCompleted { summary: None }).await;
    }
    Ok(())
}

pub(super) async fn generate_fallback(
    state: &RuntimeLoopState,
    request: &alan_llm::GenerationRequest,
    expires: Instant,
    cancel: &CancellationToken,
) -> Result<alan_llm::GenerationResponse> {
    ensure!(
        !cancel.is_cancelled(),
        "owner work cancelled before fallback dispatch"
    );
    ensure!(
        Instant::now() < expires,
        "owner work deadline exceeded before fallback dispatch"
    );
    let stop = cancel.child_token();
    let generation = state.namespace_generation();
    let operation = generation.generate_controlled(request, 0, &stop);
    tokio::pin!(operation);
    tokio::select! { biased;
        _ = cancel.cancelled() => {},
        _ = tokio::time::sleep_until(expires) => {},
        result = &mut operation => return result.map_err(|error| error.context(FallbackUncertainty)),
    }
    // Let existing generation control abort its allocated operation before dropping it.
    stop.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(1), &mut operation).await;
    Err(FallbackUncertainty.into())
}

async fn bounded<T>(
    expires: Instant,
    cancel: &CancellationToken,
    future: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    tokio::select! { biased;
        _ = cancel.cancelled() => anyhow::bail!("owner work cancelled"),
        _ = tokio::time::sleep_until(expires) => anyhow::bail!("owner work deadline exceeded"),
        result = future => result,
    }
}
