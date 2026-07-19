//! AgentFS file protocol used by the namespace runtime environment.

use std::sync::atomic::Ordering;

use alan_agent_protocol::{
    ContentPart, InputMode, Op, Submission, UiActivitySnapshot, UiEvent, UiNoticeSnapshot,
    UiPlanSnapshot, UiThinkingSnapshot,
};
use alan_ap::{Fid, OpenMode};
use anyhow::{Context, Result, bail};
use serde::Serialize;

use super::{
    NamespaceActionRecord, NamespaceAgentFiles, NamespaceRequestRecord,
    client::{InputFrame, NamespaceClient},
};
use crate::evidence::{
    EvidenceResolutionError, EvidenceResolutionErrorCode, NamespaceEvidenceReference,
    is_retention_expired_record,
};

impl NamespaceAgentFiles {
    fn client(&self) -> NamespaceClient {
        NamespaceClient::new(self.root.clone())
    }

    pub async fn read_next_input(&self) -> Result<String> {
        let input_path = format!("{}/io/input", self.agent_path);
        let client = self.client();
        let offset = self.input_offset.load(Ordering::Relaxed);
        let raw = client
            .read_stream_from(&input_path, offset)
            .await
            .with_context(|| format!("read input from {input_path}"))?;
        let frame = InputFrame::parse_one(&raw).context("parse agent io/input frame")?;
        self.input_offset
            .fetch_add(frame.bytes_consumed as u64, Ordering::Relaxed);
        Ok(frame.message)
    }

    pub async fn read_next_input_submission(&self, mode: InputMode) -> Result<Submission> {
        let message = self.read_next_input().await?;
        Ok(Submission::new(Op::Input {
            parts: vec![ContentPart::text(message)],
            mode,
        }))
    }

    pub async fn read_next_machine_control_submission(&self) -> Result<Option<Submission>> {
        let events_path = format!("{}/events", self.agent_path);
        let client = self.client();
        let offset = self.control_offset.load(Ordering::Relaxed);
        let stat = client
            .stat_path(&events_path)
            .await
            .with_context(|| format!("stat agent events from {events_path}"))?;
        if stat.length <= offset {
            return Ok(None);
        }

        let raw = client
            .read_file_range(&events_path, offset, stat.length - offset)
            .await
            .with_context(|| format!("read agent events from {events_path}"))?;
        let mut consumed = 0_u64;

        for line in raw.split_inclusive(|byte| *byte == b'\n') {
            if !line.ends_with(b"\n") {
                break;
            }
            consumed += line.len() as u64;
            let record = String::from_utf8(line[..line.len() - 1].to_vec())
                .context("agent events record is not utf8")?;
            if let Some(command) = record.strip_prefix("ctl:") {
                self.control_offset
                    .store(offset + consumed, Ordering::Relaxed);
                if let Some(submission) = machine_control_submission(command) {
                    return Ok(Some(submission));
                }
            }
        }

        self.control_offset
            .store(offset + consumed, Ordering::Relaxed);
        Ok(None)
    }

    pub async fn resume_submission_from_answered_request(
        &self,
        request_id: &str,
    ) -> Result<Option<Submission>> {
        let Some(response) = self.read_answered_request_response(request_id).await? else {
            return Ok(None);
        };
        Ok(Some(Submission::new(Op::Resume {
            request_id: request_id.to_string(),
            content: vec![request_response_content_part(response)],
        })))
    }

    pub async fn read_answered_request_response(&self, request_id: &str) -> Result<Option<String>> {
        validate_agent_file_id(request_id, "request id")?;
        let client = self.client();
        let request_path = format!("{}/requests/{request_id}", self.agent_path);
        let status_path = format!("{request_path}/status");
        let Some(status) = client
            .try_read_file(&status_path)
            .await
            .with_context(|| format!("read request status from {status_path}"))?
        else {
            return Ok(None);
        };
        let status = String::from_utf8(status).context("request status is not utf8")?;
        if status.trim() != "answered" {
            return Ok(None);
        }
        let response_path = format!("{request_path}/response");
        let Some(response) = client
            .try_read_file(&response_path)
            .await
            .with_context(|| format!("read request response from {response_path}"))?
        else {
            return Ok(None);
        };
        let response = String::from_utf8(response).context("request response is not utf8")?;
        Ok(Some(response))
    }

