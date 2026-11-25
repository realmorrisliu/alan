//! Conversational `WorkflowManifest` drafting helper and explicit user confirmation gate
//! before installing native `launchd` or `crontab` entries.

use std::fmt::Write as _;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::manifest::{
    SUPPORTED_MANIFEST_VERSION, WorkflowCronStreamSpec, WorkflowManifest, WorkflowStore,
    WorkflowTrustPosture, validate_mount_grant_id, validate_workflow_id,
};
use super::schedule::{
    ScheduleTarget, install_launchd_plist, parse_schedule, render_cron_entry, render_launchd_plist,
};

/// Conversational drafting inputs derived from a natural-language request.
#[derive(Debug, Clone)]
pub struct WorkflowDraftRequest {
    pub description: String,
    pub id: Option<String>,
    pub project_key: Option<String>,
    pub mounts: Vec<String>,
    pub stream_name: Option<String>,
}

/// Reviewed workflow draft presented to the user before scheduler installation.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowDraftSummary {
    pub manifest: WorkflowManifest,
    pub scheduler_target: ScheduleTarget,
    pub rendered_schedule_entry: String,
    pub confirmation_digest: String,
    pub human_review_prompt: String,
}

/// Outcome recorded only after explicit user confirmation installs a scheduled workflow.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ScheduleInstallOutcome {
    pub workflow_id: String,
    pub scheduler_target: ScheduleTarget,
    pub installed_location: String,
    pub confirmation_digest: String,
}

/// Abstraction over `crontab` installation so tests can assert gating without mutating host cron.
pub trait CrontabInstaller {
    fn install_entry(&self, workflow_id: &str, entry: &str) -> Result<String>;
}

/// Real POSIX `crontab` installer preserving unrelated existing crontab entries.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemCrontabInstaller;

impl CrontabInstaller for SystemCrontabInstaller {
    fn install_entry(&self, workflow_id: &str, entry: &str) -> Result<String> {
        let existing = match Command::new("crontab").arg("-l").output() {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).into_owned()
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.to_ascii_lowercase().contains("no crontab for") {
                    String::new()
                } else {
                    bail!("failed to read existing crontab: {}", stderr.trim());
                }
            }
            Err(err) => {
                return Err(err).context("failed to invoke `crontab -l`");
            }
        };

        let merged = merge_crontab_entry(&existing, workflow_id, entry);
        let mut child = Command::new("crontab")
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to spawn `crontab -`")?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(merged.as_bytes())
                .context("failed to write crontab payload")?;
        }
        let output = child.wait_with_output()?;
        ensure!(
            output.status.success(),
            "`crontab -` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        Ok(format!("user crontab ({workflow_id})"))
    }
}

