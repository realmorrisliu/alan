//! Resolve existing evidence references through read-only namespace descriptors.
use ratatui::text::Line;
use serde_json::Value;

pub(super) const DISPLAY_BYTES: u64 = 262144;

pub(in crate::file_backed) async fn range(
    shell: &alan_shell::Shell,
    path: &str,
    offset: u64,
    length: u64,
) -> Result<String, String> {
    range_with_budget(shell, path, offset, length, DISPLAY_BYTES).await
}

pub(in crate::file_backed) async fn range_with_budget(
    shell: &alan_shell::Shell,
    path: &str,
    offset: u64,
    length: u64,
    budget: u64,
) -> Result<String, String> {
    if length > budget {
        return Err(format!(
            "display bound: requested {length} bytes exceeds {budget}; original remains in AgentFS"
        ));
    }
    offset.checked_add(length).ok_or("invalid range")?;
    read_bounded(shell, path, offset, Some(length), budget).await
}

pub(in crate::file_backed) async fn document_with_budget(
    shell: &alan_shell::Shell,
    path: &str,
    budget: u64,
) -> Result<String, String> {
    read_bounded(shell, path, 0, None, budget).await
}

async fn read_bounded(
    shell: &alan_shell::Shell,
    path: &str,
    offset: u64,
    length: Option<u64>,
    budget: u64,
) -> Result<String, String> {
    let bytes = shell
        .read_range_bounded(
            path,
            offset,
            length,
            budget,
            std::time::Duration::from_secs(5),
        )
        .await
        .map_err(|error| match error {
            alan_ap::ErrorCode::NotFound => {
                "unavailable: retained range missing or path not found".into()
            }
            _ => format!("unavailable: bounded range read: {error:?}"),
        })?;
    String::from_utf8(bytes).map_err(|_| "unavailable: range not UTF-8 aligned".into())
}

pub(super) struct Resolved {
    pub rows: Vec<Line<'static>>,
    pub readable: bool,
}
impl From<Vec<Line<'static>>> for Resolved {
    fn from(rows: Vec<Line<'static>>) -> Self {
        Self {
            rows,
            readable: false,
        }
    }
}

pub(super) async fn resolve(
    shell: &alan_shell::Shell,
    value: &Value,
    owner_path: &str,
) -> Resolved {
    let unavailable = |reason: &str| -> Resolved {
        vec![Line::from(format!(
            "Truncated evidence unavailable: {reason}"
        ))]
        .into()
    };
    let truncation = &value["truncation"];
    let original = truncation["original_bytes"].as_u64();
    let preview = truncation["preview_bytes"].as_u64();
    let mut rows = vec![Line::from(format!(
        "Evidence bytes: original {original:?}, preview {preview:?}"
    ))];
    if let Some(reason) = truncation["fallback_reason"].as_str() {
        rows.push(Line::from(format!("Evidence fallback reason: {reason}")));
    }
    if truncation["full_content_recoverable"].as_bool() != Some(true) {
        rows.extend(
            unavailable(
                truncation["fallback_reason"]
                    .as_str()
                    .unwrap_or("full content not recoverable"),
            )
            .rows,
        );
        return rows.into();
    }
    if let (Some(original), Some(preview)) = (original, preview)
        && (preview > original
            || value["preview"]
                .as_str()
                .is_some_and(|s| s.len() as u64 != preview))
    {
        return unavailable("invalid original/preview lengths");
    }
    let reference = &value["reference"];
    let Some(path) = reference["path"].as_str() else {
        return unavailable("no valid retained reference");
    };
    // Root is a moving alias: resolve it against the owner pinned for this request.
    let pinned = path
        .strip_prefix("/agent/root/")
        .map(|suffix| format!("{owner_path}/{suffix}"));
    let path = pinned.as_deref().unwrap_or(path);
    let stat = match shell.stat(path).await {
        Ok(stat) => stat,
        Err(e) => return unavailable(&format!("unavailable: {e:?}")),
    };
    // Expiry changes the file to a small structured record, regardless of the old range.
    if stat.length <= 4096
        && let Ok(text) = range(shell, path, 0, stat.length).await
        && serde_json::from_str::<Value>(&text)
            .ok()
            .is_some_and(|v| v["type"].as_str() == Some("evidence_retention_expired"))
    {
        return vec![Line::from(
            "Evidence retention expired: referenced original is no longer retained",
        )]
        .into();
    }
    let offset = reference["offset"].as_u64().unwrap_or(0);
    let length = reference["length"]
        .as_u64()
        .unwrap_or(stat.length.saturating_sub(offset));
    if (!reference["offset"].is_null() && reference["offset"].as_u64().is_none())
        || (!reference["length"].is_null() && reference["length"].as_u64().is_none())
    {
        return unavailable("invalid range");
    }
    let text = match range(shell, path, offset, length).await {
        Ok(text) => text,
        Err(e) => return unavailable(&e),
    };
    rows.push(Line::from(
        if offset == 0 && original == Some(length) && length == stat.length {
            "Recovered reference (retained original)"
        } else {
            "Recovered reference range only; full original length not verified"
        },
    ));
    if text.is_empty() {
        rows.push(Line::from("(empty)"));
    }
    if text.contains("[REDACTED reason=") {
        rows.push(Line::from("Redacted evidence (not truncation)"));
    }
    let full = offset == 0 && original == Some(length) && length == stat.length;
    let readable = super::presentation::acquired_content(&mut rows, &text) && full;
    Resolved { rows, readable }
}
