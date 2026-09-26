//! Collect the submitted child turn, excluding idle admission snapshots.
use super::*;

pub(super) async fn wait_for_child_terminal(
    root: &InProcessTransport,
    pid: Pid,
    controller: &RuntimeController,
    submission_id: &str,
) -> Result<AgentExecutableResult> {
    let shell = alan_shell::Shell::new(root.clone());
    let activity_path = format!("/agent/{}/machine/ui/activity", pid.0);
    let events_path = format!("/agent/{}/machine/ui/events", pid.0);
    let notice_path = format!("/agent/{}/machine/ui/notice", pid.0);
    let mut observed = false;
    loop {
        if controller.is_finished() {
            anyhow::bail!("child Agent Machine stopped before publishing a terminal result");
        }
        if !observed {
            let events = shell.cat(&events_path).await?;
            for line in std::str::from_utf8(&events)?
                .lines()
                .filter(|line| !line.is_empty())
            {
                if let alan_agent_engine::UiEvent::Activity { snapshot } =
                    serde_json::from_str(line)?
                {
                    observed |= snapshot
                        .active_submission
                        .as_ref()
                        .is_some_and(|input| input.submission_id == submission_id)
                        || snapshot
                            .pending_submissions
                            .iter()
                            .any(|input| input.submission_id == submission_id);
                }
            }
        }
        if observed {
            let activity: UiActivitySnapshot =
                serde_json::from_slice(&shell.cat(&activity_path).await?)?;
            if child_activity_terminal(&activity, submission_id) {
                let notice: UiNoticeSnapshot =
                    serde_json::from_slice(&shell.cat(&notice_path).await?)?;
                let output_text =
                    String::from_utf8(shell.cat(&format!("/agent/{}/io/output", pid.0)).await?)
                        .context("child Agent output is utf8")?;
                let warnings = (notice.kind == UiNoticeKind::Warning)
                    .then(|| notice.message.clone())
                    .into_iter()
                    .collect();
                if notice.kind == UiNoticeKind::Error {
                    return Ok(AgentExecutableResult::failed_with_output(
                        output_text,
                        warnings,
                        notice.message,
                    ));
                }
                if activity.state == UiActivityState::Paused {
                    let Some(pause) = read_child_pause(&shell, pid).await? else {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        continue;
                    };
                    return Ok(AgentExecutableResult::paused(output_text, warnings, pause));
                }
                return Ok(AgentExecutableResult::completed(output_text, warnings));
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn read_child_pause(
    shell: &alan_shell::Shell,
    pid: Pid,
) -> Result<Option<AgentExecutablePause>> {
    let requests_path = format!("/agent/{}/requests", pid.0);
    for request_id in shell.ls(&requests_path).await? {
        if matches!(request_id.as_str(), "clone" | "events") {
            continue;
        }
        let request_path = format!("{requests_path}/{request_id}");
        if shell.cat(&format!("{request_path}/status")).await? != b"pending" {
            continue;
        }
        let kind = String::from_utf8(shell.cat(&format!("{request_path}/kind")).await?)
            .context("child Agent request kind is utf8")?;
        let kind = match kind.as_str() {
            "confirmation" => YieldKind::Confirmation,
            "structured_input" => YieldKind::StructuredInput,
            other => YieldKind::Custom(other.to_string()),
        };
        return Ok(Some(AgentExecutablePause { request_id, kind }));
    }
    Ok(None)
}

fn child_activity_terminal(activity: &UiActivitySnapshot, submission_id: &str) -> bool {
    match activity.state {
        UiActivityState::Idle => {
            activity.active_submission.is_none() && activity.pending_submissions.is_empty()
        }
        UiActivityState::Paused => activity
            .active_submission
            .as_ref()
            .is_some_and(|input| input.submission_id == submission_id),
        UiActivityState::Running => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_agent_protocol::{InputIntent, UiSubmission};

    #[test]
    fn admitted_idle_input_is_not_a_child_result() {
        let mut activity = UiActivitySnapshot::idle();
        let input = UiSubmission {
            submission_id: "mine".into(),
            intent: InputIntent::Agent,
        };
        activity.pending_submissions.push(input.clone());
        assert!(!child_activity_terminal(&activity, "mine"));
        activity.pending_submissions.clear();
        activity.active_submission = Some(input);
        assert!(!child_activity_terminal(&activity, "mine"));
        activity.state = UiActivityState::Running;
        assert!(!child_activity_terminal(&activity, "mine"));
        activity.state = UiActivityState::Paused;
        assert!(child_activity_terminal(&activity, "mine"));
        assert!(!child_activity_terminal(&activity, "other"));
        activity.active_submission = None;
        activity.state = UiActivityState::Idle;
        assert!(child_activity_terminal(&activity, "mine"));
    }
}
