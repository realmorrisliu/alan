//! Workstream ledger DTOs, `/streams` inline overview, `/mnt/route/log` parsing,
//! and compact footer status-bar formatting.

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use ratatui::text::{Line, Span};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(test)]
use unicode_width::UnicodeWidthStr;

use crate::history::{clean_text, metadata_style, wrap_styled_lines};
use crate::theme::Theme;

#[cfg(test)]
use super::FileBackedApp;

const MAX_DISPLAY_STREAMS: usize = 16;
const MAX_DISPLAY_INBOX: usize = 10;
const MAX_SEEN_ROUTE_KEYS: usize = 512;

/// Lifecycle durability class of a named workstream projected to the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkstreamPersistenceKind {
    Persistent,
    OneOff,
}

impl WorkstreamPersistenceKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Persistent => "persistent",
            Self::OneOff => "one_off",
        }
    }
}

/// Execution / retention state of a named workstream projected to the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkstreamStatusKind {
    Running,
    Idle,
    Archived,
    PausedError,
}

impl WorkstreamStatusKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Idle => "idle",
            Self::Archived => "archived",
            Self::PausedError => "paused_error",
        }
    }
}

/// One workstream entry projected for `/streams`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkstreamEntryDto {
    pub name: String,
    pub scope: String,
    pub persistence: WorkstreamPersistenceKind,
    pub state: WorkstreamStatusKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_pid: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_override: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollout_ref: Option<String>,
    #[serde(default)]
    pub updated_at: String,
}

/// One unread workflow inbox digest entry projected for `/streams`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkstreamInboxEntryDto {
    pub run_id: String,
    pub workflow_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_name: Option<String>,
    pub status: String,
    pub summary: String,
    #[serde(default)]
    pub created_at: String,
}

/// Aggregated counts for the footer status bar and `/streams` header.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkstreamCountsDto {
    pub running: usize,
    pub idle: usize,
    pub paused_error: usize,
    pub archived: usize,
    pub unread_inbox: usize,
}

/// Bounded snapshot of project workstreams and unread inbox digests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkstreamOverviewDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_key: Option<String>,
    pub counts: WorkstreamCountsDto,
    #[serde(default)]
    pub streams: Vec<WorkstreamEntryDto>,
    #[serde(default)]
    pub unread_inbox: Vec<WorkstreamInboxEntryDto>,
}

/// Resolved target for `/focus <name>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkstreamFocusTarget {
    pub stream_name: String,
    pub agent_path: String,
    pub pid: Option<u64>,
}

/// Local TUI workstream inspection / management request passed to the Host adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkstreamControl {
    Inspect,
    ResolveFocus { name: String },
    Archive { name: String },
    ClearInbox,
    Resume { name: String },
}

/// Result returned by the Host workstream control adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkstreamControlResult {
    Overview(WorkstreamOverviewDto),
    FocusResolved(WorkstreamFocusTarget),
    FocusUnavailable {
        name: String,
        message: String,
    },
    ActionCompleted {
        message: String,
        overview: WorkstreamOverviewDto,
    },
}

/// Future type used to keep System Store / Host workstream details behind the CLI adapter.
pub type WorkstreamControlFuture =
    Pin<Box<dyn Future<Output = Result<WorkstreamControlResult>> + Send + 'static>>;

/// Host-local workstream operations supplied by the CLI to the TUI.
pub type WorkstreamControlHandler =
    Arc<dyn Fn(WorkstreamControl) -> WorkstreamControlFuture + Send + Sync + 'static>;

/// Client-side state for workstreams, focus target, and per-target scrollback offsets.
#[derive(Clone, Default)]
pub(in crate::file_backed) struct StreamsState {
    pub(in crate::file_backed) overview: Option<WorkstreamOverviewDto>,
    pub(in crate::file_backed) focused_stream: Option<String>,
    pub(in crate::file_backed) focused_pid: Option<u64>,
    pub(in crate::file_backed) live_stream_pids: BTreeMap<String, u64>,
    pub(in crate::file_backed) tape_offsets_by_target: BTreeMap<String, usize>,
    pub(in crate::file_backed) ui_events_seen_by_owner: BTreeMap<String, usize>,
    pub(in crate::file_backed) seen_route_keys: BTreeSet<String>,
    pub(in crate::file_backed) pending_focus_name: Option<String>,
}

