use std::{collections::VecDeque, sync::Arc, time::Duration};

use alan_agent_protocol::{Event, InputMode, Op, Submission};
use anyhow::Result;
use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;

use super::transition::{RuntimeLoopState, handle_submission_with_cancel_and_steering};
use super::turn_support::cancel_current_task;
use crate::agent_machine::AgentMachine;

const MAX_BROKERED_INBAND_USER_INPUTS: usize = 16;
pub(super) const MAX_BUFFERED_INBAND_USER_INPUTS: usize = 16;
pub(super) const NAMESPACE_PENDING_RESPONSE_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Clone)]
pub(super) struct TurnInputBroker {
    inner: Arc<TurnInputBrokerInner>,
}

struct TurnInputBrokerInner {
    queue: Mutex<VecDeque<Submission>>,
    notify: Notify,
}

impl Default for TurnInputBroker {
    fn default() -> Self {
        Self {
            inner: Arc::new(TurnInputBrokerInner {
                queue: Mutex::new(VecDeque::new()),
                notify: Notify::new(),
            }),
        }
    }
}

impl TurnInputBroker {
    pub(super) async fn push(&self, submission: Submission) -> bool {
        let mut guard = self.inner.queue.lock().await;
        if is_brokered_input(&submission.op)
            && guard
                .iter()
                .filter(|queued| is_brokered_input(&queued.op))
                .count()
                >= MAX_BROKERED_INBAND_USER_INPUTS
        {
            return false;
        }
        guard.push_back(submission);
        drop(guard);
        self.inner.notify.notify_one();
        true
    }

    pub(super) async fn recv(&self, cancel: &CancellationToken) -> Option<Submission> {
        loop {
            if let Some(submission) = self.try_pop().await {
                return Some(submission);
            }

            tokio::select! {
                _ = cancel.cancelled() => return None,
                _ = self.inner.notify.notified() => {}
            }
        }
    }

    pub(super) async fn clear(&self) {
        self.inner.queue.lock().await.clear();
    }

    pub(super) async fn drain(&self) -> VecDeque<Submission> {
        std::mem::take(&mut *self.inner.queue.lock().await)
    }

    pub(super) async fn try_recv(&self) -> Option<Submission> {
        self.try_pop().await
    }

    async fn try_pop(&self) -> Option<Submission> {
        self.inner.queue.lock().await.pop_front()
    }
}

pub(super) fn is_turn_resume_submission(op: &Op) -> bool {
    matches!(op, Op::Resume { .. })
}

pub(super) fn is_turn_inband_submission(op: &Op) -> bool {
    is_turn_resume_submission(op) || is_brokered_input(op)
}

fn is_brokered_input(op: &Op) -> bool {
    matches!(
        op,
        Op::Input {
            mode: InputMode::Steer | InputMode::FollowUp,
            ..
        }
    )
}

pub(super) async fn drive_turn_submission_with_cancel<E, F>(
    state: &mut RuntimeLoopState,
    initial_submission: Submission,
    broker: &TurnInputBroker,
    emit: &mut E,
    cancel: &CancellationToken,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    broker.clear().await;
    let _ = state.machine.clear_buffered_inband_submissions();
    handle_submission_with_cancel_and_steering(
        state,
        initial_submission,
        emit,
        cancel,
        Some(broker),
    )
    .await?;

    loop {
        let next_submission = if state.machine.has_pending_interaction() {
            next_pending_interaction_submission(state, broker, emit, cancel).await?
        } else if let Some(buffered) = state.machine.pop_buffered_inband_submission() {
            Some(buffered)
        } else {
            broker.try_recv().await
        };

        let Some(next_submission) = next_submission else {
            break;
        };
        state.machine.accept_submission(next_submission.id.clone());

        handle_submission_with_cancel_and_steering(
            state,
            next_submission,
            emit,
            cancel,
            Some(broker),
        )
        .await?;
    }

    Ok(())
}

async fn next_pending_interaction_submission<E, F>(
    state: &mut RuntimeLoopState,
    broker: &TurnInputBroker,
    emit: &mut E,
    cancel: &CancellationToken,
) -> Result<Option<Submission>>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    loop {
        if let Some(submission) = namespace_pending_resume_submission(state).await? {
            return Ok(Some(submission));
        }

        tokio::select! {
            incoming = broker.recv(cancel) => {
                let Some(incoming) = incoming else {
                    if cancel.is_cancelled() && state.machine.has_pending_interaction() {
                        // Report and clear buffered in-turn submissions before cancel_current_task
                        // resets Machine turn state, otherwise the drop count under-reports.
                        emit_dropped_in_turn_submissions(emit, &mut state.machine, broker).await;
                        let agent_files = state.agent_files();
                        cancel_current_task(&mut state.machine, &agent_files, emit).await?;
                        return Ok(None);
                    }
                    emit_dropped_in_turn_submissions(emit, &mut state.machine, broker).await;
                    return Ok(None);
                };

                if is_turn_resume_submission(&incoming.op) {
                    return Ok(Some(incoming));
                }

                if is_brokered_input(&incoming.op)
                    && state.machine.buffered_inband_user_input_count()
                        >= MAX_BUFFERED_INBAND_USER_INPUTS
                {
                    emit(Event::Error {
                        message: format!(
                            "Too many queued in-turn user inputs (limit={MAX_BUFFERED_INBAND_USER_INPUTS}); dropping newest input."
                        ),
                        recoverable: true,
                    })
                    .await;
                    continue;
                }
                state.machine.push_buffered_inband_submission(incoming);
            }
            _ = tokio::time::sleep(NAMESPACE_PENDING_RESPONSE_POLL_INTERVAL) => {}
        }
    }
}