    pub async fn write_assistant_output(&self, response: &str) -> Result<()> {
        let client = NamespaceClient::new(self.root.clone());
        write_agent_output(&client, &self.agent_path, response).await
    }

    #[cfg(test)]
    pub async fn write_user_state(&self, input: &str) -> Result<()> {
        let client = NamespaceClient::new(self.root.clone());
        write_tape_records(&client, &self.agent_path, [("user", input)]).await
    }

    pub async fn write_turn_tape_state(&self, input: Option<&str>, response: &str) -> Result<()> {
        let client = NamespaceClient::new(self.root.clone());
        let mut records = Vec::new();
        if let Some(input) = input.filter(|value| !value.trim().is_empty()) {
            records.push(("user", input));
        }
        records.push(("assistant", response));
        write_tape_records(&client, &self.agent_path, records).await
    }

    #[cfg(test)]
    pub async fn begin_tape_generation(&self) -> Result<NamespaceTapeWriter> {
        let client = NamespaceClient::new(self.root.clone());
        NamespaceTapeWriter::open(client, &self.agent_path).await
    }

    pub async fn current_tape_checkpoint(&self) -> Result<String> {
        let client = NamespaceClient::new(self.root.clone());
        read_current_tape_checkpoint(&client, &self.agent_path).await
    }

    pub async fn write_request(&self, record: NamespaceRequestRecord) -> Result<String> {
        let client = NamespaceClient::new(self.root.clone());
        write_request_record(&client, &self.agent_path, record).await
    }

    pub(crate) async fn write_confirmation_request(
        &self,
        pending: &crate::approval::PendingConfirmation,
    ) -> Result<String> {
        let kind = crate::approval::runtime_confirmation_control_kind(&pending.checkpoint_type)
            .unwrap_or("confirmation");
        let options = serde_json::to_string(&serde_json::json!({
            "checkpoint_id": pending.checkpoint_id.clone(),
            "checkpoint_type": pending.checkpoint_type.clone(),
            "details": pending.details.clone(),
            "options": pending.options.clone(),
        }))?;
        self.write_request(
            NamespaceRequestRecord::new(kind, pending.summary.clone()).with_options(options),
        )
        .await
    }

    pub(crate) async fn write_structured_input_request(
        &self,
        pending: &crate::approval::PendingStructuredInputRequest,
    ) -> Result<String> {
        let options = serde_json::to_string(&serde_json::json!({
            "request_id": pending.request_id.clone(),
            "title": pending.title.clone(),
            "questions": pending.questions.clone(),
        }))?;
        self.write_request(
            NamespaceRequestRecord::new("structured_input", pending.prompt.clone())
                .with_options(options),
        )
        .await
    }

    pub async fn write_action(&self, record: NamespaceActionRecord) -> Result<String> {
        let client = NamespaceClient::new(self.root.clone());
        write_action_record(&client, &self.agent_path, record).await
    }

    pub(crate) async fn read_ui_activity_snapshot(&self) -> Result<UiActivitySnapshot> {
        serde_json::from_slice(
            &self
                .client()
                .read_file(&ui_activity_path(&self.agent_path))
                .await?,
        )
        .context("parse agent activity snapshot")
    }

    pub(crate) async fn read_assistant_output(&self) -> Result<String> {
        let path = format!("{}/io/output", self.agent_path);
        String::from_utf8(self.client().read_file(&path).await?)
            .context("agent assistant output is utf8")
    }

    pub(crate) async fn read_ui_notice_snapshot(&self) -> Result<UiNoticeSnapshot> {
        serde_json::from_slice(
            &self
                .client()
                .read_file(&ui_notice_path(&self.agent_path))
                .await?,
        )
        .context("parse agent notice snapshot")
    }

    pub(crate) async fn ui_events_offset(&self) -> Result<u64> {
        Ok(self
            .client()
            .stat_path(&ui_events_path(&self.agent_path))
            .await?
            .length)
    }