/// Merge an Alan-managed workflow block into an existing crontab without disturbing other lines.
pub(crate) fn merge_crontab_entry(existing: &str, workflow_id: &str, entry: &str) -> String {
    let marker = format!("# alan workflow: {workflow_id}");
    let mut kept_lines = Vec::new();
    let mut skip_next = false;
    for line in existing.lines() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if line.trim() == marker {
            skip_next = true;
            continue;
        }
        kept_lines.push(line);
    }
    let mut out = kept_lines.join("\n");
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(entry);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Draft a validated `WorkflowManifest` from a natural-language description, without installing anything.
pub fn draft_workflow_from_natural_language(
    req: &WorkflowDraftRequest,
    store: &WorkflowStore,
    alan_bin: &Path,
    scheduler_target: ScheduleTarget,
) -> Result<WorkflowDraftSummary> {
    let desc = req.description.trim();
    ensure!(
        !desc.is_empty(),
        "workflow description must not be empty when drafting"
    );

    let id = match req.id.as_deref() {
        Some(explicit) => {
            let trimmed = explicit.trim().to_string();
            validate_workflow_id(&trimmed)?;
            trimmed
        }
        None => slugify_workflow_id(desc),
    };

    let schedule = infer_schedule_from_text(desc)?;
    parse_schedule(&schedule)?;

    let mut mounts = req.mounts.clone();
    for extracted in extract_named_grants(desc) {
        if !mounts.contains(&extracted) {
            mounts.push(extracted);
        }
    }
    for mount in &mounts {
        validate_mount_grant_id(mount)?;
    }

    let inferred_stream = req
        .stream_name
        .as_deref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| extract_stream_name(desc));

    let project_key = req
        .project_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let cron_stream = match inferred_stream {
        Some(stream_name) => {
            ensure!(
                project_key.is_some(),
                "drafted stateful cron stream `{stream_name}` requires `--project-key`"
            );
            validate_workflow_id(&stream_name)?;
            Some(WorkflowCronStreamSpec {
                stream_name,
                scope: Some(truncate_scope(desc)),
                max_consecutive_failures: extract_max_failures(desc).unwrap_or(3),
                compaction_threshold_tokens: extract_compaction_threshold(desc),
                per_run_budget_usd: extract_budget_usd(desc),
            })
        }
        None => None,
    };

    let manifest = WorkflowManifest {
        schema_version: SUPPORTED_MANIFEST_VERSION,
        id,
        schedule,
        prompt: desc.to_string(),
        model_profile: None,
        project_key,
        mounts,
        packages: Vec::new(),
        fileservers: Vec::new(),
        trust: WorkflowTrustPosture::Governed,
        json: true,
        timeout_secs: Some(300),
        trigger_allowlist: None,
        release_gate: None,
        cron_stream,
    };
    manifest.validate()?;

    let rendered_schedule_entry = match scheduler_target {
        ScheduleTarget::Launchd => render_launchd_plist(&manifest, alan_bin, store)?,
        ScheduleTarget::Cron => render_cron_entry(&manifest, alan_bin)?,
    };
    let confirmation_digest =
        compute_confirmation_digest(&manifest, scheduler_target, &rendered_schedule_entry)?;
    let human_review_prompt = format_human_review_prompt(
        &manifest,
        scheduler_target,
        &rendered_schedule_entry,
        &confirmation_digest,
    );

    Ok(WorkflowDraftSummary {
        manifest,
        scheduler_target,
        rendered_schedule_entry,
        confirmation_digest,
        human_review_prompt,
    })
}

/// Enforce an explicit user confirmation + digest match before saving a drafted workflow
/// or installing any `launchd` `.plist` / `crontab` entry.
pub fn install_drafted_workflow_with_confirmation(
    draft: &WorkflowDraftSummary,
    confirmed: bool,
    expected_digest: Option<&str>,
    store: &WorkflowStore,
    home_dir: Option<&Path>,
    crontab_installer: &dyn CrontabInstaller,
    force: bool,
) -> Result<ScheduleInstallOutcome> {
    ensure!(
        confirmed,
        "confirmation gate refused scheduler installation: review the drafted workflow and re-run with explicit user confirmation (--confirm --confirm-digest {})",
        draft.confirmation_digest
    );
    let supplied = expected_digest
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .with_context(|| {
            format!(
                "confirmation gate requires `--confirm-digest {}` matching the reviewed draft",
                draft.confirmation_digest
            )
        })?;
    ensure!(
        supplied == draft.confirmation_digest,
        "confirmation digest mismatch (expected {}, got {supplied}); draft changed since review",
        draft.confirmation_digest
    );

    draft.manifest.validate()?;
    store.save_manifest(&draft.manifest, force)?;

    let installed_location = match draft.scheduler_target {
        ScheduleTarget::Launchd => {
            let home = home_dir
                .context("cannot determine home directory for LaunchAgents installation")?;
            let path =
                install_launchd_plist(&draft.manifest.id, &draft.rendered_schedule_entry, home)?;
            path.display().to_string()
        }
        ScheduleTarget::Cron => {
            crontab_installer.install_entry(&draft.manifest.id, &draft.rendered_schedule_entry)?
        }
    };

    Ok(ScheduleInstallOutcome {
        workflow_id: draft.manifest.id.clone(),
        scheduler_target: draft.scheduler_target,
        installed_location,
        confirmation_digest: draft.confirmation_digest.clone(),
    })
}

fn compute_confirmation_digest(
    manifest: &WorkflowManifest,
    target: ScheduleTarget,
    rendered_entry: &str,
) -> Result<String> {
    let manifest_toml = toml::to_string(manifest)?;
    let mut hasher = Sha256::new();
    hasher.update(format!("{target:?}\n").as_bytes());
    hasher.update(manifest_toml.as_bytes());
    hasher.update(b"\n---\n");
    hasher.update(rendered_entry.as_bytes());
    let mut out = String::with_capacity(64);
    for byte in hasher.finalize() {
        let _ = write!(out, "{byte:02x}");
    }
    Ok(out)
}

