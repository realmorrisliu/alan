//! Root-owned single-writer governance for structured child `memory_proposals`.
//!
//! Child workstreams mount project `alan-memfs` strictly read-only (`recall/<query>`)
//! and attach structured `memory_proposals` carrying rollout provenance on handoff.
//! Root scrubs secrets, deduplicates against existing committed entries, resolves
//! `supersedes` links, commits atomically to `memfs` over aP, and updates `/summary`
//! continuity state compatibly with user edit/delete/revision rollback controls.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use alan_ap::{ErrorCode, Fid, FileKind, InProcessTransport, OpenMode, Request, Response};
use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::redact_secret_tokens;

use super::memory_surfaces::project_summary::record_memory_proposals_into_summary;

pub const MEMORY_PROPOSALS_JSON_FILE: &str = "memory-proposals.json";
pub const MEMORY_PROPOSALS_MARKDOWN_FILE: &str = "project-memory.md";
const MAX_PROPOSAL_SUMMARY_CHARS: usize = 800;
const MAX_COMMITTED_PROPOSALS: usize = 256;

/// Category of a structured memory proposal emitted by a child workstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryProposalKind {
    Decision,
    Convention,
    Pitfall,
    FileIndex,
}

impl MemoryProposalKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Decision => "decision",
            Self::Convention => "convention",
            Self::Pitfall => "pitfall",
            Self::FileIndex => "file_index",
        }
    }
}

/// Structured memory proposal emitted by a child workstream on completion/handoff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryProposal {
    pub kind: MemoryProposalKind,
    pub summary: String,
    pub provenance_rollout: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
}

/// Persisted proposal entry committed atomically by Root into `alan-memfs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommittedMemoryProposal {
    pub id: String,
    pub kind: MemoryProposalKind,
    pub summary: String,
    pub provenance_rollout: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    pub committed_at: String,
    pub content_fingerprint: String,
}

/// Result of Root-owned secret-scrubbing, deduplication, and atomic commit of proposals.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryProposalCommitOutcome {
    pub committed: Vec<CommittedMemoryProposal>,
    pub skipped_duplicates: usize,
    pub superseded_count: usize,
    pub rejected_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memfs_file: Option<String>,
}

