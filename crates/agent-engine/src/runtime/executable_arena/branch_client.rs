//! `aP` protocol client for `/mnt/branch` (`alan-branchfs`) without crate coupling.

use std::sync::atomic::{AtomicU64, Ordering};

use alan_ap::{Fid, FileKind, InProcessTransport, OpenMode, Request, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{
    ArenaWorkspaceBackingKind, BRANCH_MOUNT_PATH, ExecutableArenaError, validate_no_raw_host_path,
    validate_opaque_workspace_ref,
};

static NEXT_BRANCH_FID: AtomicU64 = AtomicU64::new(50_000);

/// Branch record view read back from `/mnt/branch/branches/<id>` over `aP`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BranchRecordView {
    pub version: u16,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    pub root: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backing_kind: Option<ArenaWorkspaceBackingKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checks_summary: Option<String>,
}

/// Parameters for writing a `fork` command to `/mnt/branch/ctl`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchForkCommand<'a> {
    pub id: &'a str,
    pub from: &'a str,
    pub delta: &'a str,
    pub workspace_ref: Option<&'a str>,
    pub backing_kind: Option<ArenaWorkspaceBackingKind>,
    pub diff_summary: Option<&'a str>,
    pub checks_summary: Option<&'a str>,
}

/// Parameters for writing a `score` command to `/mnt/branch/ctl`.
#[derive(Debug, Clone, PartialEq)]
pub struct BranchScoreCommand<'a> {
    pub id: &'a str,
    pub score: f64,
    pub summary: Option<&'a str>,
    pub diff_summary: Option<&'a str>,
    pub checks_summary: Option<&'a str>,
}

/// Thin `aP` client driving `ctl`, `branches/<id>`, `selected`, and `events`.
#[derive(Clone)]
pub struct BranchFsApClient {
    transport: InProcessTransport,
    mount_prefix: String,
}

impl BranchFsApClient {
    /// Connect to `branchfs` mounted at `/mnt/branch` inside a Process namespace.
    pub fn at_namespace_mount(transport: InProcessTransport) -> Self {
        Self::new(transport, BRANCH_MOUNT_PATH)
    }

    /// Connect directly to a standalone `BranchFs` transport root.
    pub fn direct(transport: InProcessTransport) -> Self {
        Self::new(transport, "")
    }

    /// Connect to `branchfs` at an explicit mount prefix.
    pub fn new(transport: InProcessTransport, mount_prefix: impl Into<String>) -> Self {
        Self {
            transport,
            mount_prefix: mount_prefix.into().trim_end_matches('/').to_string(),
        }
    }

    /// Fork a candidate branch in `/mnt/branch/ctl` with strictly opaque workspace metadata.
    pub async fn fork(&self, cmd: BranchForkCommand<'_>) -> Result<(), ExecutableArenaError> {
        validate_no_raw_host_path(cmd.delta)?;
        if let Some(ws_ref) = cmd.workspace_ref {
            validate_opaque_workspace_ref(ws_ref)?;
        }
        if let Some(diff) = cmd.diff_summary {
            validate_no_raw_host_path(diff)?;
        }
        if let Some(checks) = cmd.checks_summary {
            validate_no_raw_host_path(checks)?;
        }

        let mut payload = json!({
            "op": "fork",
            "id": cmd.id,
            "from": cmd.from,
            "delta": cmd.delta,
        });
        if let Some(ws_ref) = cmd.workspace_ref {
            payload["workspace_ref"] = json!(ws_ref);
        }
        if let Some(kind) = cmd.backing_kind {
            payload["backing_kind"] = json!(kind.as_str());
        }
        if let Some(diff) = cmd.diff_summary {
            payload["diff_summary"] = json!(diff);
        }
        if let Some(checks) = cmd.checks_summary {
            payload["checks_summary"] = json!(checks);
        }
        self.write_ctl(&payload).await
    }

