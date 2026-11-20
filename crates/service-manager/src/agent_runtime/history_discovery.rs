//! Bounded rollout history discovery, diagnostics, cross-project isolation, and `/agent/history` FS.

#[path = "history_discovery/runtime_binding.rs"]
mod runtime_binding;

use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{BufRead, BufReader, Read},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
    time::SystemTime,
};

use alan_agent_engine::{
    AgentMachineMeta, AgentRuntimeStoreBindings, EffectStatus, MessageRecord, RolloutItem,
    redact_secret_tokens,
};
use alan_ap::ErrorCode;
use anyhow::{Context, Result, bail, ensure};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::root_recovery::RootRolloutState;
use crate::flat_fs::FlatFileService;
pub(super) use runtime_binding::PendingRootRolloutSwitch;

const MAX_DISCOVERY_FILES: usize = 64;
const MAX_SCAN_LINE_BYTES: usize = 256 * 1024;
const MAX_LINES_PER_FILE: usize = 4_096;
const MAX_GOAL_SNIPPET_CHARS: usize = 140;
const MAX_PROJECT_INDEX_ENTRIES: usize = 256;
const PROJECT_INDEX_FILENAME: &str = "rollout-projects.json";

/// Scope used to isolate rollout discovery strictly to the active authorized project.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct HistoryProjectScope {
    /// Active normalized workspace / namespace cwd (for example `/mnt/project` or `/`).
    pub(crate) project_cwd: Option<String>,
    /// Optional Host-backed fingerprint recorded in `metadata/rollout-projects.json` when available.
    pub(crate) project_fingerprint: Option<String>,
    /// Safe display label (directory basename), never a raw host path.
    pub(crate) project_label: Option<String>,
}

impl HistoryProjectScope {
    pub(crate) fn from_cwd(cwd: Option<&str>) -> Self {
        let normalized = cwd.map(normalize_scope_path).filter(|s| !s.is_empty());
        let project_label = normalized.as_deref().and_then(derive_project_label);
        Self {
            project_cwd: normalized,
            project_fingerprint: None,
            project_label,
        }
    }

    pub(crate) fn with_fingerprint(mut self, fingerprint: Option<String>) -> Self {
        self.project_fingerprint = fingerprint;
        self
    }

    pub(crate) fn with_label(mut self, label: Option<String>) -> Self {
        if label.as_deref().is_some_and(|l| !l.trim().is_empty()) {
            self.project_label = label;
        }
        self
    }
}

/// Summary metadata for one authorized, discoverable rollout file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RolloutHistoryEntry {
    pub rollout_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_fingerprint: Option<String>,
    pub turn_count: usize,
    pub last_status: String,
    pub goal_snippet: String,
    pub has_pending_checkpoint: bool,
    pub has_unknown_effects: bool,
    pub corrupt_lines: usize,
}

/// Non-fatal per-file diagnostic recorded during bounded rollout scanning.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryDiscoveryDiagnostic {
    pub file_name: String,
    pub message: String,
}

/// Complete catalog projected at `/agent/history/catalog`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RolloutHistoryCatalog {
    pub current_rollout_id: Option<String>,
    pub active_project_cwd: Option<String>,
    pub active_project_fingerprint: Option<String>,
    pub excluded_cross_project_count: usize,
    pub entries: Vec<RolloutHistoryEntry>,
    pub diagnostics: Vec<HistoryDiscoveryDiagnostic>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RolloutProjectIndexEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_fingerprint: Option<String>,
    updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RolloutProjectIndex {
    entries: BTreeMap<String, RolloutProjectIndexEntry>,
}

/// Compute a non-reversible fingerprint for a project boundary without exposing Host paths.
pub(crate) fn fingerprint_project_boundary(raw: &str) -> String {
    let digest = Sha256::digest(raw.as_bytes());
    hex_prefix(&digest, 16)
}