impl StreamsState {
    pub(in crate::file_backed) fn apply_overview(&mut self, overview: WorkstreamOverviewDto) {
        let mut live_pids = BTreeMap::new();
        for entry in &overview.streams {
            if entry.state == WorkstreamStatusKind::Running
                && let Some(pid) = entry.child_pid
            {
                live_pids.insert(entry.name.clone(), pid);
            }
        }
        // Retain any live child PIDs discovered directly from ActionSnapshot / /agent/root/children
        // when the overview omits runtime PIDs.
        for (name, pid) in &self.live_stream_pids {
            let still_running = overview
                .streams
                .iter()
                .find(|entry| &entry.name == name)
                .is_none_or(|entry| entry.state == WorkstreamStatusKind::Running);
            if still_running {
                live_pids.entry(name.clone()).or_insert(*pid);
            }
        }
        self.live_stream_pids = live_pids;
        self.overview = Some(overview);
    }

    pub(in crate::file_backed) fn observe_stream_pid(&mut self, name: &str, pid: u64) {
        if !name.trim().is_empty() && pid > 0 {
            self.live_stream_pids.insert(name.trim().to_string(), pid);
        }
    }

    pub(in crate::file_backed) fn remove_stream_pid(&mut self, name: &str) {
        self.live_stream_pids.remove(name.trim());
    }

    #[cfg(test)]
    pub(in crate::file_backed) fn live_pid_for(&self, name: &str) -> Option<u64> {
        self.live_stream_pids.get(name).copied().or_else(|| {
            self.overview.as_ref().and_then(|ov| {
                ov.streams
                    .iter()
                    .find(|entry| {
                        entry.name == name && entry.state == WorkstreamStatusKind::Running
                    })
                    .and_then(|entry| entry.child_pid)
            })
        })
    }

    pub(in crate::file_backed) fn effective_counts(&self) -> Option<WorkstreamCountsDto> {
        if !self.has_badge() {
            return None;
        }
        let mut counts = self
            .overview
            .as_ref()
            .map(|ov| ov.counts)
            .unwrap_or_default();
        counts.running = counts.running.max(self.live_stream_pids.len());
        Some(counts)
    }

    pub(in crate::file_backed) fn has_badge(&self) -> bool {
        self.focused_stream.is_some()
            || !self.live_stream_pids.is_empty()
            || self.overview.as_ref().is_some_and(|ov| {
                ov.project_key.is_some()
                    || ov.counts.running > 0
                    || ov.counts.idle > 0
                    || ov.counts.paused_error > 0
                    || ov.counts.unread_inbox > 0
            })
    }

    #[cfg(test)]
    pub(in crate::file_backed) fn status_badge(&self, width: usize) -> Option<String> {
        let counts = self.effective_counts()?;
        let full = if counts.paused_error > 0 {
            let with_paused = format!(
                "streams: {} active · {} idle · {} paused · {} inbox",
                counts.running, counts.idle, counts.paused_error, counts.unread_inbox
            );
            if with_paused.width() <= width {
                return Some(with_paused);
            }
            format!(
                "streams: {} active · {} idle · {} inbox",
                counts.running, counts.idle, counts.unread_inbox
            )
        } else {
            format!(
                "streams: {} active · {} idle · {} inbox",
                counts.running, counts.idle, counts.unread_inbox
            )
        };
        if full.width() <= width {
            return Some(full);
        }
        let compact = format!(
            "streams: {}a · {}i · {}in",
            counts.running, counts.idle, counts.unread_inbox
        );
        if compact.width() <= width {
            return Some(compact);
        }
        let tiny = format!(
            "{}a·{}i·{}in",
            counts.running, counts.idle, counts.unread_inbox
        );
        Some(crate::history::clamp_text_width(&tiny, width.max(1)))
    }

    pub(in crate::file_backed) fn record_route_key(&mut self, key: String) -> bool {
        if self.seen_route_keys.len() >= MAX_SEEN_ROUTE_KEYS {
            let keep: Vec<String> = self
                .seen_route_keys
                .iter()
                .skip(MAX_SEEN_ROUTE_KEYS / 2)
                .cloned()
                .collect();
            self.seen_route_keys = keep.into_iter().collect();
        }
        self.seen_route_keys.insert(key)
    }

    pub(in crate::file_backed) fn focus_label(&self) -> String {
        self.focused_stream.as_deref().unwrap_or("root").to_string()
    }

    pub(in crate::file_backed) fn target_key(agent_path: &str) -> String {
        agent_path.trim_end_matches('/').to_string()
    }
}

/// Parsed `/mnt/route/log` record normalized for inline receipt rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::file_backed) struct ParsedRouteReceipt {
    pub dedup_key: String,
    pub source: String,
    pub target: String,
    pub kind: String,
    pub summary: Option<String>,
}

