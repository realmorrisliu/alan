//! Schedule parsing (`cron` and `interval`) plus `launchd` `.plist` and `crontab` rendering.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};

use super::manifest::{WorkflowManifest, WorkflowStore, atomic_write_file, ensure_real_dir};

const MAX_LAUNCHD_CALENDAR_INTERVALS: usize = 128;
const MAX_INTERVAL_SECS: u64 = 365 * 86_400;

/// Supported native scheduler targets for `alan workflow render-schedule`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleTarget {
    /// macOS `launchd` LaunchAgent property list (`.plist`).
    Launchd,
    /// Standard POSIX `crontab` line.
    Cron,
}

/// Parsed and validated workflow schedule specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleSpec {
    /// Standard 5-field cron expression (`min hour dom month dow`).
    Cron { raw: String, fields: [CronField; 5] },
    /// Fixed positive interval in seconds.
    Interval { raw: String, seconds: u64 },
}

/// Parsed cron field expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CronField {
    /// Wildcard `*` (no explicit constraint).
    Any,
    /// Discrete sorted allowed values within `[min, max]`.
    Values(Vec<u16>),
}

/// Parse and validate a workflow `schedule` string (5-field cron, `@daily`/etc., or interval).
pub fn parse_schedule(raw: &str) -> Result<ScheduleSpec> {
    let trimmed = raw.trim();
    ensure!(!trimmed.is_empty(), "workflow schedule must not be empty");
    ensure!(
        !trimmed.contains(['\n', '\r', '\0']),
        "workflow schedule contains invalid control characters"
    );

    if let Some(expanded) = expand_cron_alias(trimmed) {
        return parse_cron_fields(trimmed, expanded);
    }

    if let Some(seconds) = try_parse_interval(trimmed)? {
        return Ok(ScheduleSpec::Interval {
            raw: trimmed.to_string(),
            seconds,
        });
    }

    parse_cron_fields(trimmed, trimmed)
}

fn expand_cron_alias(raw: &str) -> Option<&'static str> {
    match raw.to_ascii_lowercase().as_str() {
        "@hourly" => Some("0 * * * *"),
        "@daily" | "@midnight" => Some("0 0 * * *"),
        "@weekly" => Some("0 0 * * 0"),
        "@monthly" => Some("0 0 1 * *"),
        _ => None,
    }
}

fn try_parse_interval(raw: &str) -> Result<Option<u64>> {
    let lower = raw.trim().to_ascii_lowercase();
    let candidate = lower
        .strip_prefix("every ")
        .map(str::trim)
        .unwrap_or(lower.as_str());

    let Some(unit) = candidate.chars().last() else {
        return Ok(None);
    };
    if !matches!(unit, 's' | 'm' | 'h' | 'd') {
        return Ok(None);
    }
    let digits = &candidate[..candidate.len() - 1];
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(None);
    }
    let count: u64 = digits
        .parse()
        .with_context(|| format!("invalid interval count in schedule `{raw}`"))?;
    let multiplier = match unit {
        's' => 1,
        'm' => 60,
        'h' => 3600,
        'd' => 86_400,
        _ => unreachable!(),
    };
    let seconds = count
        .checked_mul(multiplier)
        .context("workflow interval overflows u64")?;
    ensure!(
        (1..=MAX_INTERVAL_SECS).contains(&seconds),
        "workflow interval `{raw}` must be between 1s and 365d"
    );
    Ok(Some(seconds))
}

fn parse_cron_fields(original: &str, expr: &str) -> Result<ScheduleSpec> {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    ensure!(
        parts.len() == 5,
        "invalid workflow schedule `{original}`: expected 5-field cron (`min hour dom month dow`) or interval (`30m`, `every 1h`)"
    );
    let fields = [
        parse_cron_field(parts[0], 0, 59, "minute")?,
        parse_cron_field(parts[1], 0, 23, "hour")?,
        parse_cron_field(parts[2], 1, 31, "day of month")?,
        parse_cron_field(parts[3], 1, 12, "month")?,
        parse_cron_field(parts[4], 0, 7, "day of week")?,
    ];
    Ok(ScheduleSpec::Cron {
        raw: parts.join(" "),
        fields,
    })
}