fn hex_prefix(bytes: &[u8], hex_chars: usize) -> String {
    let mut out = String::with_capacity(hex_chars);
    for byte in bytes.iter().take(hex_chars / 2) {
        use std::fmt::Write;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

/// Record project provenance for a Root rollout inside `metadata/rollout-projects.json`.
pub(crate) fn record_rollout_project_index(
    stores: &AgentRuntimeStoreBindings,
    rollout_path: &Path,
    scope: &HistoryProjectScope,
) -> Result<()> {
    let Some(file_name) = rollout_path.file_name().and_then(|n| n.to_str()) else {
        return Ok(());
    };
    if !is_safe_leaf_filename(file_name) {
        return Ok(());
    }
    fs::create_dir_all(&stores.metadata)?;
    let index_path = stores.metadata.join(PROJECT_INDEX_FILENAME);
    let mut index = read_project_index(stores);
    index.entries.insert(
        file_name.to_string(),
        RolloutProjectIndexEntry {
            project_cwd: scope.project_cwd.clone(),
            project_label: scope.project_label.clone(),
            project_fingerprint: scope.project_fingerprint.clone(),
            updated_at: now_rfc3339(),
        },
    );
    while index.entries.len() > MAX_PROJECT_INDEX_ENTRIES {
        if let Some(oldest_key) = index
            .entries
            .iter()
            .min_by(|(_, a), (_, b)| a.updated_at.cmp(&b.updated_at))
            .map(|(k, _)| k.clone())
        {
            index.entries.remove(&oldest_key);
        } else {
            break;
        }
    }
    let bytes = serde_json::to_vec_pretty(&index)?;
    let mut temp = tempfile::NamedTempFile::new_in(&stores.metadata)?;
    std::io::Write::write_all(&mut temp, &bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(index_path)
        .context("persist rollout project index")?;
    Ok(())
}

fn read_project_index(stores: &AgentRuntimeStoreBindings) -> RolloutProjectIndex {
    let path = stores.metadata.join(PROJECT_INDEX_FILENAME);
    let Ok(bytes) = fs::read(path) else {
        return RolloutProjectIndex::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

/// Discover authorized, project-isolated rollouts under `stores.rollouts`.
pub(crate) fn discover_rollout_history(
    stores: &AgentRuntimeStoreBindings,
    scope: &HistoryProjectScope,
    current_rollout_id: Option<&str>,
    query: Option<&str>,
    limit: usize,
) -> RolloutHistoryCatalog {
    let mut catalog = RolloutHistoryCatalog {
        current_rollout_id: current_rollout_id.map(str::to_string),
        active_project_cwd: scope.project_cwd.clone(),
        active_project_fingerprint: scope.project_fingerprint.clone(),
        excluded_cross_project_count: 0,
        entries: Vec::new(),
        diagnostics: Vec::new(),
    };

    let Ok(read_dir) = fs::read_dir(&stores.rollouts) else {
        return catalog;
    };

    let project_index = read_project_index(stores);
    let mut candidates: Vec<(SystemTime, String, PathBuf)> = Vec::new();
    for item in read_dir {
        let Ok(entry) = item else { continue };
        let path = entry.path();
        let Some(name_os) = path.file_name() else {
            continue;
        };
        let name = name_os.to_string_lossy().to_string();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() || !meta.file_type().is_file() {
            catalog.diagnostics.push(HistoryDiscoveryDiagnostic {
                file_name: name,
                message: "ignored symlink or non-regular entry in rollout store".to_string(),
            });
            continue;
        }
        if !name.ends_with(".jsonl") || !is_safe_leaf_filename(&name) {
            continue;
        }
        let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        candidates.push((mtime, name, path));
    }

    candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));

    let effective_limit = limit.clamp(1, MAX_DISCOVERY_FILES);
    let mut seen_ids = HashSet::new();
    let normalized_query = query
        .map(|q| q.trim().to_ascii_lowercase())
        .filter(|q| !q.is_empty());

    for (mtime, file_name, path) in candidates.into_iter().take(MAX_DISCOVERY_FILES) {
        let parsed = parse_rollout_file(
            &file_name,
            &path,
            mtime,
            project_index.entries.get(&file_name),
        );
        catalog.diagnostics.extend(parsed.diagnostics);
        let Some(entry) = parsed.entry else {
            continue;
        };
        if !seen_ids.insert(entry.rollout_id.clone()) {
            continue;
        }
        if !entry_matches_scope(&entry, scope) {
            catalog.excluded_cross_project_count += 1;
            continue;
        }
        if let Some(q) = normalized_query.as_deref()
            && !entry_matches_query(&entry, q)
        {
            continue;
        }
        catalog.entries.push(entry);
        if catalog.entries.len() >= effective_limit {
            break;
        }
    }

    catalog.entries.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| b.rollout_id.cmp(&a.rollout_id))
    });
    catalog
}