    pub(crate) async fn request_ids(&self) -> Result<Vec<String>> {
        self.child_tree_ids("requests").await
    }

    pub(crate) async fn pending_request_id(&self, ids: &[String]) -> Result<Option<String>> {
        for id in ids {
            let path = format!("{}/requests/{id}/status", self.agent_path);
            let status = String::from_utf8(self.client().read_file(&path).await?)
                .context("request status is utf8")?;
            if status.trim() == "pending" {
                return Ok(Some(id.clone()));
            }
        }
        Ok(None)
    }

    pub(crate) async fn action_ids(&self) -> Result<Vec<String>> {
        self.child_tree_ids("actions").await
    }

    pub(crate) async fn read_request_kind(&self, id: &str) -> Result<String> {
        let path = format!("{}/requests/{id}/kind", self.agent_path);
        String::from_utf8(self.client().read_file(&path).await?).context("request kind is utf8")
    }

    pub(crate) async fn request_events_offset(&self) -> Result<u64> {
        self.child_tree_events_offset("requests").await
    }

    pub(crate) async fn action_events_offset(&self) -> Result<u64> {
        self.child_tree_events_offset("actions").await
    }

    async fn child_tree_events_offset(&self, tree: &str) -> Result<u64> {
        Ok(self
            .client()
            .stat_path(&format!("{}/{tree}/events", self.agent_path))
            .await?
            .length)
    }

    async fn child_tree_ids(&self, tree: &str) -> Result<Vec<String>> {
        let mut ids = self
            .client()
            .try_read_directory_names(&format!("{}/{tree}", self.agent_path))
            .await?
            .unwrap_or_default();
        ids.retain(|name| !matches!(name.as_str(), "clone" | "events" | "help"));
        ids.sort();
        Ok(ids)
    }

    /// Build a bounded reference only when the path currently resolves in this
    /// Agent Process namespace.
    pub(crate) async fn evidence_reference(
        &self,
        path: impl Into<String>,
    ) -> Option<NamespaceEvidenceReference> {
        let path = path.into();
        let client = NamespaceClient::new(self.root.clone());
        let stat = client.stat_path(&path).await.ok()?;
        Some(NamespaceEvidenceReference {
            path,
            offset: Some(0),
            length: Some(stat.length),
        })
    }

    /// Resolve evidence through the same namespace walk used for ordinary
    /// files, preserving preview and child metadata in structured failures.
    pub(crate) async fn resolve_evidence_reference(
        &self,
        reference: &NamespaceEvidenceReference,
        preview: Option<String>,
        child_run: Option<serde_json::Value>,
    ) -> std::result::Result<Vec<u8>, EvidenceResolutionError> {
        let client = NamespaceClient::new(self.root.clone());
        let full_bytes =
            client
                .read_file(&reference.path)
                .await
                .map_err(|_| EvidenceResolutionError {
                    code: EvidenceResolutionErrorCode::Missing,
                    reference: reference.clone(),
                    message: "evidence reference is not reachable in this namespace".to_string(),
                    preview: preview.clone(),
                    child_run: child_run.clone(),
                })?;

        if is_retention_expired_record(&full_bytes) {
            return Err(EvidenceResolutionError {
                code: EvidenceResolutionErrorCode::RetentionExpired,
                reference: reference.clone(),
                message: "evidence expired under the storing file server retention policy"
                    .to_string(),
                preview,
                child_run,
            });
        }
        let range = match (reference.offset, reference.length) {
            (Some(offset), Some(length)) => usize::try_from(offset)
                .ok()
                .zip(usize::try_from(length).ok())
                .and_then(|(start, length)| start.checked_add(length).map(|end| (start, end))),
            (Some(offset), None) => usize::try_from(offset)
                .ok()
                .map(|start| (start, full_bytes.len())),
            (None, Some(length)) => usize::try_from(length).ok().map(|end| (0, end)),
            (None, None) => return Ok(full_bytes),
        };
        range
            .filter(|(start, end)| *start <= *end && *end <= full_bytes.len())
            .map(|(start, end)| full_bytes[start..end].to_vec())
            .ok_or_else(|| EvidenceResolutionError {
                code: EvidenceResolutionErrorCode::Missing,
                reference: reference.clone(),
                message: "evidence reference range is not available".to_string(),
                preview,
                child_run,
            })
    }

