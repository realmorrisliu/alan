use alan_agent_protocol::UiEvent;
use alan_ap::InProcessTransport;
use anyhow::{Context, Result, bail};

use super::drain_lines;
use super::file_surface::{TapeRecordV1, write_agent_input, write_machine_ctl};
use super::stdio_completion;
use super::tail::{self, StdioTailAttachment, close_stdio_tails, open_stdio_tail_attachment};
#[cfg(test)]
use super::unix_time_ms;

/// Run one redirected input, write its output streams, and return its exit status.
pub async fn run_stdio_task(
    root_transport: InProcessTransport,
    agent_path: impl Into<String>,
    input: &str,
    interrupt: impl std::future::Future<Output = Result<()>>,
) -> Result<i32> {
    let task = StdioTaskWaitContext::new(input)?;
    let agent_path = agent_path.into();
    let shell = alan_shell::Shell::new(root_transport);
    let attachment = open_stdio_tail_attachment(&shell, &agent_path)
        .await
        .map_err(humanize_anyhow_error)?;
    run_opened_plain_stdio_task(&shell, task, attachment, interrupt).await
}

pub(super) fn humanize_anyhow_error(error: anyhow::Error) -> anyhow::Error {
    let raw = format!("{error:#}");
    let humanized = super::app::humanize_protocol_message(&raw);
    if humanized != raw.trim() {
        anyhow::anyhow!("{humanized}")
    } else {
        error
    }
}

pub(super) async fn run_opened_plain_stdio_task(
    shell: &alan_shell::Shell,
    task: StdioTaskWaitContext,
    mut attachment: StdioTailAttachment,
    interrupt: impl std::future::Future<Output = Result<()>>,
) -> Result<i32> {
    use tokio::io::AsyncWriteExt;

    let result = wait_for_stdio_answer(shell, task, &mut attachment, interrupt).await;
    let close_result = close_stdio_tails(attachment.tape_tail, attachment.ui_tail).await;
    let answer = result.map_err(humanize_anyhow_error)?;
    close_result?;

    let (out, err, exit_code, add_newline) = match answer {
        StdioTaskOutput::Agent(answer) => (answer, String::new(), 0, true),
        StdioTaskOutput::Command(output) => (output.stdout, output.stderr, output.exit_code, false),
    };
    let mut stdout = tokio::io::stdout();
    stdout.write_all(out.as_bytes()).await?;
    if add_newline && !out.ends_with('\n') {
        stdout.write_all(b"\n").await?;
    }
    stdout.flush().await?;
    let mut stderr = tokio::io::stderr();
    stderr.write_all(err.as_bytes()).await?;
    stderr.flush().await?;
    Ok(exit_code)
}

pub(super) async fn wait_for_stdio_answer(
    shell: &alan_shell::Shell,
    task: StdioTaskWaitContext,
    attachment: &mut StdioTailAttachment,
    interrupt: impl std::future::Future<Output = Result<()>>,
) -> Result<StdioTaskOutput> {
    submit_stdio_task(shell, &task, attachment).await?;
    wait_for_stdio_answer_after_submit(shell, task, attachment, interrupt).await
}

pub(super) async fn submit_stdio_task(
    shell: &alan_shell::Shell,
    task: &StdioTaskWaitContext,
    attachment: &StdioTailAttachment,
) -> Result<()> {
    write_agent_input(
        shell,
        "/agent/root",
        Some(attachment.root_agent_pid),
        &task.record,
    )
    .await
}