/// Resolve an explicit rollout ID strictly inside `stores.rollouts` and authorized `scope`.
pub(crate) fn resolve_authorized_rollout_file(
    stores: &AgentRuntimeStoreBindings,
    scope: &HistoryProjectScope,
    requested_id: &str,
) -> Result<(PathBuf, RolloutHistoryEntry)> {
    let requested = requested_id.trim();
    ensure!(
        !requested.is_empty()
            && requested.chars().count() <= 200
            && !requested.contains(['/', '\\'])
            && !requested.chars().any(char::is_control),
        "invalid rollout id '{requested}'"
    );

    let read_dir = fs::read_dir(&stores.rollouts).with_context(|| {
        format!(
            "rollout store '{}' is unavailable",
            stores.rollouts.display()
        )
    })?;
    let project_index = read_project_index(stores);
    let mut cross_project_match = false;
    let mut candidates = Vec::new();
    for item in read_dir {
        let Ok(entry) = item else { continue };
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".jsonl") || !is_safe_leaf_filename(name) {
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() || !meta.file_type().is_file() {
            continue;
        }
        let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        candidates.push((mtime, name.to_string(), path));
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));

    for (mtime, file_name, path) in candidates.into_iter().take(MAX_DISCOVERY_FILES) {
        let parsed = parse_rollout_file(
            &file_name,
            &path,
            mtime,
            project_index.entries.get(&file_name),
        );
        let Some(entry) = parsed.entry else { continue };
        let stem = file_name.strip_suffix(".jsonl").unwrap_or(&file_name);
        if entry.rollout_id == requested || stem == requested {
            if !entry_matches_scope(&entry, scope) {
                cross_project_match = true;
                continue;
            }
            return Ok((path, entry));
        }
    }

    if cross_project_match {
        bail!(
            "rollout '{requested}' belongs to another project and cannot be resumed in this workspace"
        );
    }
    bail!("no authorized rollout matches id '{requested}'")
}

struct ParsedRolloutFile {
    entry: Option<RolloutHistoryEntry>,
    diagnostics: Vec<HistoryDiscoveryDiagnostic>,
}

