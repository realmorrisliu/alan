use std::collections::HashMap;
use std::future::Future;

use alan_agent_protocol::{ContentPart, UiEvent, UiInputStatus};
use alan_ap::InProcessTransport;
use anyhow::{Context, Result, bail};
use serde::Serialize;

use super::app::humanize_protocol_message;
use super::file_surface::{
    TapeRecordV1, action_ids_from_events, action_title, read_action_snapshot, write_agent_input,
    write_machine_ctl,
};
use super::stdio_completion;
use super::stdio_runner::{
    StdioTaskSnapshot, StdioTaskWaitContext, finish_stdio_task_if_ready,
    probe_root_agent_attachment,
};
use super::tail::{
    StdioTailAttachment, close_stdio_tails, open_stdio_tail_attachment, tail_with_history,
};
use super::usage::read_usage;
use super::{drain_lines, run_stdio_task};

/// Options controlling one-shot CLI stdio task execution.
#[derive(Debug, Clone, Default)]
pub struct StdioTaskOptions {
    /// Additional content parts (such as image attachments) attached to the submission.
    pub parts: Vec<ContentPart>,
    /// Emit structured NDJSON events to stdout instead of buffered plain text.
    pub json: bool,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
enum JsonStreamEvent {
    TextDelta {
        text: String,
    },
    ThinkingDelta {
        text: String,
    },
    ToolStart {
        id: String,
        name: String,
        title: String,
    },
    ToolEnd {
        id: String,
        name: String,
        title: String,
        status: String,
        output: String,
    },
    Usage {
        input_tokens: u64,
        output_tokens: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        cached_input_tokens: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reasoning_tokens: Option<u64>,
        context_window: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        cost_usd: Option<f64>,
    },
    TurnFinished {
        status: String,
        answer: String,
    },
    Error {
        message: String,
    },
}

/// Run one redirected or scripted task with optional attachments and NDJSON streaming.
pub async fn run_stdio_task_with_options(
    root_transport: InProcessTransport,
    agent_path: impl Into<String>,
    input: &str,
    options: StdioTaskOptions,
    interrupt: impl Future<Output = Result<()>>,
) -> Result<i32> {
    if !options.json && options.parts.is_empty() {
        return run_stdio_task(root_transport, agent_path, input, interrupt).await;
    }
    let task = StdioTaskWaitContext::new_with_parts(input, options.parts)?;
    let agent_path = agent_path.into();
    let shell = alan_shell::Shell::new(root_transport);
    let mut attachment = match open_stdio_tail_attachment(&shell, &agent_path).await {
        Ok(attachment) => attachment,
        Err(error) => {
            if options.json {
                let msg = humanize_protocol_message(&format!("{error:#}"));
                let _ = emit_json_event(&JsonStreamEvent::Error {
                    message: msg.clone(),
                })
                .await;
                let _ = emit_json_event(&JsonStreamEvent::TurnFinished {
                    status: "failed".to_string(),
                    answer: String::new(),
                })
                .await;
                bail!("{msg}");
            }
            return Err(super::stdio_runner::humanize_anyhow_error(error));
        }
    };
    if !options.json {
        return super::stdio_runner::run_opened_plain_stdio_task(
            &shell, task, attachment, interrupt,
        )
        .await;
    }

    let output_tail = tail_with_history(
        &shell,
        &format!("{}/io/output", attachment.agent_process_path),
    )
    .await
    .ok()
    .map(|(tail, _)| tail);
    let actions_tail = tail_with_history(
        &shell,
        &format!("{}/actions/events", attachment.agent_process_path),
    )
    .await
    .ok()
    .map(|(tail, _)| tail);

    let result = wait_for_stdio_json_answer(
        &shell,
        task,
        &mut attachment,
        output_tail,
        actions_tail,
        interrupt,
    )
    .await;
    let close_result = close_stdio_tails(attachment.tape_tail, attachment.ui_tail).await;
    let exit_code = result?;
    close_result?;
    Ok(exit_code)
}

async fn emit_json_event(event: &JsonStreamEvent) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut line = serde_json::to_string(event)?;
    line.push('\n');
    let mut stdout = tokio::io::stdout();
    stdout.write_all(line.as_bytes()).await?;
    stdout.flush().await?;
    Ok(())
}