/// Parse one NDJSON line from `/mnt/route/log` into an inline routing receipt.
pub(in crate::file_backed) fn parse_route_log_line(raw_line: &str) -> Option<ParsedRouteReceipt> {
    let trimmed = raw_line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let record = serde_json::from_str::<Value>(trimmed).ok()?;
    let port = record.get("port").and_then(Value::as_str).unwrap_or("");
    let rule = record.get("rule").and_then(Value::as_str).unwrap_or("");
    let reason = record
        .get("reason")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let message = record.get("message")?;

    let embedded_content = message
        .get("content")
        .and_then(Value::as_str)
        .and_then(|s| serde_json::from_str::<Value>(s).ok());
    let handoff = embedded_content.as_ref().unwrap_or(message);

    let msg_type = message.get("type").and_then(Value::as_str).unwrap_or("");
    let raw_kind = handoff
        .get("kind")
        .and_then(Value::as_str)
        .or_else(|| message.get("kind").and_then(Value::as_str))
        .unwrap_or(msg_type);
    let hop_count = handoff
        .get("hop_count")
        .and_then(Value::as_u64)
        .or_else(|| message.get("hop_count").and_then(Value::as_u64))
        .unwrap_or(0);

    let is_handoff = matches!(
        raw_kind,
        "reroute_request" | "cross_stream_dependency" | "handoff_result" | "reroute"
    ) || handoff.get("source_stream").is_some()
        || port == "dead-letter";
    if !is_handoff {
        return None;
    }

    let source = handoff
        .get("source_stream")
        .and_then(Value::as_str)
        .or_else(|| message.get("source_stream").and_then(Value::as_str))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("root")
        .to_string();
    let target = handoff
        .get("suggested_target")
        .and_then(Value::as_str)
        .or_else(|| handoff.get("target_stream").and_then(Value::as_str))
        .or_else(|| message.get("suggested_target").and_then(Value::as_str))
        .or_else(|| message.get("target_stream").and_then(Value::as_str))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if port.is_empty() || port == "dead-letter" {
                "root"
            } else {
                port
            }
        })
        .to_string();

    let kind = if port == "dead-letter" || hop_count > 2 {
        "dead_letter".to_string()
    } else if raw_kind.is_empty() {
        "reroute_request".to_string()
    } else {
        raw_kind.to_string()
    };

    let correlation_id = handoff
        .get("correlation_id")
        .and_then(Value::as_str)
        .or_else(|| message.get("correlation_id").and_then(Value::as_str))
        .unwrap_or("");
    let dedup_key = if correlation_id.is_empty() {
        format!("{kind}:{source}:{target}:{port}:{rule}:{trimmed}")
    } else {
        format!("{correlation_id}:{kind}:{source}:{target}:{port}")
    };

    let summary = if kind == "dead_letter" {
        Some(
            reason
                .filter(|r| r != "no matching rule")
                .unwrap_or_else(|| format!("hop limit ({hop_count}) escalated to root")),
        )
    } else {
        None
    };

    Some(ParsedRouteReceipt {
        dedup_key,
        source,
        target,
        kind,
        summary,
    })
}