fn parse_rollout_file(
    file_name: &str,
    path: &Path,
    mtime: SystemTime,
    index_entry: Option<&RolloutProjectIndexEntry>,
) -> ParsedRolloutFile {
    let mut diagnostics = Vec::new();
    let Ok(file) = fs::File::open(path) else {
        diagnostics.push(HistoryDiscoveryDiagnostic {
            file_name: file_name.to_string(),
            message: "unable to open rollout file".to_string(),
        });
        return ParsedRolloutFile {
            entry: None,
            diagnostics,
        };
    };

    let mut reader = BufReader::new(file);
    let mut meta: Option<AgentMachineMeta> = None;
    let mut updated_at: Option<String> = None;
    let mut project_cwd: Option<String> = index_entry.and_then(|idx| idx.project_cwd.clone());
    let mut project_label: Option<String> = index_entry.and_then(|idx| idx.project_label.clone());
    let project_fingerprint = index_entry.and_then(|idx| idx.project_fingerprint.clone());
    let mut turn_count = 0usize;
    let mut first_user_goal: Option<String> = None;
    let mut pending_checkpoints: HashSet<String> = HashSet::new();
    let mut has_unknown_effects = false;
    let mut saw_turn_completed = false;
    let mut saw_error_or_cancelled = false;
    let mut corrupt_lines = 0usize;
    let mut line_index = 0usize;

    loop {
        if line_index >= MAX_LINES_PER_FILE {
            diagnostics.push(HistoryDiscoveryDiagnostic {
                file_name: file_name.to_string(),
                message: format!("stopped scanning after {MAX_LINES_PER_FILE} lines"),
            });
            break;
        }
        let line_result = read_bounded_line(&mut reader, MAX_SCAN_LINE_BYTES);
        let bytes = match line_result {
            Ok(Some(bytes)) => bytes,
            Ok(None) => break,
            Err(ReadLineError::Oversized) => {
                corrupt_lines += 1;
                diagnostics.push(HistoryDiscoveryDiagnostic {
                    file_name: file_name.to_string(),
                    message: format!(
                        "line {} exceeded {MAX_SCAN_LINE_BYTES} byte cap",
                        line_index + 1
                    ),
                });
                line_index += 1;
                continue;
            }
            Err(ReadLineError::Io(err)) => {
                diagnostics.push(HistoryDiscoveryDiagnostic {
                    file_name: file_name.to_string(),
                    message: format!("read error at line {}: {err}", line_index + 1),
                });
                break;
            }
        };
        line_index += 1;
        let Ok(text) = std::str::from_utf8(&bytes) else {
            corrupt_lines += 1;
            diagnostics.push(HistoryDiscoveryDiagnostic {
                file_name: file_name.to_string(),
                message: format!("non-UTF8 bytes at line {line_index}"),
            });
            continue;
        };
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(item) = serde_json::from_str::<RolloutItem>(trimmed) else {
            corrupt_lines += 1;
            diagnostics.push(HistoryDiscoveryDiagnostic {
                file_name: file_name.to_string(),
                message: format!("corrupt or torn JSONL record at line {line_index}"),
            });
            continue;
        };

        match item {
            RolloutItem::AgentMachineMeta(m) => {
                updated_at.get_or_insert_with(|| m.started_at.clone());
                if project_cwd.is_none() {
                    project_cwd = Some(normalize_scope_path(&m.cwd));
                }
                meta = Some(m);
            }
            RolloutItem::Message(msg) => {
                updated_at = Some(msg.timestamp.clone());
                if msg.role == "user" {
                    turn_count += 1;
                    if first_user_goal.is_none()
                        && let Some(raw_text) = extract_user_message_text(&msg)
                    {
                        first_user_goal = Some(sanitize_goal_snippet(&raw_text));
                    }
                }
            }
            RolloutItem::TurnContext(ctx) => {
                updated_at = Some(ctx.timestamp);
            }
            RolloutItem::CompactionAttempt(attempt) => {
                updated_at = Some(attempt.timestamp);
            }
            RolloutItem::MemoryFlushAttempt(attempt) => {
                updated_at = Some(attempt.timestamp);
            }
            RolloutItem::Compacted(compacted) => {
                updated_at = Some(compacted.timestamp);
            }
            RolloutItem::ToolCall(tc) => {
                updated_at = Some(tc.timestamp);
            }
            RolloutItem::Effect(effect) => {
                updated_at = Some(effect.timestamp);
                if effect.status == EffectStatus::Unknown {
                    has_unknown_effects = true;
                }
            }
            RolloutItem::Checkpoint(cp) => {
                updated_at = Some(cp.timestamp);
                if cp.choice.is_none() {
                    pending_checkpoints.insert(cp.checkpoint_id);
                } else {
                    pending_checkpoints.remove(&cp.checkpoint_id);
                }
            }
            RolloutItem::Event(ev) => {
                updated_at = Some(ev.timestamp);
                if ev.event_type == "agent_action_v1"
                    && let Some(record) = ev.payload.get("record")
                {
                    let name = record.get("name").and_then(|v| v.as_str());
                    if matches!(name, Some("project-cwd-v1" | "cd"))
                        && let Some(result_str) = record.get("result").and_then(|v| v.as_str())
                        && let Ok(result_json) =
                            serde_json::from_str::<serde_json::Value>(result_str)
                        && let Some(cwd) =
                            result_json.pointer("/outcome/cwd").and_then(|v| v.as_str())
                    {
                        project_cwd = Some(normalize_scope_path(cwd));
                    }
                } else if ev.event_type.contains("completed") || ev.event_type == "turn_completed" {
                    saw_turn_completed = true;
                } else if ev.event_type.contains("cancelled") || ev.event_type.contains("failed") {
                    saw_error_or_cancelled = true;
                }
            }
        }
    }

    let Some(meta) = meta else {
        diagnostics.push(HistoryDiscoveryDiagnostic {
            file_name: file_name.to_string(),
            message: "rollout missing AgentMachineMeta header".to_string(),
        });
        return ParsedRolloutFile {
            entry: None,
            diagnostics,
        };
    };

    if project_label.is_none() {
        project_label = project_cwd.as_deref().and_then(derive_project_label);
    }
    let has_pending_checkpoint = !pending_checkpoints.is_empty();
    let last_status = if has_pending_checkpoint {
        "paused"
    } else if has_unknown_effects || saw_error_or_cancelled {
        "attention"
    } else if saw_turn_completed || turn_count > 0 {
        if corrupt_lines > 0 {
            "partial"
        } else {
            "completed"
        }
    } else {
        "empty"
    };

    let updated_at = updated_at.unwrap_or_else(|| system_time_to_rfc3339(mtime));
    let goal_snippet = first_user_goal.unwrap_or_else(|| "(no user prompt recorded)".to_string());

    ParsedRolloutFile {
        entry: Some(RolloutHistoryEntry {
            rollout_id: meta.rollout_id,
            file_name: Some(file_name.to_string()),
            updated_at,
            project_cwd,
            project_label,
            project_fingerprint,
            turn_count,
            last_status: last_status.to_string(),
            goal_snippet,
            has_pending_checkpoint,
            has_unknown_effects,
            corrupt_lines,
        }),
        diagnostics,
    }
}

