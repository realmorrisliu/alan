//! Match shared activity and retained history to one input identity.
use super::file_surface::TapeRecordV1;
use alan_agent_protocol::{UiActivitySnapshot, UiActivityState, UiEvent};
use anyhow::{Context, Result};

#[derive(Default)]
pub(super) struct CorrelatedUiTask {
    pub(super) started: bool,
    pub(super) identity_based: bool,
    pub(super) state: Option<UiActivityState>,
    pub(super) error: Option<String>,
}

pub(super) fn observe_input_activity(
    submission_id: &str,
    submitted_at_ms: u64,
    started: &mut bool,
    state: &mut Option<UiActivityState>,
    activity: &UiActivitySnapshot,
) {
    if activity.version >= 2 {
        let id = submission_id;
        if activity
            .pending_submissions
            .iter()
            .any(|input| input.submission_id == id)
        {
            *started = true;
            *state = None;
        } else if activity
            .active_submission
            .as_ref()
            .is_some_and(|input| input.submission_id == id)
        {
            *started = true;
            // An idle active ID is admission, unless this client already observed execution.
            if activity.state != UiActivityState::Idle
                || matches!(
                    *state,
                    Some(UiActivityState::Running | UiActivityState::Paused)
                )
            {
                *state = Some(activity.state);
            }
        } else if activity.active_submission.is_none()
            && activity.state == UiActivityState::Idle
            && *started
        {
            *state = Some(UiActivityState::Idle);
        }
        return;
    }
    // Retained v1 history has no identity; new submissions require the v2 protocol.
    if activity.state == UiActivityState::Running
        && activity
            .started_at_ms
            .is_none_or(|time| time >= submitted_at_ms)
    {
        *started = true;
        *state = Some(UiActivityState::Running);
    } else if matches!(
        *state,
        Some(UiActivityState::Running | UiActivityState::Paused)
    ) {
        *state = Some(activity.state);
    }
}

pub(super) fn correlated_ui_task(
    ui_history: &[u8],
    submitted_at_ms: u64,
    submission_id: &str,
) -> Result<CorrelatedUiTask> {
    let ui_history = std::str::from_utf8(ui_history).context("ui events are not utf8")?;
    let events = ui_history
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<UiEvent>(line).context("parse Agent UI event"))
        .collect::<Result<Vec<_>>>()?;
    let mut task = CorrelatedUiTask {
        identity_based: events
            .iter()
            .any(|event| matches!(event, UiEvent::Activity { snapshot } if snapshot.version >= 2)),
        ..CorrelatedUiTask::default()
    };
    let mut legacy = false;
    for event in events {
        match event {
            UiEvent::Activity { snapshot } if task.error.is_none() => {
                legacy = snapshot.version < 2;
                if legacy && task.identity_based {
                    continue;
                }
                observe_input_activity(
                    submission_id,
                    submitted_at_ms,
                    &mut task.started,
                    &mut task.state,
                    &snapshot,
                );
            }
            UiEvent::Error {
                message,
                submission_id: id,
                ..
            } if id.as_deref() == Some(submission_id)
                || (id.is_none() && !task.identity_based && legacy && task.started) =>
            {
                task.started = true;
                task.state = Some(UiActivityState::Idle);
                task.error = Some(message);
            }
            _ => {}
        }
        if task.started && task.state == Some(UiActivityState::Idle) {
            break;
        }
    }
    Ok(task)
}

pub(super) fn prompt_position(tape: &[u8], submission_id: &str) -> Option<(String, usize)> {
    let records = std::str::from_utf8(tape)
        .ok()?
        .lines()
        .filter_map(|line| serde_json::from_str::<TapeRecordV1>(line).ok())
        .filter(|record| record.kind == "message" && record.role == "user")
        .collect::<Vec<_>>();
    let index = records
        .iter()
        .position(|record| record.submission_id.as_deref() == Some(submission_id))?;
    let body = &records[index].content;
    let prior = records[..index]
        .iter()
        .filter(|record| record.content == *body)
        .count();
    Some((body.clone(), prior))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_agent_protocol::{InputIntent, UiSubmission};

    #[test]
    fn reconnect_uses_identity_even_when_clients_submit_identical_prompts() {
        let tape = br#"{"version":1,"kind":"message","role":"user","content":"repeat","submission_id":"other"}
{"version":1,"kind":"message","role":"assistant","content":"foreign answer","submission_id":"other"}
{"version":1,"kind":"message","role":"user","content":"repeat","submission_id":"mine"}
"#;
        assert_eq!(prompt_position(tape, "mine"), Some(("repeat".into(), 1)));
        assert_eq!(prompt_position(tape, "missing"), None);
        let mut foreign = UiActivitySnapshot::running(30);
        foreign.active_submission = Some(UiSubmission {
            submission_id: "other".into(),
            intent: InputIntent::Agent,
        });
        let event = |snapshot| serde_json::to_string(&UiEvent::Activity { snapshot }).unwrap();
        let mut old = UiActivitySnapshot::running(30);
        old.version = 1;
        let mut old_idle = UiActivitySnapshot::idle();
        old_idle.version = 1;
        let mut history =
            event(old) + "\n" + &event(old_idle) + "\n" + &event(foreign.clone()) + "\n";
        let unseen = correlated_ui_task(history.as_bytes(), 20, "mine").unwrap();
        assert!(!unseen.started);
        foreign.pending_submissions.push(UiSubmission {
            submission_id: "mine".into(),
            intent: InputIntent::Agent,
        });
        history += &(event(foreign) + "\n");
        let queued = correlated_ui_task(history.as_bytes(), 20, "mine").unwrap();
        assert!(queued.started && queued.identity_based);
        assert!(queued.state.is_none());
        let mut running = UiActivitySnapshot::running(40);
        running.active_submission = Some(UiSubmission {
            submission_id: "mine".into(),
            intent: InputIntent::Agent,
        });
        history += &(event(running) + "\n" + &event(UiActivitySnapshot::idle()) + "\n");
        history += &event(UiActivitySnapshot::running(50));
        let settled = correlated_ui_task(history.as_bytes(), 20, "mine").unwrap();
        assert_eq!(settled.state, Some(UiActivityState::Idle));
    }
}