    /// Update opaque workspace metadata on an existing visible branch.
    pub async fn update_workspace(
        &self,
        id: &str,
        workspace_ref: Option<&str>,
        backing_kind: Option<ArenaWorkspaceBackingKind>,
        diff_summary: Option<&str>,
        checks_summary: Option<&str>,
    ) -> Result<(), ExecutableArenaError> {
        if let Some(ws_ref) = workspace_ref {
            validate_opaque_workspace_ref(ws_ref)?;
        }
        if let Some(diff) = diff_summary {
            validate_no_raw_host_path(diff)?;
        }
        if let Some(checks) = checks_summary {
            validate_no_raw_host_path(checks)?;
        }
        let mut payload = json!({
            "op": "update_workspace",
            "id": id,
        });
        if let Some(ws_ref) = workspace_ref {
            payload["workspace_ref"] = json!(ws_ref);
        }
        if let Some(kind) = backing_kind {
            payload["backing_kind"] = json!(kind.as_str());
        }
        if let Some(diff) = diff_summary {
            payload["diff_summary"] = json!(diff);
        }
        if let Some(checks) = checks_summary {
            payload["checks_summary"] = json!(checks);
        }
        self.write_ctl(&payload).await
    }

    /// Record a Verifier score and optional diff/check summaries for a branch.
    pub async fn score(&self, cmd: BranchScoreCommand<'_>) -> Result<(), ExecutableArenaError> {
        if !cmd.score.is_finite() {
            return Err(ExecutableArenaError::BranchFsProtocol(format!(
                "non-finite score for branch `{}`",
                cmd.id
            )));
        }
        if let Some(summary) = cmd.summary {
            validate_no_raw_host_path(summary)?;
        }
        if let Some(diff) = cmd.diff_summary {
            validate_no_raw_host_path(diff)?;
        }
        if let Some(checks) = cmd.checks_summary {
            validate_no_raw_host_path(checks)?;
        }

        let mut payload = json!({
            "op": "score",
            "id": cmd.id,
            "score": cmd.score,
        });
        if let Some(summary) = cmd.summary {
            payload["summary"] = json!(summary);
        }
        if let Some(diff) = cmd.diff_summary {
            payload["diff_summary"] = json!(diff);
        }
        if let Some(checks) = cmd.checks_summary {
            payload["checks_summary"] = json!(checks);
        }
        self.write_ctl(&payload).await
    }

    /// Explicitly select the winning branch.
    pub async fn select(&self, id: &str) -> Result<(), ExecutableArenaError> {
        self.write_ctl(&json!({ "op": "select", "id": id })).await
    }

    /// Discard a branch from `branches/` while preserving lifecycle evidence in `events`.
    pub async fn discard(&self, id: &str) -> Result<(), ExecutableArenaError> {
        self.write_ctl(&json!({ "op": "discard", "id": id })).await
    }

    /// Read and deserialize `/mnt/branch/branches/<id>`.
    pub async fn read_branch(&self, id: &str) -> Result<BranchRecordView, ExecutableArenaError> {
        let bytes = self.read_file(&["branches", id]).await?;
        serde_json::from_slice(&bytes).map_err(|err| {
            ExecutableArenaError::BranchFsProtocol(format!(
                "invalid branch record JSON for `{id}`: {err}"
            ))
        })
    }

    /// Read `/mnt/branch/selected` JSON document.
    pub async fn read_selected(&self) -> Result<Value, ExecutableArenaError> {
        let bytes = self.read_file(&["selected"]).await?;
        serde_json::from_slice(&bytes).map_err(|err| {
            ExecutableArenaError::BranchFsProtocol(format!("invalid selected JSON: {err}"))
        })
    }

    /// Read `/mnt/branch/events` NDJSON lines as UTF-8.
    pub async fn read_events(&self) -> Result<String, ExecutableArenaError> {
        let bytes = self.read_file(&["events"]).await?;
        String::from_utf8(bytes).map_err(|err| {
            ExecutableArenaError::BranchFsProtocol(format!("events stream is not utf8: {err}"))
        })
    }

    async fn write_ctl(&self, payload: &Value) -> Result<(), ExecutableArenaError> {
        let encoded = serde_json::to_vec(payload).map_err(|err| {
            ExecutableArenaError::BranchFsProtocol(format!("serialize ctl command: {err}"))
        })?;
        let fid = self.walk(&["ctl"]).await?;
        if let Err(err) = self.open(fid, OpenMode::Write).await {
            let _ = self.clunk(fid).await;
            return Err(err);
        }
        if let Err(err) = self.write_all(fid, &encoded).await {
            let _ = self.clunk(fid).await;
            return Err(err);
        }
        self.clunk(fid).await
    }