async fn wait_for_stdio_json_answer(
    shell: &alan_shell::Shell,
    task: StdioTaskWaitContext,
    attachment: &mut StdioTailAttachment,
    mut output_tail: Option<alan_shell::Tail>,
    mut actions_tail: Option<alan_shell::Tail>,
    interrupt: impl Future<Output = Result<()>>,
) -> Result<i32> {
    write_agent_input(
        shell,
        "/agent/root",
        Some(attachment.root_agent_pid),
        &task.record,
    )
    .await?;

    tokio::pin!(interrupt);
    let mut tape_pending = Vec::new();
    let mut ui_pending = Vec::new();
    let mut output_pending = Vec::new();
    let mut actions_pending = Vec::new();
    let mut emitted_thinking = String::new();
    let mut action_states: HashMap<String, String> = HashMap::new();
    let mut interrupt_requested = false;
    let mut snapshot = StdioTaskSnapshot {
        task_started: false,
        waiting_for_response: false,
        assistant_answer: None,
        command_output: None,
        activity_state: None,
        task_error: None,
        completion: None,
    };
    let mut root_agent_pid_tick = tokio::time::interval(std::time::Duration::from_millis(250));
    root_agent_pid_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let outcome: Result<(String, String, i32)> = loop {
        tokio::select! {
            bytes = async {
                match output_tail.as_mut() {
                    Some(tail) => tail.read(4096).await,
                    None => std::future::pending().await,
                }
            } => {
                match bytes {
                    Ok(chunk) if !chunk.is_empty() => {
                        output_pending.extend_from_slice(&chunk);
                        emit_utf8_text_delta(&mut output_pending).await?;
                    }
                    _ => {
                        output_tail = None;
                    }
                }
            }
            bytes = async {
                match actions_tail.as_mut() {
                    Some(tail) => tail.read(4096).await,
                    None => std::future::pending().await,
                }
            } => {
                match bytes {
                    Ok(chunk) if !chunk.is_empty() => {
                        actions_pending.extend_from_slice(&chunk);
                        for id in action_ids_from_events(&mut actions_pending) {
                            process_json_action(shell, &attachment.agent_process_path, &id, &mut action_states).await?;
                        }
                    }
                    _ => {
                        actions_tail = None;
                    }
                }
            }
            bytes = attachment.tape_tail.read(4096) => {
                let bytes = match bytes {
                    Ok(bytes) if !bytes.is_empty() => bytes,
                    result => {
                        let error = match result {
                            Ok(_) => anyhow::anyhow!("Agent tape closed before the task completed"),
                            Err(err) => anyhow::anyhow!("read Agent tape failed: {err:?}"),
                        };
                        break Err(error.context("task outcome is unknown"));
                    }
                };
                tape_pending.extend_from_slice(&bytes);
                for line in drain_lines(&mut tape_pending) {
                    let Ok(record) = serde_json::from_slice::<TapeRecordV1>(&line) else {
                        continue;
                    };
                    if record.kind == "message" && record.belongs_to(&task.record.submission_id) {
                        snapshot.task_started = true;
                        if record.role == "assistant" {
                            snapshot.assistant_answer = Some(record.content);
                        }
                    }
                }
                match finish_stdio_task_if_ready(&mut snapshot) {
                    Ok(Some(answer)) => {
                        let (status, text, code) = match answer {
                            super::stdio_runner::StdioTaskOutput::Agent(a) => {
                                ("completed".to_string(), a, 0)
                            }
                            super::stdio_runner::StdioTaskOutput::Command(c) => {
                                let status = if c.exit_code == 0 {
                                    "completed"
                                } else {
                                    "failed"
                                };
                                (status.to_string(), c.stdout, c.exit_code)
                            }
                        };
                        break Ok((status, text, code));
                    }
                    Ok(None) => {}
                    Err(error) => break Err(error),
                }
            }
            bytes = attachment.ui_tail.read(4096) => {
                let bytes = match bytes {
                    Ok(bytes) if !bytes.is_empty() => bytes,
                    result => {
                        let error = match result {
                            Ok(_) => anyhow::anyhow!("Agent UI event stream closed before the task completed"),
                            Err(err) => anyhow::anyhow!("read Agent UI events failed: {err:?}"),
                        };
                        break Err(error.context("task outcome is unknown"));
                    }
                };
                ui_pending.extend_from_slice(&bytes);
                for line in drain_lines(&mut ui_pending) {
                    let event = serde_json::from_slice::<UiEvent>(&line)
                        .map_err(|err| anyhow::anyhow!("parse Agent UI event failed: {err}"))?;
                    match event {
                        UiEvent::Thinking { snapshot: ref thinking } => {
                            if let Some(delta) = thinking_delta(&mut emitted_thinking, &thinking.text) {
                                emit_json_event(&JsonStreamEvent::ThinkingDelta { text: delta }).await?;
                            }
                        }
                        UiEvent::Error { ref message, .. } => {
                            emit_json_event(&JsonStreamEvent::Error {
                                message: humanize_protocol_message(message),
                            })
                            .await?;
                        }
                        event @ (UiEvent::Activity { .. } | UiEvent::InputCompleted { .. }) => {
                            stdio_completion::observe_event(&task.record.submission_id, &mut snapshot, event);
                        }
                        UiEvent::Plan { .. } | UiEvent::Notice { .. } => {}
                    }
                }
                if interrupt_requested && snapshot.completion.is_none() {
                    let _ = write_machine_ctl(
                        shell,
                        &attachment.agent_process_path,
                        &format!("queue-v1 interrupt {}", task.record.submission_id),
                    )
                    .await;
                    break Err(anyhow::anyhow!("Agent input cancellation requested"));
                }
                if snapshot.completion.is_none() && snapshot.waiting_for_response {
                    break Err(anyhow::anyhow!(
                        "Agent task needs interactive input; attach with the TTY renderer (or for workspace access in one-shot mode, save a grant with `alan host grant save <id> <path>` and pass `--grant <id>`)"
                    ));
                }
                stdio_completion::refresh_answer_after_completion(
                    shell,
                    &attachment.agent_process_path,
                    &task,
                    &mut snapshot,
                )
                .await
                .context("final task outcome is unknown")?;
                if snapshot.completion.is_some()
                    && let Some(err) = snapshot.task_error.take()
                {
                    let status = if snapshot.completion == Some(UiInputStatus::Cancelled) {
                        "cancelled"
                    } else {
                        "failed"
                    };
                    let msg = humanize_protocol_message(&err);
                    emit_json_event(&JsonStreamEvent::Error { message: msg.clone() }).await?;
                    break Ok((status.to_string(), msg, 1));
                }
                match finish_stdio_task_if_ready(&mut snapshot) {
                    Ok(Some(answer)) => {
                        let (status, text, code) = match answer {
                            super::stdio_runner::StdioTaskOutput::Agent(a) => {
                                ("completed".to_string(), a, 0)
                            }
                            super::stdio_runner::StdioTaskOutput::Command(c) => {
                                let status = if c.exit_code == 0 {
                                    "completed"
                                } else {
                                    "failed"
                                };
                                (status.to_string(), c.stdout, c.exit_code)
                            }
                        };
                        break Ok((status, text, code));
                    }
                    Ok(None) => {}
                    Err(error) => break Err(error),
                }
            }
            _ = root_agent_pid_tick.tick() => {
                probe_root_agent_attachment(shell, attachment).await?;
            }
            signal = &mut interrupt, if !interrupt_requested => {
                signal?;
                interrupt_requested = true;
                if snapshot.completion.is_none() {
                    let _ = write_machine_ctl(
                        shell,
                        &attachment.agent_process_path,
                        &format!("queue-v1 interrupt {}", task.record.submission_id),
                    )
                    .await;
                    break Err(anyhow::anyhow!("Agent input cancellation requested"));
                }
            }
        }
    };

    // Close helper tails cleanly before final usage/turn_finished emission.
    if let Some(tail) = output_tail {
        let _ = tail.close().await;
    }
    if let Some(tail) = actions_tail {
        let _ = tail.close().await;
    }
    if !output_pending.is_empty() {
        let text = String::from_utf8_lossy(&output_pending).into_owned();
        if !text.is_empty() {
            emit_json_event(&JsonStreamEvent::TextDelta { text }).await?;
        }
    }

    match outcome {
        Ok((status, answer, code)) => {
            if let Some(u) = read_usage(shell, &attachment.agent_process_path).await {
                emit_json_event(&JsonStreamEvent::Usage {
                    input_tokens: u.input_tokens,
                    output_tokens: u.output_tokens,
                    cached_input_tokens: u.cached_input_tokens,
                    reasoning_tokens: u.reasoning_tokens,
                    context_window: u.context_window,
                    cost_usd: u.session_cost_usd.or(u.cost_usd),
                })
                .await?;
            }
            emit_json_event(&JsonStreamEvent::TurnFinished { status, answer }).await?;
            Ok(code)
        }
        Err(error) => {
            let msg = humanize_protocol_message(&format!("{error:#}"));
            emit_json_event(&JsonStreamEvent::Error {
                message: msg.clone(),
            })
            .await?;
            emit_json_event(&JsonStreamEvent::TurnFinished {
                status: "failed".to_string(),
                answer: String::new(),
            })
            .await?;
            bail!("{msg}")
        }
    }
}