fn parse_cron_field(token: &str, min: u16, max: u16, label: &str) -> Result<CronField> {
    if token == "*" {
        return Ok(CronField::Any);
    }
    let mut values = BTreeSet::new();
    for part in token.split(',') {
        ensure!(!part.is_empty(), "empty segment in cron {label} `{token}`");
        let (range_part, step) = match part.split_once('/') {
            Some((left, step_raw)) => {
                let step: u16 = step_raw
                    .parse()
                    .with_context(|| format!("invalid cron step `{step_raw}` in {label}"))?;
                ensure!(step > 0, "cron step in {label} must be > 0");
                (left, step)
            }
            None => (part, 1),
        };
        let (start, end) = if range_part == "*" {
            (min, max)
        } else if let Some((lo_raw, hi_raw)) = range_part.split_once('-') {
            let lo: u16 = lo_raw
                .parse()
                .with_context(|| format!("invalid cron value `{lo_raw}` in {label}"))?;
            let hi: u16 = hi_raw
                .parse()
                .with_context(|| format!("invalid cron value `{hi_raw}` in {label}"))?;
            ensure!(
                lo <= hi && lo >= min && hi <= max,
                "cron range `{range_part}` out of bounds ({min}..={max}) in {label}"
            );
            (lo, hi)
        } else {
            let val: u16 = range_part
                .parse()
                .with_context(|| format!("invalid cron value `{range_part}` in {label}"))?;
            ensure!(
                (min..=max).contains(&val),
                "cron value `{val}` out of bounds ({min}..={max}) in {label}"
            );
            (val, val)
        };

        let mut cursor = start;
        while cursor <= end {
            values.insert(cursor);
            cursor = match cursor.checked_add(step) {
                Some(next) => next,
                None => break,
            };
        }
    }
    ensure!(
        !values.is_empty(),
        "cron {label} `{token}` matched no values"
    );
    Ok(CronField::Values(values.into_iter().collect()))
}

/// Render a macOS `launchd` LaunchAgent `.plist` invoking `alan workflow run <id>`.
pub fn render_launchd_plist(
    manifest: &WorkflowManifest,
    alan_bin: &Path,
    store: &WorkflowStore,
) -> Result<String> {
    manifest.validate()?;
    let spec = parse_schedule(&manifest.schedule)?;
    let label = launchd_label(&manifest.id);
    let runs_dir = store.workflow_runs_dir(&manifest.id)?;
    let stdout_log = runs_dir.join("launchd.stdout.log");
    let stderr_log = runs_dir.join("launchd.stderr.log");
    let bin_str = alan_bin
        .to_str()
        .context("alan executable path is not valid UTF-8")?;

    let schedule_xml = match spec {
        ScheduleSpec::Interval { seconds, .. } => {
            format!("  <key>StartInterval</key>\n  <integer>{seconds}</integer>\n")
        }
        ScheduleSpec::Cron { fields, .. } => render_launchd_calendar_xml(&fields)?,
    };

    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \x20 <key>Label</key>\n\
         \x20 <string>{label}</string>\n\
         \x20 <key>ProgramArguments</key>\n\
         \x20 <array>\n\
         \x20   <string>{bin_xml}</string>\n\
         \x20   <string>workflow</string>\n\
         \x20   <string>run</string>\n\
         \x20   <string>{id_xml}</string>\n\
         \x20 </array>\n\
         {schedule_xml}\
         \x20 <key>RunAtLoad</key>\n\
         \x20 <false/>\n\
         \x20 <key>ProcessType</key>\n\
         \x20 <string>Background</string>\n\
         \x20 <key>StandardOutPath</key>\n\
         \x20 <string>{stdout_xml}</string>\n\
         \x20 <key>StandardErrorPath</key>\n\
         \x20 <string>{stderr_xml}</string>\n\
         </dict>\n\
         </plist>\n",
        label = xml_escape(&label),
        bin_xml = xml_escape(bin_str),
        id_xml = xml_escape(&manifest.id),
        stdout_xml = xml_escape(&stdout_log.display().to_string()),
        stderr_xml = xml_escape(&stderr_log.display().to_string()),
    ))
}