pub(super) async fn wait_for_stdio_answer_after_submit(
    shell: &alan_shell::Shell,
    task: StdioTaskWaitContext,
    attachment: &mut StdioTailAttachment,
    interrupt: impl std::future::Future<Output = Result<()>>,
) -> Result<StdioTaskOutput> {
    tokio::pin!(interrupt);
    let mut tape_pending = Vec::new();
    let mut ui_pending = Vec::new();
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

    // Every established future in this loop is polled by its own arm; nothing is left
    // queued on a shared lock while another arm's body awaits (see
    // `mountfs::lock_discipline_tests`). `interrupt` is the one future pinned across
    // iterations, so it must stay a pure signal wait: a caller that made it issue
    // namespace requests could strand them here while a branch body runs.
    loop {
        tokio::select! {
            bytes = attachment.tape_tail.read(4096) => {
                let bytes = match bytes {
                    Ok(bytes) if !bytes.is_empty() => bytes,
                    result => {
                        let error = match result {
                            Ok(_) => anyhow::anyhow!("Agent tape closed before the task completed"),
                            Err(err) => anyhow::anyhow!("read Agent tape failed: {err:?}"),
                        };
                        return Err(error.context("task outcome is unknown"));
                    }
                };
                tape_pending.extend_from_slice(&bytes);
                for line in drain_lines(&mut tape_pending) {
                    let Ok(record) = serde_json::from_slice::<TapeRecordV1>(&line) else {
                        continue;
                    };
                    if record.kind != "message" {
                        continue;
                    }
                    if record.belongs_to(&task.record.submission_id) {
                        snapshot.task_started = true;
                        if record.role == "assistant" {
                            snapshot.assistant_answer = Some(record.content);
                        }
                    }
                }
                if let Some(answer) = finish_stdio_task_if_ready(&mut snapshot)? {
                    return Ok(answer);
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
                        return Err(error.context("task outcome is unknown"));
                    }
                };
                ui_pending.extend_from_slice(&bytes);
                for line in drain_lines(&mut ui_pending) {
                    let event = serde_json::from_slice::<UiEvent>(&line)
                        .map_err(|err| anyhow::anyhow!("parse Agent UI event failed: {err}"))?;
                    match event {
                        event @ UiEvent::Activity { .. } => {
                            stdio_completion::observe_event(&task.record.submission_id, &mut snapshot, event);
                        }
                        event @ UiEvent::InputCompleted { .. } => {
                            stdio_completion::observe_event(&task.record.submission_id, &mut snapshot, event);
                        }
                        UiEvent::Error { .. } | UiEvent::Plan { .. } | UiEvent::Thinking { .. } | UiEvent::Notice { .. } => {}
                    }
                }
                if interrupt_requested
                    && interrupt_stdio_task_if_pending(shell, &attachment.agent_process_path, &task.record.submission_id, &snapshot).await?
                {
                    bail!("Agent input cancellation requested");
                }
                if snapshot.completion.is_none() && snapshot.waiting_for_response {
                    bail!(
                        "Agent task needs interactive input; attach with the TTY renderer (or for workspace access in one-shot mode, save a grant with `alan host grant save <id> <path>` and pass `--grant <id>`)"
                    );
                }
                let refresh_result = {
                    let refresh = stdio_completion::refresh_answer_after_completion(
                        shell,
                        &attachment.agent_process_path,
                        &task,
                        &mut snapshot,
                    );
                    tokio::pin!(refresh);
                    // Every await in this loop must be its own select arm. A future left
                    // queued on a shared lock while another arm's body awaits keeps the
                    // permit it was handed, so the awaiting arm starves behind it: the
                    // namespace read then blocks for as long as the other arm takes.
                    let mut probe: Option<ProbeFuture<'_>> = None;
                    loop {
                        tokio::select! {
                            result = &mut refresh => break result,
                            _ = root_agent_pid_tick.tick() => {
                                probe = Some(Box::pin(probe_root_agent_attachment(shell, attachment)));
                            }
                            result = async { probe.as_mut().expect("probe future").await }, if probe.is_some() => {
                                probe = None;
                                result?;
                            }
                        }
                    }
                };
                refresh_result.context("final task outcome is unknown")?;
                if let Some(answer) = finish_stdio_task_if_ready(&mut snapshot)? {
                    return Ok(answer);
                }
            }
            _ = root_agent_pid_tick.tick() => {
                probe_root_agent_attachment(shell, attachment).await?;
            }
            signal = &mut interrupt, if !interrupt_requested => {
                signal?;
                interrupt_requested = true;
                if interrupt_stdio_task_if_pending(shell, &attachment.agent_process_path, &task.record.submission_id, &snapshot).await? {
                    bail!("Agent input cancellation requested");
                }
            }
        }
    }
}

/// A root-agent probe in flight, polled as its own select arm.
type ProbeFuture<'a> = std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>>;

/// A root-agent liveness probe must never gate the answer: a stalled namespace read
/// is retried on the next tick instead of wedging the wait.
const ROOT_AGENT_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(250);

pub(super) async fn probe_root_agent_attachment(
    shell: &alan_shell::Shell,
    attachment: &StdioTailAttachment,
) -> Result<()> {
    match tokio::time::timeout(
        ROOT_AGENT_PROBE_TIMEOUT,
        tail::require_stdio_attachment_current(shell, attachment),
    )
    .await
    {
        Ok(result) => result,
        // The probe is advisory; a stalled read is retried on the next tick.
        Err(_) => Ok(()),
    }
}