pub(super) async fn namespace_pending_resume_submission(
    state: &RuntimeLoopState,
) -> Result<Option<Submission>> {
    let agent_files = state.agent_files();

    for request_id in state.machine.pending_request_ids() {
        if let Some(submission) = agent_files
            .resume_submission_from_answered_request(&request_id)
            .await?
        {
            return Ok(Some(submission));
        }
    }

    Ok(None)
}

async fn emit_dropped_in_turn_submissions<E, F>(
    emit: &mut E,
    machine: &mut AgentMachine,
    broker: &TurnInputBroker,
) where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let dropped_buffered = machine.clear_buffered_inband_submissions();
    let dropped_brokered = broker.drain().await.len();
    let dropped_total = dropped_buffered + dropped_brokered;
    if dropped_total > 0 {
        emit(Event::Error {
            message: format!(
                "Dropped {dropped_total} in-turn buffered submissions due turn cancellation or shutdown."
            ),
            recoverable: true,
        })
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use alan_agentfs::AgentFs;
    use alan_ap::InProcessTransport;
    use alan_kernel::{Access, MountFs, Namespace};
    use alan_shell::Shell;

    #[test]
    fn test_inband_and_resume_submission_classification() {
        assert!(is_turn_resume_submission(&Op::Resume {
            request_id: "latest".to_string(),
            content: vec![alan_agent_protocol::ContentPart::structured(
                serde_json::json!({"choice": "approve"})
            )],
        }));
        assert!(is_turn_inband_submission(&Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text("follow up")],
            mode: InputMode::FollowUp,
        }));
        assert!(is_turn_inband_submission(&Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text("steer")],
            mode: InputMode::Steer,
        }));
        assert!(!is_turn_inband_submission(&Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text("next turn")],
            mode: InputMode::NextTurn,
        }));
        assert!(is_turn_inband_submission(&Op::Resume {
            request_id: "latest".to_string(),
            content: vec![alan_agent_protocol::ContentPart::structured(
                serde_json::json!({"choice": "approve"})
            )],
        }));
        assert!(is_turn_resume_submission(&Op::Resume {
            request_id: "r1".to_string(),
            content: vec![alan_agent_protocol::ContentPart::structured(
                serde_json::json!({"answers": []})
            )],
        }));
        assert!(is_turn_resume_submission(&Op::Resume {
            request_id: "c1".to_string(),
            content: vec![alan_agent_protocol::ContentPart::structured(
                serde_json::json!({"success": true})
            )],
        }));
    }

    #[tokio::test]
    async fn test_turn_input_broker_roundtrip_and_clear() {
        let broker = TurnInputBroker::default();
        assert!(
            broker
                .push(Submission {
                    id: "sub-1".to_string(),
                    op: Op::Resume {
                        request_id: "latest".to_string(),
                        content: vec![alan_agent_protocol::ContentPart::structured(
                            serde_json::json!({"choice": "approve"}),
                        )],
                    },
                })
                .await
        );

        let cancel = CancellationToken::new();
        let got = broker.recv(&cancel).await.expect("queued submission");
        assert_eq!(got.id, "sub-1");

        assert!(
            broker
                .push(Submission {
                    id: "sub-2".to_string(),
                    op: Op::Input {
                        parts: vec![alan_agent_protocol::ContentPart::text("follow up")],
                        mode: InputMode::Steer,
                    },
                })
                .await
        );
        let got = broker.try_recv().await.expect("queued submission");
        assert_eq!(got.id, "sub-2");

        assert!(
            broker
                .push(Submission {
                    id: "sub-3".to_string(),
                    op: Op::Resume {
                        request_id: "r1".to_string(),
                        content: vec![alan_agent_protocol::ContentPart::structured(
                            serde_json::json!({"answers": []}),
                        )],
                    },
                })
                .await
        );
        broker.clear().await;
        cancel.cancel();
        assert!(broker.recv(&cancel).await.is_none());
    }

    #[tokio::test]
    async fn test_turn_input_broker_caps_user_inputs_but_allows_resume_submissions() {
        let broker = TurnInputBroker::default();
        for idx in 0..MAX_BROKERED_INBAND_USER_INPUTS {
            assert!(
                broker
                    .push(Submission {
                        id: format!("u-{idx}"),
                        op: Op::Input {
                            parts: vec![alan_agent_protocol::ContentPart::text(format!(
                                "msg {idx}"
                            ))],
                            mode: InputMode::Steer,
                        },
                    })
                    .await
            );
        }

        assert!(
            !broker
                .push(Submission {
                    id: "u-overflow".to_string(),
                    op: Op::Input {
                        parts: vec![alan_agent_protocol::ContentPart::text("overflow")],
                        mode: InputMode::Steer,
                    },
                })
                .await
        );
        assert!(
            broker
                .push(Submission {
                    id: "resume-1".to_string(),
                    op: Op::Resume {
                        request_id: "latest".to_string(),
                        content: vec![alan_agent_protocol::ContentPart::structured(
                            serde_json::json!({"choice": "approve"}),
                        )],
                    },
                })
                .await
        );
    }

    #[tokio::test]
    async fn test_emit_dropped_in_turn_submissions_reports_count() {
        let broker = TurnInputBroker::default();
        let mut machine = AgentMachine::new();
        machine.push_buffered_inband_submission(Submission {
            id: "u-1".to_string(),
            op: Op::Input {
                parts: vec![alan_agent_protocol::ContentPart::text("queued")],
                mode: InputMode::Steer,
            },
        });
        assert!(
            broker
                .push(Submission {
                    id: "c-1".to_string(),
                    op: Op::Resume {
                        request_id: "latest".to_string(),
                        content: vec![alan_agent_protocol::ContentPart::structured(
                            serde_json::json!({"choice": "approve"}),
                        )],
                    },
                })
                .await
        );

        let mut events = Vec::new();
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        emit_dropped_in_turn_submissions(&mut emit, &mut machine, &broker).await;

        assert_eq!(machine.clear_buffered_inband_submissions(), 0);
        assert!(broker.try_recv().await.is_none());
        assert!(events.iter().any(|event| matches!(
            event,
            Event::Error { message, recoverable }
                if *recoverable && message.contains("Dropped 2 in-turn buffered submissions")
        )));
    }

    #[tokio::test]
    async fn namespace_answered_request_unblocks_pending_interaction_wait() {
        let agentfs = Arc::new(AgentFs::new());
        let mut ns = Namespace::new();
        ns.mount(
            "/agent/1",
            InProcessTransport::new(agentfs),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
        let shell = Shell::new(root.clone());
        let environment =
            super::super::transition::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");

        let request_id = environment
            .agent_files()
            .write_request(super::super::transition::NamespaceRequestRecord::new(
                "structured_input",
                "Provide the missing value",
            ))
            .await
            .unwrap();
        let mut machine = AgentMachine::new();
        machine.set_structured_input(crate::approval::PendingStructuredInputRequest {
            request_id: request_id.clone(),
            title: "Missing value".to_string(),
            prompt: "Provide the missing value".to_string(),
            questions: Vec::new(),
        });

        let mut state = RuntimeLoopState {
            machine,
            environment,
            core_config: crate::Config::default(),
            runtime_config: super::super::RuntimeConfig::default(),
            definition_persona_dirs: Vec::new(),
            prompt_cache: super::super::prompt_cache::PromptAssemblyCache::new(Vec::new()),
        };
        let broker = TurnInputBroker::default();
        let cancel = CancellationToken::new();
        let mut events = Vec::new();
        let mut emit = |event| {
            events.push(event);
            async {}
        };

        let response_path = format!("/agent/1/requests/{request_id}/response");
        let waiter = next_pending_interaction_submission(&mut state, &broker, &mut emit, &cancel);
        let writer = async {
            tokio::time::sleep(Duration::from_millis(25)).await;
            shell
                .write(
                    &response_path,
                    br#"{"answers":[{"question_id":"q1","value":"from agentfs"}]}"#,
                )
                .await
                .unwrap();
        };
        let (submission, _) = tokio::time::timeout(Duration::from_secs(1), async {
            tokio::join!(waiter, writer)
        })
        .await
        .expect("namespace response should unblock pending wait");
        let submission = submission
            .unwrap()
            .expect("answered request should become next submission");

        match submission.op {
            Op::Resume {
                request_id: resumed_id,
                content,
            } => {
                assert_eq!(resumed_id, request_id);
                assert_eq!(
                    content,
                    vec![alan_agent_protocol::ContentPart::structured(
                        serde_json::json!({
                            "answers": [{"question_id": "q1", "value": "from agentfs"}]
                        })
                    )]
                );
            }
            other => panic!("expected Op::Resume from namespace response, got {other:?}"),
        }
        assert!(events.is_empty());
    }
}