fn format_human_review_prompt(
    manifest: &WorkflowManifest,
    target: ScheduleTarget,
    rendered_entry: &str,
    digest: &str,
) -> String {
    let target_name = match target {
        ScheduleTarget::Launchd => "launchd",
        ScheduleTarget::Cron => "cron",
    };
    let stream_line = match &manifest.cron_stream {
        Some(cs) => format!(
            "stream={} max_failures={} compact_tokens={} budget_usd={}",
            cs.stream_name,
            cs.max_consecutive_failures,
            cs.compaction_threshold_tokens
                .map(|v| v.to_string())
                .unwrap_or_else(|| "none".to_string()),
            cs.per_run_budget_usd
                .map(|v| format!("{v:.2}"))
                .unwrap_or_else(|| "none".to_string()),
        ),
        None => "ephemeral (stateless)".to_string(),
    };
    format!(
        "Drafted workflow `{id}` (target: {target_name}, trust: {trust:?})\n\
         Schedule: {schedule}\n\
         Mounts: [{mounts}]\n\
         Mode: {stream_line}\n\
         Confirmation digest: {digest}\n\
         --- Scheduled Entry Preview ---\n\
         {rendered_entry}\n\
         Confirm explicitly before installing with `--confirm --confirm-digest {digest}`.",
        id = manifest.id,
        trust = manifest.trust,
        schedule = manifest.schedule,
        mounts = manifest.mounts.join(", "),
    )
}

fn slugify_workflow_id(text: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = true;
    for ch in text.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
        if out.len() >= 36 {
            break;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "draft-workflow".to_string()
    } else {
        trimmed
    }
}

fn truncate_scope(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= 200 {
        trimmed.to_string()
    } else {
        format!("{}...", trimmed.chars().take(197).collect::<String>())
    }
}

