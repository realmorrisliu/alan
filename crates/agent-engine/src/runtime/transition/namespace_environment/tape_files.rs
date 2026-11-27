//! Tape and assistant-output files under AgentFS `machine/tape` and `io/output`.

use alan_ap::OpenMode;
use anyhow::{Context, Result};
use serde::Serialize;

use super::client::{NamespaceClient, NamespaceFidGuard};

/// Canonical v1 record appended to `machine/tape`.
///
/// This is deliberately small and self-contained so ADR-0027 D1 can later hash
/// each record without depending on file offsets or mutable tape state.
#[derive(Serialize)]
struct TapeRecordV1<'a> {
    version: u16,
    kind: &'static str,
    role: &'a str,
    content: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_intent: Option<alan_agent_protocol::InputIntent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    submission_id: Option<&'a str>,
    #[serde(skip_serializing_if = "<[String]>::is_empty")]
    related_submission_ids: &'a [String],
}

/// A held GENERATING lease for `machine/tape`.
pub struct NamespaceTapeWriter {
    client: NamespaceClient,
    guard: NamespaceFidGuard,
}

impl NamespaceTapeWriter {
    pub(super) async fn open(client: NamespaceClient, tape_path: &str) -> Result<Self> {
        let fid = client.walk_to(tape_path).await?;
        let guard = client
            .open_guarded_fid(fid, OpenMode::WriteExclusive)
            .await
            .with_context(|| format!("open tape writer for {tape_path}"))?;
        Ok(Self { client, guard })
    }

    pub async fn append_record(
        &self,
        role: &str,
        content: &str,
        submission_id: Option<&str>,
        related_submission_ids: &[String],
    ) -> Result<()> {
        let bytes = tape_record_bytes(role, content, submission_id, related_submission_ids, None)?;
        self.client
            .write_at(self.guard.fid(), 0, &bytes)
            .await
            .context("append tape record")?;
        Ok(())
    }

    /// Append an admitted input with its explicit route, preserving the literal body.
    pub async fn append_input_record(
        &self,
        content: &str,
        submission_id: &str,
        intent: alan_agent_protocol::InputIntent,
    ) -> Result<()> {
        let bytes = tape_record_bytes("user", content, Some(submission_id), &[], Some(intent))?;
        self.client
            .write_at(self.guard.fid(), 0, &bytes)
            .await
            .context("append input tape record")?;
        Ok(())
    }

    pub async fn finish(self) -> Result<()> {
        self.guard.close().await
    }
}

pub(super) async fn write_agent_output(
    client: &NamespaceClient,
    agent_path: &str,
    response: &str,
) -> Result<()> {
    let output_path = format!("{agent_path}/io/output");
    client
        .write_exclusive_document(&output_path, response.as_bytes())
        .await
        .with_context(|| format!("write assistant output to {output_path}"))
}

#[cfg(test)]
pub(super) async fn write_tape_records<'a>(
    client: &NamespaceClient,
    agent_path: &str,
    records: impl IntoIterator<Item = (&'a str, &'a str)>,
    submission_id: Option<&str>,
) -> Result<()> {
    let tape_path = format!("{agent_path}/machine/tape");
    let writer = NamespaceTapeWriter::open(client.clone(), &tape_path).await?;
    for (role, content) in records {
        writer
            .append_record(role, content, submission_id, &[])
            .await?;
    }
    writer.finish().await
}

pub(super) async fn read_current_tape_checkpoint(
    client: &NamespaceClient,
    agent_path: &str,
) -> Result<String> {
    let checkpoint_path = format!("{agent_path}/machine/checkpoints/current");
    let bytes = client
        .read_file(&checkpoint_path)
        .await
        .with_context(|| format!("read current tape checkpoint from {checkpoint_path}"))?;
    let checkpoint = String::from_utf8(bytes).context("current tape checkpoint is not utf8")?;
    Ok(checkpoint.trim().to_string())
}

pub(super) fn tape_record_bytes(
    role: &str,
    content: &str,
    submission_id: Option<&str>,
    related_submission_ids: &[String],
    input_intent: Option<alan_agent_protocol::InputIntent>,
) -> Result<Vec<u8>> {
    let record = TapeRecordV1 {
        version: 1,
        kind: "message",
        role,
        content,
        input_intent,
        submission_id,
        related_submission_ids,
    };
    let mut bytes = serde_json::to_vec(&record).context("serialize tape record")?;
    bytes.push(b'\n');
    Ok(bytes)
}
