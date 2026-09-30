//! Resolve existing evidence references through read-only namespace descriptors.
use ratatui::text::Line;
use serde_json::Value;

pub(super) const DISPLAY_BYTES: u64 = 262144;

pub(super) async fn range(
    shell: &alan_shell::Shell,
    path: &str,
    offset: u64,
    length: u64,
) -> Result<String, String> {
    if length > DISPLAY_BYTES {
        return Err(format!(
            "display bound: requested {length} bytes exceeds {DISPLAY_BYTES}; original remains in AgentFS"
        ));
    }
    let end = offset.checked_add(length).ok_or("invalid range")?;
    let stat = shell
        .stat(path)
        .await
        .map_err(|e| format!("unavailable: {e:?}"))?;
    if end > stat.length {
        return Err("unavailable: retained range missing".into());
    }
    let mut descriptor = shell
        .tail_from(path, offset)
        .await
        .map_err(|e| format!("unavailable: {e:?}"))?;
    let result = async {
        let mut bytes = Vec::new();
        while bytes.len() < length as usize {
            let count = (length - bytes.len() as u64).min(4096) as u32;
            let chunk =
                tokio::time::timeout(std::time::Duration::from_secs(5), descriptor.read(count))
                    .await
                    .map_err(|_| "unavailable: retained read timed out".to_string())?
                    .map_err(|e| format!("unavailable: {e:?}"))?;
            if chunk.is_empty() || chunk.len() > count as usize {
                return Err("unavailable: short or invalid range read".into());
            }
            bytes.extend(chunk);
        }
        String::from_utf8(bytes).map_err(|_| "unavailable: range not UTF-8 aligned".into())
    }
    .await;
    let close = descriptor
        .close()
        .await
        .map_err(|e| format!("unavailable: descriptor close: {e:?}"));
    close?;
    result
}

pub(super) async fn resolve(
    shell: &alan_shell::Shell,
    value: &Value,
    owner_path: &str,
) -> Vec<Line<'static>> {
    let unavailable = |reason: &str| {
        vec![Line::from(format!(
            "Truncated evidence unavailable: {reason}"
        ))]
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
        rows.extend(unavailable(
            truncation["fallback_reason"]
                .as_str()
                .unwrap_or("full content not recoverable"),
        ));
        return rows;
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
        )];
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
    rows.push(Line::from(if original == Some(length) {
        "Recovered reference (retained original)"
    } else {
        "Recovered reference range only; full original length not verified"
    }));
    if text.is_empty() {
        rows.push(Line::from("(empty)"));
    }
    if text.contains("[REDACTED reason=") {
        rows.push(Line::from("Redacted evidence (not truncation)"));
    }
    rows.extend(
        text.lines()
            .map(|s| Line::from(crate::history::clean_text(s))),
    );
    rows
}