    async fn read_file(&self, rel_segments: &[&str]) -> Result<Vec<u8>, ExecutableArenaError> {
        let fid = self.walk(rel_segments).await?;
        if let Err(err) = self.open(fid, OpenMode::Read).await {
            let _ = self.clunk(fid).await;
            return Err(err);
        }
        let result = self.read_all(fid).await;
        let clunk_res = self.clunk(fid).await;
        match (result, clunk_res) {
            (Ok(data), Ok(())) => Ok(data),
            (Err(err), _) => Err(err),
            (Ok(_), Err(err)) => Err(err),
        }
    }

    async fn walk(&self, rel_segments: &[&str]) -> Result<Fid, ExecutableArenaError> {
        let mut names = Vec::new();
        for segment in self
            .mount_prefix
            .split('/')
            .filter(|part| !part.is_empty())
            .chain(rel_segments.iter().copied())
        {
            names.push(segment.to_string());
        }
        let fid = Fid(NEXT_BRANCH_FID.fetch_add(1, Ordering::Relaxed));
        match self
            .transport
            .call(Request::Walk {
                fid: Fid::ROOT,
                newfid: fid,
                names,
            })
            .await
            .map_err(|err| ExecutableArenaError::BranchFsProtocol(err.to_string()))?
        {
            Response::Walk { .. } => Ok(fid),
            other => Err(ExecutableArenaError::BranchFsProtocol(format!(
                "unexpected walk response: {other:?}"
            ))),
        }
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<FileKind, ExecutableArenaError> {
        match self
            .transport
            .call(Request::Open { fid, mode })
            .await
            .map_err(|err| ExecutableArenaError::BranchFsProtocol(err.to_string()))?
        {
            Response::Open { qid } => Ok(qid.kind),
            other => Err(ExecutableArenaError::BranchFsProtocol(format!(
                "unexpected open response: {other:?}"
            ))),
        }
    }

    async fn write_all(&self, fid: Fid, bytes: &[u8]) -> Result<(), ExecutableArenaError> {
        let mut offset = 0_u64;
        for chunk in bytes.chunks(32 * 1024) {
            match self
                .transport
                .call(Request::Write {
                    fid,
                    offset,
                    data: chunk.to_vec(),
                })
                .await
                .map_err(|err| ExecutableArenaError::BranchFsProtocol(err.to_string()))?
            {
                Response::Write { count } => {
                    if count as usize != chunk.len() {
                        return Err(ExecutableArenaError::BranchFsProtocol(format!(
                            "short write to branchfs ctl: wrote {count} of {} bytes",
                            chunk.len()
                        )));
                    }
                    offset += u64::from(count);
                }
                other => {
                    return Err(ExecutableArenaError::BranchFsProtocol(format!(
                        "unexpected write response: {other:?}"
                    )));
                }
            }
        }
        Ok(())
    }

    async fn read_all(&self, fid: Fid) -> Result<Vec<u8>, ExecutableArenaError> {
        let stat = match self
            .transport
            .call(Request::Stat { fid })
            .await
            .map_err(|err| ExecutableArenaError::BranchFsProtocol(err.to_string()))?
        {
            Response::Stat { stat } => stat,
            other => {
                return Err(ExecutableArenaError::BranchFsProtocol(format!(
                    "unexpected stat response: {other:?}"
                )));
            }
        };

        let bounded_length =
            matches!(stat.qid.kind, FileKind::File | FileKind::Stream).then_some(stat.length);
        let mut offset = 0_u64;
        let mut out = Vec::new();
        loop {
            let count = bounded_length
                .map(|length| length.saturating_sub(offset).min(64 * 1024) as u32)
                .unwrap_or(64 * 1024);
            if count == 0 {
                break;
            }
            match self
                .transport
                .call(Request::Read { fid, offset, count })
                .await
                .map_err(|err| ExecutableArenaError::BranchFsProtocol(err.to_string()))?
            {
                Response::Read { data } => {
                    if data.is_empty() {
                        break;
                    }
                    offset += data.len() as u64;
                    out.extend_from_slice(&data);
                }
                other => {
                    return Err(ExecutableArenaError::BranchFsProtocol(format!(
                        "unexpected read response: {other:?}"
                    )));
                }
            }
        }
        Ok(out)
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ExecutableArenaError> {
        match self
            .transport
            .call(Request::Clunk { fid })
            .await
            .map_err(|err| ExecutableArenaError::BranchFsProtocol(err.to_string()))?
        {
            Response::Clunk => Ok(()),
            other => Err(ExecutableArenaError::BranchFsProtocol(format!(
                "unexpected clunk response: {other:?}"
            ))),
        }
    }
}