enum ReadLineError {
    Oversized,
    Io(std::io::Error),
}

fn read_bounded_line(
    reader: &mut BufReader<fs::File>,
    max_bytes: usize,
) -> std::result::Result<Option<Vec<u8>>, ReadLineError> {
    let mut buf = Vec::new();
    let mut limited = reader.take((max_bytes + 1) as u64);
    let read = limited
        .read_until(b'\n', &mut buf)
        .map_err(ReadLineError::Io)?;
    if read == 0 {
        return Ok(None);
    }
    if buf.len() > max_bytes && !buf.ends_with(b"\n") {
        // Drain until end of this oversized line so subsequent lines parse cleanly.
        let mut discard = Vec::new();
        loop {
            discard.clear();
            let mut drain = reader.take(8192);
            let n = drain
                .read_until(b'\n', &mut discard)
                .map_err(ReadLineError::Io)?;
            if n == 0 || discard.ends_with(b"\n") {
                break;
            }
        }
        return Err(ReadLineError::Oversized);
    }
    Ok(Some(buf))
}

fn extract_user_message_text(record: &MessageRecord) -> Option<String> {
    if let Some(content) = record.content.as_deref()
        && !content.trim().is_empty()
    {
        return Some(content.to_string());
    }
    record.message.as_ref().and_then(|msg| {
        let text = msg.text_content();
        (!text.trim().is_empty()).then_some(text)
    })
}

fn sanitize_goal_snippet(raw: &str) -> String {
    let scrubbed = redact_secret_tokens(raw);
    let collapsed = scrubbed.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= MAX_GOAL_SNIPPET_CHARS {
        return collapsed;
    }
    let truncated: String = collapsed
        .chars()
        .take(MAX_GOAL_SNIPPET_CHARS.saturating_sub(3))
        .collect();
    format!("{truncated}...")
}

fn entry_matches_scope(entry: &RolloutHistoryEntry, scope: &HistoryProjectScope) -> bool {
    if let Some(expected_fp) = scope.project_fingerprint.as_deref() {
        if let Some(entry_fp) = entry.project_fingerprint.as_deref() {
            return entry_fp == expected_fp;
        }
        // An entry without a fingerprint only matches if scope also names a distinct path != "/"
        // that does not look like a generic shared mount point.
        return matches!(
            (entry.project_cwd.as_deref(), scope.project_cwd.as_deref()),
            (Some(entry_cwd), Some(scope_cwd))
                if scope_cwd != "/" && path_prefix_matches(entry_cwd, scope_cwd)
        );
    }

    let Some(scope_cwd) = scope.project_cwd.as_deref() else {
        return true;
    };
    let Some(entry_cwd) = entry.project_cwd.as_deref() else {
        return scope_cwd == "/";
    };
    if scope_cwd == "/" {
        return entry_cwd == "/";
    }
    path_prefix_matches(entry_cwd, scope_cwd)
}

fn path_prefix_matches(candidate: &str, scope: &str) -> bool {
    let c = normalize_scope_path(candidate);
    let s = normalize_scope_path(scope);
    c == s || c.starts_with(&format!("{}/", s.trim_end_matches('/')))
}

