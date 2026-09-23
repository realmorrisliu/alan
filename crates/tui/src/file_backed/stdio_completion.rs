use super::{StdioTaskSnapshot, StdioTaskWaitContext, file_surface::TapeRecordV1};
use alan_agent_protocol::UiActivityState;
use anyhow::{Context, Result, anyhow};

pub(super) fn tape_outcome(
    input: &str,
    baseline_tape_history: &[u8],
    tape_history: &[u8],
) -> Result<(bool, Option<String>)> {
    let tape_history = std::str::from_utf8(tape_history).context("machine/tape is not utf8")?;
    let records = tape_history
        .lines()
        .filter_map(|line| serde_json::from_str::<TapeRecordV1>(line).ok())
        .filter(|record| record.kind == "message")
        .collect::<Vec<_>>();
    let matching_task_indices = records
        .iter()
        .enumerate()
        .filter_map(|(index, record)| {
            (record.role == "user" && record.content == input).then_some(index)
        })
        .collect::<Vec<_>>();
    let baseline_record_count = std::str::from_utf8(baseline_tape_history)
        .context("baseline machine/tape is not utf8")?
        .lines()
        .filter_map(|line| serde_json::from_str::<TapeRecordV1>(line).ok())
        .filter(|record| record.kind == "message")
        .count();
    let task_index = tape_history
        .as_bytes()
        .starts_with(baseline_tape_history)
        .then(|| {
            matching_task_indices
                .iter()
                .copied()
                .rfind(|index| *index >= baseline_record_count)
        })
        .flatten();
    let answer = task_index.and_then(|index| {
        let following = &records[index + 1..];
        let turn_end = following
            .iter()
            .position(|record| record.role == "user")
            .unwrap_or(following.len());
        following[..turn_end]
            .iter()
            .rev()
            .find(|record| record.role == "assistant")
            .map(|record| record.content.clone())
    });

    Ok((task_index.is_some(), answer))
}

pub(super) async fn refresh_answer_after_idle(
    shell: &alan_shell::Shell,
    agent_path: &str,
    task: &StdioTaskWaitContext<'_>,
    snapshot: &mut StdioTaskSnapshot,
) -> Result<()> {
    if !snapshot.task_started
        || snapshot.activity_state != Some(UiActivityState::Idle)
        || snapshot.task_error.is_some()
    {
        return Ok(());
    }

    let tape = shell
        .cat(&format!("{agent_path}/machine/tape"))
        .await
        .map_err(|err| anyhow!("read final Agent tape snapshot failed: {err:?}"))?;
    let (_, answer) = tape_outcome(task.input, &task.baseline_tape_history, &tape)?;
    snapshot.assistant_answer = Some(answer.ok_or_else(|| {
        anyhow!("Agent task reached idle without a correlated final answer; outcome is unknown")
    })?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_ap::{InProcessTransport, reference::MemFs};
    use alan_kernel::{Access, MountFs, Namespace};
    use std::sync::Arc;

    #[tokio::test]
    async fn idle_reloads_final_tape_record_instead_of_streamed_preamble() {
        let baseline = b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"prior task\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"prior answer\"}\n";
        let tape = [
            baseline.as_slice(),
            b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"current task\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"preamble\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"final answer\"}\n",
        ]
        .concat();
        let mut namespace = Namespace::new();
        namespace.mount(
            "/agent/1/machine",
            InProcessTransport::new(Arc::new(MemFs::with_read_only_file("tape", tape))),
            Access::ReadOnly,
        );
        let shell =
            alan_shell::Shell::new(InProcessTransport::new(Arc::new(MountFs::new(namespace))));
        let task = StdioTaskWaitContext {
            input: "current task",
            baseline_tape_history: baseline.to_vec(),
            submitted_at_ms: 20,
        };
        let mut snapshot = StdioTaskSnapshot {
            task_started: true,
            assistant_answer: Some("preamble".to_string()),
            activity_state: Some(UiActivityState::Idle),
            task_error: None,
        };

        refresh_answer_after_idle(&shell, "/agent/1", &task, &mut snapshot)
            .await
            .unwrap();

        assert_eq!(
            super::super::finish_stdio_task_if_ready(&mut snapshot)
                .unwrap()
                .as_deref(),
            Some("final answer")
        );
    }
}