async fn emit_utf8_text_delta(pending: &mut Vec<u8>) -> Result<()> {
    let valid_up_to = match std::str::from_utf8(pending) {
        Ok(_) => pending.len(),
        Err(err) => err.valid_up_to(),
    };
    if valid_up_to == 0 {
        return Ok(());
    }
    let valid_bytes: Vec<u8> = pending.drain(..valid_up_to).collect();
    if let Ok(text) = String::from_utf8(valid_bytes)
        && !text.is_empty()
    {
        emit_json_event(&JsonStreamEvent::TextDelta { text }).await?;
    }
    Ok(())
}

async fn process_json_action(
    shell: &alan_shell::Shell,
    agent_path: &str,
    action_id: &str,
    states: &mut HashMap<String, String>,
) -> Result<()> {
    let Ok(snapshot) = read_action_snapshot(shell, agent_path, action_id).await else {
        return Ok(());
    };
    let status = snapshot.status.trim().to_string();
    if states.get(action_id) == Some(&status) {
        return Ok(());
    }
    let title = action_title(&snapshot);
    let was_started = states.contains_key(action_id);
    if matches!(status.as_str(), "running" | "pending") && !was_started {
        states.insert(action_id.to_string(), status);
        emit_json_event(&JsonStreamEvent::ToolStart {
            id: action_id.to_string(),
            name: snapshot.name,
            title,
        })
        .await?;
    } else if matches!(
        status.as_str(),
        "completed" | "failed" | "rejected" | "cancelled"
    ) {
        if !was_started {
            emit_json_event(&JsonStreamEvent::ToolStart {
                id: action_id.to_string(),
                name: snapshot.name.clone(),
                title: title.clone(),
            })
            .await?;
        }
        states.insert(action_id.to_string(), status.clone());
        let output = if !snapshot.output.trim().is_empty() {
            snapshot.output
        } else {
            snapshot.result
        };
        emit_json_event(&JsonStreamEvent::ToolEnd {
            id: action_id.to_string(),
            name: snapshot.name,
            title,
            status,
            output,
        })
        .await?;
    }
    Ok(())
}