/// Extract structured `memory_proposals` from a child's `structured_output` JSON payload.
pub fn extract_memory_proposals(
    structured_output: Option<&Value>,
    fallback_rollout_ref: Option<&str>,
) -> Vec<MemoryProposal> {
    let Some(items) = structured_output
        .and_then(|val| val.get("memory_proposals"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let mut parsed = Vec::new();
    for item in items {
        let Ok(mut proposal) = serde_json::from_value::<MemoryProposal>(item.clone()) else {
            continue;
        };
        if proposal.provenance_rollout.trim().is_empty()
            && let Some(fallback) = fallback_rollout_ref
                .map(str::trim)
                .filter(|s| !s.is_empty())
        {
            proposal.provenance_rollout = fallback.to_string();
        }
        parsed.push(proposal);
    }
    parsed
}

/// Secret-scrub, deduplicate, resolve `supersedes`, and merge `proposals` into `existing` in memory.
pub fn apply_memory_proposals(
    existing: &mut Vec<CommittedMemoryProposal>,
    proposals: &[MemoryProposal],
) -> MemoryProposalCommitOutcome {
    let mut outcome = MemoryProposalCommitOutcome::default();
    if proposals.is_empty() {
        return outcome;
    }

    let mut known_fingerprints: HashMap<(MemoryProposalKind, String), usize> = existing
        .iter()
        .enumerate()
        .map(|(idx, entry)| ((entry.kind, normalize_summary_key(&entry.summary)), idx))
        .collect();
    let mut superseded_indices = BTreeSet::new();
    let mut newly_committed = Vec::new();

    for raw in proposals {
        let scrubbed_summary = sanitize_proposal_summary(&raw.summary);
        let scrubbed_provenance = redact_secret_tokens(raw.provenance_rollout.trim())
            .trim()
            .to_string();
        if scrubbed_summary.is_empty() || scrubbed_provenance.is_empty() {
            outcome.rejected_count += 1;
            continue;
        }
        let scrubbed_supersedes = raw
            .supersedes
            .as_deref()
            .map(redact_secret_tokens)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let norm_key = normalize_summary_key(&scrubbed_summary);
        let dedup_key = (raw.kind, norm_key.clone());
        if known_fingerprints.contains_key(&dedup_key) {
            outcome.skipped_duplicates += 1;
            continue;
        }

        if let Some(target) = scrubbed_supersedes.as_deref() {
            let target_norm = normalize_summary_key(target);
            for (idx, prev) in existing.iter().enumerate() {
                if !superseded_indices.contains(&idx)
                    && (prev.id == target || normalize_summary_key(&prev.summary) == target_norm)
                {
                    superseded_indices.insert(idx);
                    outcome.superseded_count += 1;
                }
            }
        }

        let content_fingerprint = fingerprint_proposal(raw.kind, &norm_key);
        let id = format!("mem-{}", &content_fingerprint[..12]);
        let entry = CommittedMemoryProposal {
            id,
            kind: raw.kind,
            summary: scrubbed_summary,
            provenance_rollout: scrubbed_provenance,
            supersedes: scrubbed_supersedes,
            committed_at: Utc::now().to_rfc3339(),
            content_fingerprint,
        };
        known_fingerprints.insert(dedup_key, existing.len() + newly_committed.len());
        newly_committed.push(entry);
    }

    if !superseded_indices.is_empty() {
        let mut retained = Vec::with_capacity(existing.len());
        for (idx, item) in existing.drain(..).enumerate() {
            if !superseded_indices.contains(&idx) {
                retained.push(item);
            }
        }
        *existing = retained;
    }

    existing.extend(newly_committed.iter().cloned());
    if existing.len() > MAX_COMMITTED_PROPOSALS {
        let excess = existing.len() - MAX_COMMITTED_PROPOSALS;
        existing.drain(0..excess);
    }
    outcome.committed = newly_committed;
    outcome
}

/// Render committed proposals into searchable Markdown for `alan-memfs` `recall/<query>`.
pub fn render_proposals_markdown(entries: &[CommittedMemoryProposal]) -> String {
    let mut out = String::from(
        "# Project Shared Memory (Root-Committed Proposals)\n\n\
         > Committed atomically by Root from child workstream proposals with rollout provenance.\n\n",
    );
    if entries.is_empty() {
        out.push_str("_No proposals committed yet._\n");
        return out;
    }
    for entry in entries {
        out.push_str(&format!(
            "- [{}] ({}) {} (provenance: {}, id: {})\n",
            entry.kind.as_str(),
            entry.committed_at,
            entry.summary,
            entry.provenance_rollout,
            entry.id
        ));
    }
    out
}

/// Atomically commit Root-governed `proposals` onto a mounted `alan-memfs` over aP (`memfs_mount`)
/// and optionally sync into `/summary` continuity state under `memory_dir`.
pub async fn commit_memory_proposals_over_ap(
    root: &InProcessTransport,
    memfs_mount: &str,
    proposals: &[MemoryProposal],
    memory_dir: Option<&Path>,
    project_cwd: Option<&str>,
) -> Result<MemoryProposalCommitOutcome> {
    if proposals.is_empty() {
        return Ok(MemoryProposalCommitOutcome::default());
    }
    let mount = memfs_mount.trim_end_matches('/');
    let json_path = format!("{mount}/{MEMORY_PROPOSALS_JSON_FILE}");
    let md_path = format!("{mount}/{MEMORY_PROPOSALS_MARKDOWN_FILE}");

    let mut existing = match try_read_ap_file(root, &json_path).await? {
        Some(bytes) if !bytes.is_empty() => {
            serde_json::from_slice::<Vec<CommittedMemoryProposal>>(&bytes)
                .with_context(|| format!("parse existing proposals at {json_path}"))?
        }
        _ => Vec::new(),
    };

    let mut outcome = apply_memory_proposals(&mut existing, proposals);
    if outcome.committed.is_empty() && outcome.superseded_count == 0 {
        return Ok(outcome);
    }

    let json_bytes = serde_json::to_vec_pretty(&existing)?;
    let md_bytes = render_proposals_markdown(&existing).into_bytes();

    write_or_create_ap_file(root, &json_path, &json_bytes)
        .await
        .with_context(|| format!("commit proposals JSON to {json_path}"))?;
    write_or_create_ap_file(root, &md_path, &md_bytes)
        .await
        .with_context(|| format!("commit proposals Markdown to {md_path}"))?;
    outcome.memfs_file = Some(md_path);

    if let Some(dir) = memory_dir
        && !outcome.committed.is_empty()
    {
        record_memory_proposals_into_summary(dir, project_cwd, &outcome.committed)?;
    }

    Ok(outcome)
}

fn sanitize_proposal_summary(raw: &str) -> String {
    let scrubbed = redact_secret_tokens(raw.trim());
    let cleaned = scrubbed.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.chars().count() <= MAX_PROPOSAL_SUMMARY_CHARS {
        return cleaned;
    }
    let mut out: String = cleaned
        .chars()
        .take(MAX_PROPOSAL_SUMMARY_CHARS.saturating_sub(3))
        .collect();
    out.push_str("...");
    out
}

fn normalize_summary_key(raw: &str) -> String {
    raw.split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

fn fingerprint_proposal(kind: MemoryProposalKind, normalized_summary: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(kind.as_str().as_bytes());
    hasher.update(b"\x1f");
    hasher.update(normalized_summary.as_bytes());
    hex::encode(hasher.finalize())
}

static NEXT_AP_FID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(500_000);

fn next_fid() -> Fid {
    Fid(NEXT_AP_FID.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

fn split_ap_path(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

pub(in crate::runtime) async fn try_read_ap_file(
    root: &InProcessTransport,
    path: &str,
) -> Result<Option<Vec<u8>>> {
    let fid = next_fid();
    match root
        .call(Request::Walk {
            fid: Fid::ROOT,
            newfid: fid,
            names: split_ap_path(path),
        })
        .await
    {
        Ok(Response::Walk { .. }) => {}
        Ok(_) => bail!("unexpected walk response for {path}"),
        Err(ErrorCode::NotFound) => return Ok(None),
        Err(err) => return Err(err).with_context(|| format!("walk {path}")),
    }

    let res = async {
        root.call(Request::Open {
            fid,
            mode: OpenMode::Read,
        })
        .await?;
        let mut out = Vec::new();
        let mut offset = 0u64;
        loop {
            let Response::Read { data } = root
                .call(Request::Read {
                    fid,
                    offset,
                    count: 64 * 1024,
                })
                .await?
            else {
                bail!("unexpected read response for {path}");
            };
            if data.is_empty() {
                break;
            }
            offset += data.len() as u64;
            out.extend_from_slice(&data);
        }
        Ok(Some(out))
    }
    .await;
    let _ = root.call(Request::Clunk { fid }).await;
    res
}

async fn write_or_create_ap_file(root: &InProcessTransport, path: &str, data: &[u8]) -> Result<()> {
    let mut parts = split_ap_path(path);
    let file_name = parts.pop().context("missing file name in aP path")?;
    let fid = next_fid();

    let walked = root
        .call(Request::Walk {
            fid: Fid::ROOT,
            newfid: fid,
            names: split_ap_path(path),
        })
        .await;

    let open_fid = match walked {
        Ok(Response::Walk { .. }) => {
            root.call(Request::Open {
                fid,
                mode: OpenMode::Write,
            })
            .await?;
            fid
        }
        Err(ErrorCode::NotFound) => {
            let dir_fid = next_fid();
            root.call(Request::Walk {
                fid: Fid::ROOT,
                newfid: dir_fid,
                names: parts,
            })
            .await?;
            let create_res = root
                .call(Request::Create {
                    fid: dir_fid,
                    newfid: fid,
                    name: file_name,
                    kind: FileKind::File,
                })
                .await;
            let _ = root.call(Request::Clunk { fid: dir_fid }).await;
            create_res?;
            root.call(Request::Open {
                fid,
                mode: OpenMode::Write,
            })
            .await?;
            fid
        }
        Ok(_) => bail!("unexpected walk response for {path}"),
        Err(err) => return Err(err).with_context(|| format!("walk {path}")),
    };

    let write_res = async {
        let mut offset = 0u64;
        while (offset as usize) < data.len() {
            let chunk_end = (offset as usize + 64 * 1024).min(data.len());
            let Response::Write { count } = root
                .call(Request::Write {
                    fid: open_fid,
                    offset,
                    data: data[offset as usize..chunk_end].to_vec(),
                })
                .await?
            else {
                bail!("unexpected write response for {path}");
            };
            if count == 0 {
                bail!("zero-byte write to {path}");
            }
            offset += count as u64;
        }
        Ok(())
    }
    .await;

    let clunk_res = root.call(Request::Clunk { fid: open_fid }).await;
    write_res?;
    clunk_res?;
    Ok(())
}

#[cfg(test)]
#[path = "memory_proposals_tests.rs"]
mod tests;