async fn interrupt_stdio_task_if_pending(
    shell: &alan_shell::Shell,
    agent_path: &str,
    submission_id: &str,
    snapshot: &StdioTaskSnapshot,
) -> Result<bool> {
    if snapshot.completion.is_some() {
        return Ok(false);
    }
    write_machine_ctl(
        shell,
        agent_path,
        &format!("queue-v1 interrupt {submission_id}"),
    )
    .await
    .context("failed to send Agent task interrupt")?;
    Ok(true)
}

pub(super) fn finish_stdio_task_if_ready(
    snapshot: &mut StdioTaskSnapshot,
) -> Result<Option<StdioTaskOutput>> {
    if snapshot.completion.is_none() {
        return Ok(None);
    }
    if let Some(output) = snapshot.command_output.take() {
        return Ok(Some(StdioTaskOutput::Command(output)));
    }
    if let Some(message) = snapshot.task_error.take() {
        let humanized = super::app::humanize_protocol_message(&message);
        if humanized != message.trim() {
            bail!("{humanized}");
        }
        bail!("Agent task failed: {message}");
    }
    if let Some(answer) = snapshot.assistant_answer.take() {
        return Ok(Some(StdioTaskOutput::Agent(answer)));
    }
    Ok(None)
}

#[derive(Debug)]
pub(super) enum StdioTaskOutput {
    Agent(String),
    Command(CommandOutput),
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct CommandOutput {
    pub(super) stdout: String,
    pub(super) stderr: String,
    #[serde(default)]
    pub(super) exit_code: i32,
}

#[cfg(test)]
impl StdioTaskOutput {
    pub(super) fn agent_answer(self) -> String {
        match self {
            Self::Agent(answer) => answer,
            Self::Command(_) => panic!("expected Agent answer"),
        }
    }
}

pub(super) struct StdioTaskSnapshot {
    pub(super) task_started: bool,
    pub(super) waiting_for_response: bool,
    pub(super) assistant_answer: Option<String>,
    pub(super) command_output: Option<CommandOutput>,
    pub(super) activity_state: Option<alan_agent_protocol::UiActivityState>,
    pub(super) task_error: Option<String>,
    pub(super) completion: Option<alan_agent_protocol::UiInputStatus>,
}

pub(super) struct StdioTaskWaitContext {
    pub(super) record: alan_agent_protocol::UserInputRecord,
    #[cfg(test)]
    pub(super) submitted_at_ms: u64,
}

impl StdioTaskWaitContext {
    pub(super) fn new(input: &str) -> Result<Self> {
        Self::new_with_parts(input, Vec::new())
    }

    pub(super) fn new_with_parts(
        input: &str,
        parts: Vec<alan_agent_protocol::ContentPart>,
    ) -> Result<Self> {
        let (intent, body) = alan_agent_protocol::parse_input_prefix(input);
        anyhow::ensure!(
            !body.trim().is_empty() || !parts.is_empty(),
            "stdin input body is empty"
        );
        Ok(Self {
            record: alan_agent_protocol::UserInputRecord::new(
                intent,
                alan_agent_protocol::InputMode::FollowUp,
                body,
            )
            .with_parts(parts),
            #[cfg(test)]
            submitted_at_ms: unix_time_ms(),
        })
    }
}

#[cfg(test)]
pub(super) fn stdio_task_snapshot_from_history(
    task: &StdioTaskWaitContext,
    tape_history: &[u8],
    ui_history: &[u8],
) -> Result<StdioTaskSnapshot> {
    let (tape_task_started, assistant_answer) =
        stdio_completion::tape_outcome(&task.record.submission_id, tape_history)?;
    let ui_task = super::file_surface::correlated_ui_task(ui_history, task.submitted_at_ms)?;
    let mut snapshot = StdioTaskSnapshot {
        task_started: tape_task_started,
        waiting_for_response: false,
        assistant_answer,
        command_output: None,
        activity_state: ui_task.state,
        task_error: None,
        completion: None,
    };
    for line in std::str::from_utf8(ui_history)
        .context("UI history is not utf8")?
        .lines()
    {
        let event = serde_json::from_str::<UiEvent>(line).context("parse Agent UI history")?;
        stdio_completion::observe_event(&task.record.submission_id, &mut snapshot, event);
    }
    Ok(snapshot)
}