fn render_launchd_calendar_xml(fields: &[CronField; 5]) -> Result<String> {
    const KEYS: [&str; 5] = ["Minute", "Hour", "Day", "Month", "Weekday"];

    let mut options_per_field: [Vec<Option<u16>>; 5] = Default::default();
    for (idx, field) in fields.iter().enumerate() {
        options_per_field[idx] = match field {
            CronField::Any => vec![None],
            CronField::Values(vals) => {
                let mut deduped = BTreeSet::new();
                for &v in vals {
                    // Normalize cron weekday 7 (Sunday) -> 0 for launchd Weekday.
                    let normalized = if idx == 4 && v == 7 { 0 } else { v };
                    deduped.insert(normalized);
                }
                deduped.into_iter().map(Some).collect()
            }
        };
    }

    let total: usize = options_per_field
        .iter()
        .try_fold(1usize, |acc, v| acc.checked_mul(v.len()))
        .unwrap_or(usize::MAX);
    ensure!(
        total <= MAX_LAUNCHD_CALENDAR_INTERVALS,
        "cron schedule expands to {total} launchd calendar entries (limit {MAX_LAUNCHD_CALENDAR_INTERVALS}); simplify the cron expression or use an interval schedule"
    );

    let mut dicts: Vec<[Option<u16>; 5]> = vec![[None; 5]];
    for (idx, choices) in options_per_field.iter().enumerate() {
        let mut next = Vec::with_capacity(dicts.len() * choices.len());
        for existing in &dicts {
            for &choice in choices {
                let mut entry = *existing;
                entry[idx] = choice;
                next.push(entry);
            }
        }
        dicts = next;
    }

    let render_dict = |entry: &[Option<u16>; 5], indent: &str| -> String {
        let pairs: Vec<String> = entry
            .iter()
            .enumerate()
            .filter_map(|(idx, val)| {
                val.map(|v| {
                    format!(
                        "{indent}  <key>{}</key>\n{indent}  <integer>{v}</integer>",
                        KEYS[idx]
                    )
                })
            })
            .collect();
        if pairs.is_empty() {
            format!("{indent}<dict/>\n")
        } else {
            format!("{indent}<dict>\n{}\n{indent}</dict>\n", pairs.join("\n"))
        }
    };

    if dicts.len() == 1 {
        Ok(format!(
            "  <key>StartCalendarInterval</key>\n{}",
            render_dict(&dicts[0], "  ")
        ))
    } else {
        let mut out = String::from("  <key>StartCalendarInterval</key>\n  <array>\n");
        for entry in &dicts {
            out.push_str(&render_dict(entry, "    "));
        }
        out.push_str("  </array>\n");
        Ok(out)
    }
}

/// Render a ready-to-install POSIX `crontab` entry invoking `alan workflow run <id>`.
pub fn render_cron_entry(manifest: &WorkflowManifest, alan_bin: &Path) -> Result<String> {
    manifest.validate()?;
    let spec = parse_schedule(&manifest.schedule)?;
    let cron_expr = match spec {
        ScheduleSpec::Cron { raw, .. } => raw,
        ScheduleSpec::Interval { raw, seconds } => interval_to_cron(&raw, seconds)?,
    };
    let bin_str = alan_bin
        .to_str()
        .context("alan executable path is not valid UTF-8")?;
    Ok(format!(
        "# alan workflow: {}\n{} {} workflow run {}\n",
        manifest.id,
        cron_expr,
        shell_quote(bin_str),
        shell_quote(&manifest.id)
    ))
}

fn interval_to_cron(raw: &str, seconds: u64) -> Result<String> {
    if seconds == 60 {
        return Ok("* * * * *".to_string());
    }
    if seconds < 60 || !seconds.is_multiple_of(60) {
        bail!(
            "interval `{raw}` cannot be represented in 1-minute POSIX cron; use a 5-field cron expression or `--target launchd`"
        );
    }
    let minutes = seconds / 60;
    if minutes < 60 && 60u64.is_multiple_of(minutes) {
        return Ok(format!("*/{minutes} * * * *"));
    }
    if seconds.is_multiple_of(3600) {
        let hours = seconds / 3600;
        if hours == 1 {
            return Ok("0 * * * *".to_string());
        }
        if hours < 24 && 24u64.is_multiple_of(hours) {
            return Ok(format!("0 */{hours} * * *"));
        }
        if seconds.is_multiple_of(86_400) {
            let days = seconds / 86_400;
            if days == 1 {
                return Ok("0 0 * * *".to_string());
            }
            if days <= 28 {
                return Ok(format!("0 0 */{days} * *"));
            }
        }
    }
    bail!(
        "interval `{raw}` is not evenly representable in standard crontab steps; use a 5-field cron expression or `--target launchd`"
    )
}

/// Install a `launchd` LaunchAgent `.plist` into `<home>/Library/LaunchAgents/`.
pub fn install_launchd_plist(id: &str, plist: &str, home_dir: &Path) -> Result<PathBuf> {
    let launch_agents = home_dir.join("Library").join("LaunchAgents");
    ensure_real_dir(&launch_agents)?;
    let target = launch_agents.join(format!("{}.plist", launchd_label(id)));
    atomic_write_file(&target, plist.as_bytes())?;
    Ok(target)
}

/// Canonical `launchd` label for a workflow id.
pub fn launchd_label(id: &str) -> String {
    format!("app.alanworks.workflow.{id}")
}

fn xml_escape(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn shell_quote(raw: &str) -> String {
    if !raw.is_empty()
        && raw
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.'))
    {
        raw.to_string()
    } else {
        format!("'{}'", raw.replace('\'', "'\"'\"'"))
    }
}