fn entry_matches_query(entry: &RolloutHistoryEntry, lower_query: &str) -> bool {
    entry.rollout_id.to_ascii_lowercase().contains(lower_query)
        || entry
            .goal_snippet
            .to_ascii_lowercase()
            .contains(lower_query)
        || entry.last_status.to_ascii_lowercase().contains(lower_query)
        || entry
            .project_label
            .as_deref()
            .is_some_and(|l| l.to_ascii_lowercase().contains(lower_query))
        || entry
            .project_cwd
            .as_deref()
            .is_some_and(|c| c.to_ascii_lowercase().contains(lower_query))
}

fn normalize_scope_path(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut out = PathBuf::new();
    for component in Path::new(trimmed).components() {
        match component {
            Component::Prefix(prefix) => out.push(prefix.as_os_str()),
            Component::RootDir => out.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part),
        }
    }
    let s = out.to_string_lossy().to_string();
    if s.is_empty() { "/".to_string() } else { s }
}

fn derive_project_label(cwd: &str) -> Option<String> {
    if cwd == "/" {
        return Some("workspace (/)".to_string());
    }
    Path::new(cwd)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
}

fn is_safe_leaf_filename(name: &str) -> bool {
    let mut components = Path::new(name).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn system_time_to_rfc3339(time: SystemTime) -> String {
    let dt: chrono::DateTime<chrono::Utc> = time.into();
    dt.to_rfc3339()
}

/// Callback invoked by `/agent/history/ctl` to switch Root Agent continuity.
pub(crate) type SwitchRootRolloutFn = Arc<
    dyn Fn(
            Option<PathBuf>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>>
        + Send
        + Sync,
>;

/// Snapshot provider returning `(Option<stores>, HistoryProjectScope, Option<current_rollout_id>, ephemeral)`.
pub(crate) type HistoryContextProviderFn = Arc<
    dyn Fn() -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = (
                            Option<AgentRuntimeStoreBindings>,
                            HistoryProjectScope,
                            Option<String>,
                            bool,
                        ),
                    > + Send,
            >,
        > + Send
        + Sync,
>;

/// Provider returning an optional Process Memory Store path for `/agent/history/summary`.
pub(crate) type MemoryStoreProviderFn = Arc<dyn Fn() -> Option<PathBuf> + Send + Sync>;

/// `/agent/history` flat file service exposing `catalog`, `ctl`, `summary`, and `summary_ctl`.
pub(crate) struct HistoryDiscoveryService {
    context_provider: HistoryContextProviderFn,
    switch_root: SwitchRootRolloutFn,
    memory_store_provider: Option<MemoryStoreProviderFn>,
    query_filter: Mutex<Option<String>>,
}

impl HistoryDiscoveryService {
    pub(crate) fn new(
        context_provider: HistoryContextProviderFn,
        switch_root: SwitchRootRolloutFn,
    ) -> Self {
        Self {
            context_provider,
            switch_root,
            memory_store_provider: None,
            query_filter: Mutex::new(None),
        }
    }

    pub(crate) fn with_memory_store_provider(mut self, provider: MemoryStoreProviderFn) -> Self {
        self.memory_store_provider = Some(provider);
        self
    }
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum HistoryCtlJson {
    Resume { rollout_id: String },
    Fresh,
    Filter { query: Option<String> },
}

#[async_trait]
impl FlatFileService for HistoryDiscoveryService {
    fn files(&self) -> &'static [(&'static str, bool)] {
        &[
            ("catalog", false),
            ("ctl", true),
            ("summary", false),
            ("summary_ctl", true),
        ]
    }

    async fn read(&self, name: &str) -> std::result::Result<Vec<u8>, ErrorCode> {
        let (stores, scope, current_id, ephemeral) = (self.context_provider)().await;
        if name == "summary" {
            let memory_dir = self
                .memory_store_provider
                .as_ref()
                .and_then(|provider| provider());
            return runtime_binding::read_project_summary_bytes(
                memory_dir.as_deref(),
                scope.project_cwd.as_deref(),
                ephemeral,
            );
        }
        if name != "catalog" {
            return Err(ErrorCode::NotFound);
        }
        if ephemeral {
            return serde_json::to_vec(&RolloutHistoryCatalog {
                current_rollout_id: current_id,
                active_project_cwd: scope.project_cwd,
                active_project_fingerprint: scope.project_fingerprint,
                excluded_cross_project_count: 0,
                entries: Vec::new(),
                diagnostics: vec![HistoryDiscoveryDiagnostic {
                    file_name: "ephemeral".to_string(),
                    message: "rollout discovery disabled in ephemeral rollout mode".to_string(),
                }],
            })
            .map_err(|_| ErrorCode::Io);
        }
        let Some(stores) = stores else {
            return serde_json::to_vec(&RolloutHistoryCatalog::default())
                .map_err(|_| ErrorCode::Io);
        };
        let query = self
            .query_filter
            .lock()
            .expect("history query lock poisoned")
            .clone();
        let catalog = discover_rollout_history(
            &stores,
            &scope,
            current_id.as_deref(),
            query.as_deref(),
            MAX_DISCOVERY_FILES,
        );
        serde_json::to_vec(&catalog).map_err(|_| ErrorCode::Io)
    }

    async fn commit(&self, name: &str, bytes: &[u8]) -> std::result::Result<(), ErrorCode> {
        if name == "summary_ctl" {
            let (_, scope, _, ephemeral) = (self.context_provider)().await;
            let memory_dir = self
                .memory_store_provider
                .as_ref()
                .and_then(|provider| provider());
            return runtime_binding::commit_project_summary_ctl(
                memory_dir.as_deref(),
                scope.project_cwd.as_deref(),
                ephemeral,
                bytes,
            );
        }
        if name != "ctl" {
            return Err(ErrorCode::NoAccess);
        }
        let text = std::str::from_utf8(bytes).map_err(|_| ErrorCode::BadRequest)?;
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(ErrorCode::BadRequest);
        }

        let cmd = if trimmed.starts_with('{') {
            serde_json::from_str::<HistoryCtlJson>(trimmed).map_err(|_| ErrorCode::BadRequest)?
        } else if trimmed == "fresh" {
            HistoryCtlJson::Fresh
        } else if let Some(rest) = trimmed.strip_prefix("resume ") {
            HistoryCtlJson::Resume {
                rollout_id: rest.trim().to_string(),
            }
        } else if let Some(rest) = trimmed.strip_prefix("filter") {
            let q = rest.trim();
            HistoryCtlJson::Filter {
                query: (!q.is_empty()).then(|| q.to_string()),
            }
        } else {
            return Err(ErrorCode::BadRequest);
        };

        let (stores, scope, _, ephemeral) = (self.context_provider)().await;
        match cmd {
            HistoryCtlJson::Filter { query } => {
                *self
                    .query_filter
                    .lock()
                    .expect("history query lock poisoned") = query;
                Ok(())
            }
            HistoryCtlJson::Fresh => {
                if ephemeral {
                    return Err(ErrorCode::NoAccess);
                }
                (self.switch_root)(None).await.map_err(|_| ErrorCode::Io)
            }
            HistoryCtlJson::Resume { rollout_id } => {
                if ephemeral {
                    return Err(ErrorCode::NoAccess);
                }
                let Some(stores) = stores else {
                    return Err(ErrorCode::NotFound);
                };
                let (path, _) = resolve_authorized_rollout_file(&stores, &scope, &rollout_id)
                    .map_err(|err| {
                        let msg = err.to_string();
                        if msg.contains("another project") {
                            ErrorCode::NoAccess
                        } else {
                            ErrorCode::NotFound
                        }
                    })?;
                (self.switch_root)(Some(path))
                    .await
                    .map_err(|_| ErrorCode::Io)
            }
        }
    }
}

pub(crate) fn current_rollout_id_from_state(state: &RootRolloutState) -> Option<String> {
    match state {
        RootRolloutState::Durable(path) => read_rollout_id_header(path),
        RootRolloutState::NotStarted | RootRolloutState::NoDurableRollout => None,
    }
}

fn read_rollout_id_header(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    for _ in 0..8 {
        let Ok(Some(bytes)) = read_bounded_line(&mut reader, MAX_SCAN_LINE_BYTES) else {
            break;
        };
        let Ok(line) = std::str::from_utf8(&bytes) else {
            continue;
        };
        if let Ok(RolloutItem::AgentMachineMeta(meta)) =
            serde_json::from_str::<RolloutItem>(line.trim())
        {
            return Some(meta.rollout_id);
        }
    }
    path.file_stem().map(|s| s.to_string_lossy().to_string())
}

#[cfg(test)]
#[path = "history_discovery/tests.rs"]
mod tests;