/// Format `/streams` inline transcript lines wrapped cleanly to `width`.
pub(in crate::file_backed) fn format_streams_overview_lines(
    overview: &WorkstreamOverviewDto,
    focused_stream: Option<&str>,
    theme: Theme,
    width: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let counts = overview.counts;
    let header = format!(
        "Workstreams · {} active · {} idle · {} archived · {} inbox",
        counts.running, counts.idle, counts.archived, counts.unread_inbox
    );
    lines.push(Line::styled(clean_text(&header), theme.accent_bold()));

    let focus_display = focused_stream.unwrap_or("root");
    lines.push(Line::styled(
        clean_text(&format!("Focused: [{focus_display}]")),
        theme.muted_style(),
    ));

    let non_archived: Vec<&WorkstreamEntryDto> = overview
        .streams
        .iter()
        .filter(|entry| entry.state != WorkstreamStatusKind::Archived)
        .collect();
    if non_archived.is_empty() {
        lines.push(Line::styled(
            "  No active or idle workstreams.",
            theme.muted_style(),
        ));
    } else {
        for entry in non_archived.iter().take(MAX_DISPLAY_STREAMS) {
            let is_focused = focused_stream == Some(entry.name.as_str());
            let marker = if is_focused {
                if theme.is_dumb { "> " } else { "▶ " }
            } else {
                "  "
            };
            let pid_suffix = entry
                .child_pid
                .filter(|pid| *pid > 0 && entry.state == WorkstreamStatusKind::Running)
                .map(|pid| format!(" · pid {pid}"))
                .unwrap_or_default();
            let scope = clean_text(&entry.scope);
            let scope_suffix = if scope.is_empty() {
                String::new()
            } else {
                format!(" — {scope}")
            };
            let name_style = if is_focused {
                theme.accent_bold()
            } else if entry.state == WorkstreamStatusKind::PausedError {
                theme.wait_style()
            } else {
                theme.accent_style()
            };
            lines.push(Line::from(vec![
                Span::styled(marker, name_style),
                Span::styled(clean_text(&entry.name), name_style),
                Span::styled(
                    format!(
                        "  {} · {}{}{scope_suffix}",
                        entry.state.label(),
                        entry.persistence.label(),
                        pid_suffix
                    ),
                    theme.muted_style(),
                ),
            ]));
        }
        if non_archived.len() > MAX_DISPLAY_STREAMS {
            lines.push(Line::styled(
                format!(
                    "  +{} more workstreams",
                    non_archived.len() - MAX_DISPLAY_STREAMS
                ),
                theme.muted_style(),
            ));
        }
    }

    if counts.archived > 0 {
        lines.push(Line::styled(
            format!("  Archived: {} collapsed", counts.archived),
            theme.muted_style(),
        ));
    }

    if !overview.unread_inbox.is_empty() {
        lines.push(Line::styled(
            format!("Inbox ({} unread):", overview.unread_inbox.len()),
            theme.accent_style(),
        ));
        for entry in overview.unread_inbox.iter().take(MAX_DISPLAY_INBOX) {
            let stream_suffix = entry
                .stream_name
                .as_deref()
                .map(|name| format!(" ({name})"))
                .unwrap_or_default();
            let summary = crate::history::clamp_text_width(&clean_text(&entry.summary), 96);
            lines.push(Line::styled(
                clean_text(&format!(
                    "  [{}] {}{stream_suffix}: {summary}",
                    entry.status, entry.workflow_id
                )),
                theme.muted_style(),
            ));
        }
        if overview.unread_inbox.len() > MAX_DISPLAY_INBOX {
            lines.push(Line::styled(
                format!(
                    "  +{} more unread inbox items",
                    overview.unread_inbox.len() - MAX_DISPLAY_INBOX
                ),
                theme.muted_style(),
            ));
        }
    }

    lines.push(Line::styled(
        "  actions: /focus <name> · /root · /streams archive <name> · /streams clear-inbox · /streams resume <name>",
        metadata_style(theme),
    ));

    wrap_styled_lines(lines, width.max(16))
}

/// Render the single-row footer below the composer, preserving exact legacy hints
/// when no workstream/inbox badge is active and adding `streams: X active · Y idle · Z inbox`
/// cleanly across wide and narrow terminal widths.
#[cfg(test)]
pub(in crate::file_backed) fn footer_status_line(
    app: &FileBackedApp,
    width: usize,
) -> Option<Line<'static>> {
    if app.completion.is_some()
        || app.project_selection.is_some()
        || app.pending_yield.is_some()
        || app.login.is_entering_input()
    {
        return None;
    }

    let running = matches!(app.activity.state, super::UiActivityState::Running);
    let has_details = !app.projected_actions.is_empty() || !app.plan_owners.is_empty();
    let badge = app.streams.status_badge(width);

    if running {
        let hint = if width >= 52 {
            "Enter queue next · Ctrl+S steer now · Esc interrupt"
        } else if width >= 38 {
            "Enter queue · Ctrl+S steer · Esc stop"
        } else {
            "Ctrl+S steer · Esc stop"
        };
        let combined = match badge {
            Some(badge) => {
                let with_full_hint = format!("{hint} · {badge}");
                if with_full_hint.width() <= width {
                    with_full_hint
                } else {
                    let short = format!("Esc stop · {badge}");
                    if short.width() <= width {
                        short
                    } else {
                        hint.to_string()
                    }
                }
            }
            None => hint.to_string(),
        };
        return Some(Line::styled(
            crate::history::clamp_text_width(&combined, width),
            app.theme.muted_style(),
        ));
    }

    if has_details {
        let hint = if width >= 40 {
            "Ctrl+O details (p: plans, t: tape)"
        } else {
            "Ctrl+O details (p: plans)"
        };
        let combined = match badge {
            Some(badge) => {
                let with_hint = format!("{badge} · {hint}");
                if with_hint.width() <= width {
                    with_hint
                } else {
                    badge
                }
            }
            None => hint.to_string(),
        };
        return Some(Line::styled(
            crate::history::clamp_text_width(&combined, width),
            app.theme.muted_style(),
        ));
    }

    badge.map(|text| {
        Line::styled(
            crate::history::clamp_text_width(&text, width),
            app.theme.muted_style(),
        )
    })
}

#[cfg(test)]
#[path = "streams_tests.rs"]
mod tests;