    pub(crate) async fn write_ui_activity_snapshot(
        &self,
        snapshot: &UiActivitySnapshot,
    ) -> Result<()> {
        let client = NamespaceClient::new(self.root.clone());
        write_json_document(&client, &ui_activity_path(&self.agent_path), snapshot).await
    }

    pub(crate) async fn write_ui_plan_snapshot(&self, snapshot: &UiPlanSnapshot) -> Result<()> {
        let client = NamespaceClient::new(self.root.clone());
        write_json_document(&client, &ui_plan_path(&self.agent_path), snapshot).await
    }

    pub(crate) async fn write_ui_thinking_snapshot(
        &self,
        snapshot: &UiThinkingSnapshot,
    ) -> Result<()> {
        let client = NamespaceClient::new(self.root.clone());
        write_json_document(&client, &ui_thinking_path(&self.agent_path), snapshot).await
    }

    pub(crate) async fn write_ui_notice_snapshot(&self, snapshot: &UiNoticeSnapshot) -> Result<()> {
        let client = NamespaceClient::new(self.root.clone());
        write_json_document(&client, &ui_notice_path(&self.agent_path), snapshot).await
    }

    pub(crate) async fn append_ui_event(&self, event: &UiEvent) -> Result<()> {
        let client = NamespaceClient::new(self.root.clone());
        append_json_line(&client, &ui_events_path(&self.agent_path), event).await
    }
}

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
}

/// A held GENERATING lease for `machine/tape`.
pub struct NamespaceTapeWriter {
    client: NamespaceClient,
    fid: Fid,
    closed: bool,
}

impl NamespaceTapeWriter {
    async fn open(client: NamespaceClient, agent_path: &str) -> Result<Self> {
        let tape_path = format!("{agent_path}/machine/tape");
        let fid = client.walk_to(&tape_path).await?;
        client
            .open(fid, OpenMode::Write)
            .await
            .with_context(|| format!("open tape writer for {tape_path}"))?;
        Ok(Self {
            client,
            fid,
            closed: false,
        })
    }

    pub async fn append_record(&mut self, role: &str, content: &str) -> Result<()> {
        let bytes = tape_record_bytes(role, content)?;
        self.client
            .write_at(self.fid, 0, &bytes)
            .await
            .context("append tape record")?;
        Ok(())
    }

    pub async fn finish(mut self) -> Result<()> {
        self.closed = true;
        self.client.clunk(self.fid).await
    }
}

impl Drop for NamespaceTapeWriter {
    fn drop(&mut self) {
        if !self.closed {
            tracing::warn!("namespace tape writer dropped without clunking machine/tape lease");
        }
    }
}

pub(super) async fn write_agent_output(
    client: &NamespaceClient,
    agent_path: &str,
    response: &str,
) -> Result<()> {
    let output_path = format!("{agent_path}/io/output");
    client
        .write_document(&output_path, response.as_bytes())
        .await
        .with_context(|| format!("write assistant output to {output_path}"))
}

pub(super) async fn write_tape_records<'a>(
    client: &NamespaceClient,
    agent_path: &str,
    records: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<()> {
    let mut writer = NamespaceTapeWriter::open(client.clone(), agent_path).await?;
    for (role, content) in records {
        writer.append_record(role, content).await?;
    }
    writer.finish().await
}