fn thinking_delta(emitted: &mut String, current: &str) -> Option<String> {
    if current.is_empty() || current == emitted {
        return None;
    }
    let delta = if let Some(rest) = current.strip_prefix(emitted.as_str()) {
        rest.to_string()
    } else {
        current.to_string()
    };
    *emitted = current.to_string();
    (!delta.is_empty()).then_some(delta)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_all_required_ndjson_event_variants() {
        let events = [
            JsonStreamEvent::TextDelta { text: "hi".into() },
            JsonStreamEvent::ThinkingDelta {
                text: "plan".into(),
            },
            JsonStreamEvent::ToolStart {
                id: "1".into(),
                name: "bash".into(),
                title: "Run tests".into(),
            },
            JsonStreamEvent::ToolEnd {
                id: "1".into(),
                name: "bash".into(),
                title: "Run tests".into(),
                status: "completed".into(),
                output: "ok".into(),
            },
            JsonStreamEvent::Usage {
                input_tokens: 100,
                output_tokens: 25,
                cached_input_tokens: None,
                reasoning_tokens: None,
                context_window: 128_000,
                cost_usd: Some(0.001),
            },
            JsonStreamEvent::TurnFinished {
                status: "completed".into(),
                answer: "done".into(),
            },
            JsonStreamEvent::Error {
                message: "boom".into(),
            },
        ];
        let types: Vec<String> = events
            .iter()
            .map(|e| {
                let v: serde_json::Value = serde_json::to_value(e).unwrap();
                v["type"].as_str().unwrap().to_string()
            })
            .collect();
        assert_eq!(
            types,
            [
                "text_delta",
                "thinking_delta",
                "tool_start",
                "tool_end",
                "usage",
                "turn_finished",
                "error",
            ]
        );
    }
}