fn tokenize_words(desc: &str) -> Vec<String> {
    desc.split_whitespace()
        .map(|w| w.trim_matches(|c: char| matches!(c, ',' | '.' | ';' | '(' | ')' | '"')))
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

fn parse_interval_token(qty_token: &str, unit_token: Option<&str>) -> Option<String> {
    if let Ok(qty) = qty_token.parse::<u64>()
        && let Some(unit_raw) = unit_token
    {
        let unit = match unit_raw.to_ascii_lowercase().as_str() {
            "s" | "sec" | "secs" | "second" | "seconds" => Some("s"),
            "m" | "min" | "mins" | "minute" | "minutes" => Some("m"),
            "h" | "hr" | "hrs" | "hour" | "hours" => Some("h"),
            "d" | "day" | "days" => Some("d"),
            _ => None,
        }?;
        return Some(format!("every {qty}{unit}"));
    }
    let lower = qty_token.to_ascii_lowercase();
    let digit_end = lower.find(|c: char| !c.is_ascii_digit())?;
    if digit_end == 0 {
        return None;
    }
    let qty = lower[..digit_end].parse::<u64>().ok()?;
    let unit = match &lower[digit_end..] {
        "s" | "sec" | "secs" => Some("s"),
        "m" | "min" | "mins" => Some("m"),
        "h" | "hr" | "hrs" => Some("h"),
        "d" | "day" | "days" => Some("d"),
        _ => None,
    }?;
    Some(format!("every {qty}{unit}"))
}

fn parse_time_of_day_token(token: &str, next: Option<&str>) -> Option<(u32, u32)> {
    let lower = token.to_ascii_lowercase();
    let (core, inline_meridiem) = if let Some(s) = lower.strip_suffix("am") {
        (s, Some("am"))
    } else if let Some(s) = lower.strip_suffix("pm") {
        (s, Some("pm"))
    } else {
        (lower.as_str(), None)
    };
    let (h_str, m_str) = match core.split_once(':') {
        Some((h, m)) => (h, Some(m)),
        None => (core, None),
    };
    let mut hr: u32 = h_str.parse().ok()?;
    let min: u32 = match m_str {
        Some(m) => m.parse().ok()?,
        None => 0,
    };
    if hr > 23 || min > 59 {
        return None;
    }
    let meridiem = inline_meridiem.or_else(|| {
        next.and_then(|n| match n.to_ascii_lowercase().as_str() {
            "am" => Some("am"),
            "pm" => Some("pm"),
            _ => None,
        })
    });
    if let Some(m) = meridiem {
        if m == "pm" && hr < 12 {
            hr += 12;
        } else if m == "am" && hr == 12 {
            hr = 0;
        }
    }
    Some((hr.min(23), min.min(59)))
}

fn infer_schedule_from_text(desc: &str) -> Result<String> {
    let words = tokenize_words(desc);
    let lower = desc.to_ascii_lowercase();

    for i in 0..words.len() {
        if words[i].eq_ignore_ascii_case("every")
            && let Some(qty) = words.get(i + 1)
            && let Some(spec) = parse_interval_token(qty, words.get(i + 2).map(String::as_str))
        {
            return Ok(spec);
        }
    }

    let mut explicit_hm = None;
    for i in 0..words.len() {
        if words[i].eq_ignore_ascii_case("at")
            && let Some(tok) = words.get(i + 1)
            && let Some(hm) = parse_time_of_day_token(tok, words.get(i + 2).map(String::as_str))
        {
            explicit_hm = Some(hm);
            break;
        }
    }

    let (hour, minute) = explicit_hm.unwrap_or_else(|| {
        if lower.contains("nightly") || lower.contains("midnight") {
            (0, 0)
        } else {
            (9, 0)
        }
    });

    if lower.contains("weekday") || lower.contains("weekdays") {
        return Ok(format!("{minute} {hour} * * 1-5"));
    }
    if lower.contains("daily")
        || lower.contains("nightly")
        || lower.contains("every day")
        || explicit_hm.is_some()
    {
        return Ok(format!("{minute} {hour} * * *"));
    }
    Ok("every 1h".to_string())
}

fn extract_named_grants(desc: &str) -> Vec<String> {
    let words = tokenize_words(desc);
    let mut out = Vec::new();
    for pair in words.windows(2) {
        if pair[0].eq_ignore_ascii_case("grant") {
            let candidate = pair[1].as_str();
            if validate_mount_grant_id(candidate).is_ok() {
                out.push(candidate.to_string());
            }
        }
    }
    out
}

fn extract_stream_name(desc: &str) -> Option<String> {
    let words = tokenize_words(desc);
    for pair in words.windows(2) {
        if pair[0].eq_ignore_ascii_case("stream") {
            let candidate = pair[1].to_ascii_lowercase();
            if validate_workflow_id(&candidate).is_ok() {
                return Some(candidate);
            }
        }
    }
    None
}

fn extract_max_failures(desc: &str) -> Option<u32> {
    let words = tokenize_words(desc);
    for i in 0..words.len() {
        if (words[i].eq_ignore_ascii_case("max") || words[i].eq_ignore_ascii_case("after"))
            && let Some(num_str) = words.get(i + 1)
            && let Ok(num) = num_str.parse::<u32>()
            && num >= 1
        {
            let next1 = words.get(i + 2).map(String::as_str).unwrap_or("");
            let next2 = words.get(i + 3).map(String::as_str).unwrap_or("");
            if next1.to_ascii_lowercase().starts_with("failure")
                || (next1.eq_ignore_ascii_case("consecutive")
                    && next2.to_ascii_lowercase().starts_with("failure"))
            {
                return Some(num);
            }
        }
    }
    None
}

fn extract_compaction_threshold(desc: &str) -> Option<u64> {
    let words = tokenize_words(desc);
    for i in 0..words.len() {
        if words[i].to_ascii_lowercase().starts_with("compact") {
            for offset in 1..=3 {
                if let Some(num_str) = words.get(i + offset)
                    && let Ok(num) = num_str.parse::<u64>()
                    && num >= 1
                    && words
                        .get(i + offset + 1)
                        .is_some_and(|w| w.to_ascii_lowercase().starts_with("token"))
                {
                    return Some(num);
                }
            }
        }
    }
    None
}

fn extract_budget_usd(desc: &str) -> Option<f64> {
    let words = tokenize_words(desc);
    for pair in words.windows(2) {
        if pair[0].eq_ignore_ascii_case("budget") {
            let trimmed = pair[1].trim_start_matches('$');
            if let Ok(val) = trimmed.parse::<f64>()
                && val.is_finite()
                && val > 0.0
            {
                return Some(val);
            }
        }
    }
    None
}