async fn read_current_tape_checkpoint(
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

pub(super) fn tape_record_bytes(role: &str, content: &str) -> Result<Vec<u8>> {
    let record = TapeRecordV1 {
        version: 1,
        kind: "message",
        role,
        content,
    };
    let mut bytes = serde_json::to_vec(&record).context("serialize tape record")?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn validate_agent_file_id(id: &str, label: &str) -> Result<()> {
    if id.is_empty() || id.contains('/') || id == "." || id == ".." {
        bail!("invalid {label}: {id:?}");
    }
    Ok(())
}

fn request_response_content_part(response: String) -> ContentPart {
    match serde_json::from_str::<serde_json::Value>(&response) {
        Ok(value) => ContentPart::structured(value),
        Err(_) => ContentPart::text(response),
    }
}

fn machine_control_submission(command: &str) -> Option<Submission> {
    match command.trim() {
        "compact" => Some(Submission::new(Op::CompactWithOptions { focus: None })),
        "rollback" => Some(Submission::new(Op::Rollback { turns: 1 })),
        // Turn interrupt is agent-runtime control (stop the current turn,
        // keep the agent alive), not kernel process lifecycle:
        // `/proc/<pid>/ctl` interrupt terminates the process, which is the
        // wrong semantics for a renderer host's Esc. File clients interrupt
        // through machine/ctl.
        "interrupt" => Some(Submission::new(Op::Interrupt)),
        _ => None,
    }
}

async fn write_request_record(
    client: &NamespaceClient,
    agent_path: &str,
    record: NamespaceRequestRecord,
) -> Result<String> {
    let clone_path = format!("{agent_path}/requests/clone");
    let id = client
        .clone_via_open(&clone_path)
        .await
        .with_context(|| format!("create request through {clone_path}"))?;
    let request_path = format!("{agent_path}/requests/{id}");
    client
        .write_document(&format!("{request_path}/kind"), record.kind.as_bytes())
        .await?;
    client
        .write_document(&format!("{request_path}/prompt"), record.prompt.as_bytes())
        .await?;
    if let Some(options) = record.options {
        client
            .write_document(&format!("{request_path}/options"), options.as_bytes())
            .await?;
    }
    Ok(id)
}

async fn write_action_record(
    client: &NamespaceClient,
    agent_path: &str,
    record: NamespaceActionRecord,
) -> Result<String> {
    let clone_path = format!("{agent_path}/actions/clone");
    let id = client
        .clone_via_open(&clone_path)
        .await
        .with_context(|| format!("create action through {clone_path}"))?;
    let action_path = format!("{agent_path}/actions/{id}");
    client
        .write_document(&format!("{action_path}/name"), record.name.as_bytes())
        .await?;
    client
        .write_document(&format!("{action_path}/status"), record.status.as_bytes())
        .await?;
    if let Some(output) = record.output {
        client
            .write_document(&format!("{action_path}/output"), output.as_bytes())
            .await?;
    }
    if let Some(result) = record.result {
        client
            .write_document(&format!("{action_path}/result"), result.as_bytes())
            .await?;
    }
    if let Some(approval) = record.approval {
        client
            .write_document(&format!("{action_path}/approval"), approval.as_bytes())
            .await?;
    }
    if let Some(process) = record.process {
        client
            .write_document(&format!("{action_path}/process"), process.as_bytes())
            .await?;
    }
    Ok(id)
}

fn ui_activity_path(agent_path: &str) -> String {
    format!("{agent_path}/machine/ui/activity")
}

fn ui_plan_path(agent_path: &str) -> String {
    format!("{agent_path}/machine/ui/plan")
}

fn ui_thinking_path(agent_path: &str) -> String {
    format!("{agent_path}/machine/ui/thinking")
}

fn ui_notice_path(agent_path: &str) -> String {
    format!("{agent_path}/machine/ui/notice")
}

fn ui_events_path(agent_path: &str) -> String {
    format!("{agent_path}/machine/ui/events")
}

async fn write_json_document<T: Serialize>(
    client: &NamespaceClient,
    path: &str,
    value: &T,
) -> Result<()> {
    let bytes = serde_json::to_vec(value).context("serialize ui snapshot")?;
    client.write_document(path, &bytes).await
}

async fn append_json_line<T: Serialize>(
    client: &NamespaceClient,
    path: &str,
    value: &T,
) -> Result<()> {
    let mut bytes = serde_json::to_vec(value).context("serialize ui event")?;
    bytes.push(b'\n');
    client.write_document(path, &bytes).await
}
