//! Inline image extraction (user attachments & tool results) and atomic scrollback draining.

use alan_agent_protocol::ContentPart;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
#[cfg(test)]
use ratatui::text::Line;
use serde_json::Value;

use crate::graphics::parse_image_dimensions;
use crate::graphics_scrollback::ScrollbackItem;
#[cfg(test)]
use crate::history::RenderOpts;
use crate::history::{HistoryCell, InlineImageCell};

use super::app::FileBackedApp;
use super::file_surface::{ActionSnapshot, action_title, normalize_project_mount_paths};

/// Extract inline image cells from submitted user [`ContentPart`] attachments.
pub(super) fn extract_user_images_from_parts(parts: &[ContentPart]) -> Vec<InlineImageCell> {
    parts
        .iter()
        .filter_map(|part| extract_image_from_content_part(part, "image"))
        .collect()
}

/// Extract inline image cells from an [`ActionSnapshot`]'s JSON `result.content_parts`.
pub(super) fn extract_action_images(snapshot: &ActionSnapshot) -> Vec<InlineImageCell> {
    let Ok(metadata) = serde_json::from_str::<Value>(&snapshot.result) else {
        return Vec::new();
    };
    let Some(parts) = metadata
        .get("content_parts")
        .or_else(|| metadata.get("result").and_then(|v| v.get("content_parts")))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let fallback_label = normalize_project_mount_paths(&action_title(snapshot));
    let mut images = Vec::new();
    for item in parts {
        if let Ok(part) = serde_json::from_value::<ContentPart>(item.clone())
            && let Some(cell) = extract_image_from_content_part(&part, &fallback_label)
        {
            images.push(cell);
            continue;
        }
        if let Some(cell) = extract_image_from_raw_part(item, &fallback_label) {
            images.push(cell);
        }
    }
    images
}

fn extract_image_from_content_part(
    part: &ContentPart,
    fallback_label: &str,
) -> Option<InlineImageCell> {
    let ContentPart::Attachment {
        mime_type,
        metadata,
        ..
    } = part
    else {
        return None;
    };
    let bytes = decode_base64_image_bytes(metadata)?;
    if !is_image_mime(mime_type) && parse_image_dimensions(&bytes).is_none() {
        return None;
    }
    let label = image_label_from_metadata(metadata, fallback_label);
    Some(InlineImageCell::new(bytes, label))
}

fn extract_image_from_raw_part(item: &Value, fallback_label: &str) -> Option<InlineImageCell> {
    let obj = item.as_object()?;
    let part_type = obj.get("type").and_then(Value::as_str).unwrap_or_default();
    let mime = obj
        .get("mime_type")
        .or_else(|| obj.get("media_type"))
        .or_else(|| obj.get("source").and_then(|s| s.get("media_type")))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let meta = obj.get("metadata").unwrap_or(item);
    let bytes = decode_base64_image_bytes(item)
        .or_else(|| decode_base64_image_bytes(meta))
        .or_else(|| obj.get("source").and_then(decode_base64_image_bytes))?;
    let looks_like_image = part_type.eq_ignore_ascii_case("image")
        || is_image_mime(mime)
        || parse_image_dimensions(&bytes).is_some();
    if !looks_like_image {
        return None;
    }
    let label = image_label_from_metadata(meta, fallback_label);
    Some(InlineImageCell::new(bytes, label))
}

fn is_image_mime(mime: &str) -> bool {
    mime.trim().to_ascii_lowercase().starts_with("image/")
}

fn decode_base64_image_bytes(value: &Value) -> Option<Vec<u8>> {
    if let Some(raw) = value.get("data").and_then(Value::as_str)
        && let Some(bytes) = decode_base64_nonempty(raw)
    {
        return Some(bytes);
    }
    if let Some(uri) = value.get("image_url").and_then(Value::as_str)
        && let Some(encoded) = strip_data_uri_base64(uri)
        && let Some(bytes) = decode_base64_nonempty(encoded)
    {
        return Some(bytes);
    }
    None
}

fn strip_data_uri_base64(uri: &str) -> Option<&str> {
    let rest = uri.trim().strip_prefix("data:")?;
    let (_, encoded) = rest.split_once(";base64,")?;
    Some(encoded)
}

fn decode_base64_nonempty(encoded: &str) -> Option<Vec<u8>> {
    let trimmed = encoded.trim();
    if trimmed.is_empty() {
        return None;
    }
    let bytes = BASE64_STANDARD.decode(trimmed).ok()?;
    (!bytes.is_empty()).then_some(bytes)
}

fn image_label_from_metadata(metadata: &Value, fallback_label: &str) -> String {
    let raw = metadata
        .get("filename")
        .or_else(|| metadata.get("name"))
        .or_else(|| metadata.get("path"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback_label);
    let normalized = normalize_project_mount_paths(raw);
    if normalized.trim().is_empty() {
        "image".to_string()
    } else {
        normalized
    }
}

impl FileBackedApp {
    /// Attach decoded image cells from a submitted user prompt to its tracked local input.
    pub(super) fn record_submitted_images(&mut self, submission_id: &str, parts: &[ContentPart]) {
        let images = extract_user_images_from_parts(parts);
        if images.is_empty() {
            return;
        }
        if let Some(input) = self.local_inputs.get_mut(submission_id) {
            input.pending_images = images;
        }
    }

    /// Remove any un-drained temporary receipt `HistoryCell::Image` entries owned by `ids`
    /// and return them in original attachment order so they can be re-inserted cleanly after
    /// the authoritative user tape cell.
    pub(super) fn reclaim_local_receipt_images(&mut self, ids: &[String]) -> Vec<InlineImageCell> {
        let mut indices = Vec::new();
        for id in ids {
            if let Some(input) = self.local_inputs.get_mut(id) {
                indices.append(&mut input.image_cells);
            }
        }
        indices.sort_unstable();
        indices.dedup();
        let mut reclaimed = Vec::with_capacity(indices.len());
        for idx in indices.into_iter().rev() {
            if idx < self.transcript.len()
                && let HistoryCell::Image(image) = self.remove_transcript_cell_at(idx)
            {
                reclaimed.push(image);
            }
        }
        reclaimed.reverse();
        reclaimed
    }

    /// Insert reclaimed and/or still-pending local submission images right after `user_index`.
    pub(super) fn emit_local_images_after(
        &mut self,
        ids: &[String],
        user_index: usize,
        mut images: Vec<InlineImageCell>,
    ) {
        for id in ids {
            if let Some(input) = self.local_inputs.get_mut(id) {
                images.extend(std::mem::take(&mut input.pending_images));
            }
        }
        for (offset, image) in images.into_iter().enumerate() {
            let insert_at = (user_index + 1 + offset).min(self.transcript.len());
            self.transcript.insert(insert_at, HistoryCell::Image(image));
            self.shift_action_cells_for_insert(insert_at);
        }
    }

    /// Upsert a completed tool cell and emit any tool-produced images right after it once per
    /// `(owner, action_id)`, without altering [`HistoryCell::Tool`] fields.
    pub(super) fn upsert_action_cell_with_images(
        &mut self,
        action_id: String,
        cell: HistoryCell,
        images: Vec<InlineImageCell>,
    ) {
        self.upsert_action_cell(action_id.clone(), cell);
        self.emitted_action_images
            .retain(|key| self.projected_actions.contains_key(key));
        if images.is_empty() {
            return;
        }
        let key = (self.action_owner().to_owned(), action_id.clone());
        if !self.emitted_action_images.insert(key) {
            return;
        }
        if let Some(tool_index) = self.action_cells.get(&action_id).copied() {
            for (offset, image) in images.into_iter().enumerate() {
                let insert_at = (tool_index + 1 + offset).min(self.transcript.len());
                self.transcript.insert(insert_at, HistoryCell::Image(image));
                self.shift_action_cells_for_insert(insert_at);
            }
        } else {
            for image in images {
                self.push_turn_preview_cell(HistoryCell::Image(image));
            }
        }
    }

    /// Drain committed transcript prefix into typed [`ScrollbackItem`]s so [`HistoryCell::Image`]
    /// commits atomically as [`ScrollbackItem::Image`] and is never split mid-cell.
    pub(super) fn drain_committed_scrollback_items(
        &mut self,
        viewport_width: usize,
        viewport_height: usize,
    ) -> Vec<ScrollbackItem> {
        if self.modal.active || self.resume_picker.active || self.issue.is_active() {
            return Vec::new();
        }
        let opts = self.render_opts(viewport_width);
        let max_lines =
            viewport_height.saturating_sub(super::layout::base_region_height(self, viewport_width));
        let lines = self.styled_history_lines(viewport_width);
        let drain_count = super::history_prefix_to_drain(&lines, viewport_width, max_lines);
        if drain_count == 0 {
            return Vec::new();
        }
        let (_, items) = self.prune_rendered_prefix_items(opts, &lines, drain_count);
        items
    }

    /// Flatten drained [`ScrollbackItem`]s into styled lines for text-only callers/tests.
    #[cfg(test)]
    pub(super) fn flatten_scrollback_items(
        items: Vec<ScrollbackItem>,
        opts: RenderOpts,
    ) -> Vec<Line<'static>> {
        let mut out = Vec::new();
        for item in items {
            match item {
                ScrollbackItem::Lines(lines) => out.extend(lines),
                ScrollbackItem::Image {
                    bytes,
                    label,
                    dimensions,
                } => out.extend(
                    HistoryCell::Image(InlineImageCell {
                        bytes,
                        label,
                        dimensions,
                    })
                    .render_styled_lines(opts),
                ),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_backed::file_surface::{TapeRecordV1, sync_action_snapshot};
    use crate::file_backed::layout;
    use crate::graphics::{ImageDimensions, ImageFormat};
    use alan_agent_protocol::InputIntent;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn sample_png(w: u32, h: u32) -> Vec<u8> {
        let head = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR";
        [head.as_slice(), &w.to_be_bytes(), &h.to_be_bytes()].concat()
    }

    fn png_attachment(filename: &str, w: u32, h: u32) -> ContentPart {
        let bytes = sample_png(w, h);
        let encoded = BASE64_STANDARD.encode(&bytes);
        ContentPart::Attachment {
            hash: "deadbeef".to_string(),
            mime_type: "image/png".to_string(),
            metadata: serde_json::json!({
                "filename": filename,
                "byte_len": bytes.len(),
                "data": encoded,
            }),
        }
    }

    #[test]
    fn extracts_user_images_from_data_and_image_url_and_skips_non_images() {
        let png_bytes = sample_png(320, 240);
        let encoded = BASE64_STANDARD.encode(&png_bytes);
        let parts = vec![
            ContentPart::text("describe these"),
            png_attachment("screen.png", 640, 480),
            ContentPart::Attachment {
                hash: "url-only".into(),
                mime_type: "image/png".into(),
                metadata: serde_json::json!({
                    "path": "/mnt/project-request-1/assets/logo.png",
                    "image_url": format!("data:image/png;base64,{encoded}"),
                }),
            },
            ContentPart::Attachment {
                hash: "pdf".into(),
                mime_type: "application/pdf".into(),
                metadata: serde_json::json!({
                    "filename": "doc.pdf",
                    "data": BASE64_STANDARD.encode(b"%PDF-1.7"),
                }),
            },
            ContentPart::Attachment {
                hash: "bad-b64".into(),
                mime_type: "image/png".into(),
                metadata: serde_json::json!({
                    "filename": "broken.png",
                    "data": "!!!not-base64!!!",
                }),
            },
        ];

        let extracted = extract_user_images_from_parts(&parts);
        assert_eq!(extracted.len(), 2);
        assert_eq!(extracted[0].label, "screen.png");
        assert_eq!(
            extracted[0].dimensions,
            Some(ImageDimensions {
                width: 640,
                height: 480,
                format: ImageFormat::Png,
            })
        );
        assert_eq!(extracted[1].label, "assets/logo.png");
        assert_eq!(extracted[1].bytes, png_bytes);
    }

    #[test]
    fn extracts_tool_images_from_action_snapshot_and_deduplicates_without_changing_tool_cell() {
        let png_bytes = sample_png(400, 300);
        let encoded = BASE64_STANDARD.encode(&png_bytes);
        let attachment = png_attachment("rendered.png", 400, 300);
        let result_json = serde_json::json!({
            "title": "read_file diagram.png",
            "content_parts": [
                serde_json::to_value(&attachment).unwrap(),
                {
                    "type": "image",
                    "mime_type": "image/png",
                    "filename": "raw_plot.png",
                    "data": encoded,
                }
            ]
        })
        .to_string();

        let snapshot = ActionSnapshot {
            id: "a1".into(),
            name: "read_file".into(),
            status: "completed".into(),
            output: String::new(),
            result: result_json,
        };

        let mut app = FileBackedApp::new("/agent/1".into());
        sync_action_snapshot(&mut app, snapshot.clone());
        sync_action_snapshot(&mut app, snapshot);

        assert_eq!(app.transcript.len(), 3);
        assert!(matches!(app.transcript[0], HistoryCell::Tool { .. }));
        match &app.transcript[1] {
            HistoryCell::Image(img) => {
                assert_eq!(img.label, "rendered.png");
                assert_eq!(img.bytes, png_bytes);
            }
            other => panic!("expected first tool image cell, got {other:?}"),
        }
        match &app.transcript[2] {
            HistoryCell::Image(img) => {
                assert_eq!(img.label, "raw_plot.png");
                assert_eq!(img.bytes, png_bytes);
            }
            other => panic!("expected second tool image cell, got {other:?}"),
        }
    }

    #[test]
    fn user_submitted_images_order_cleanly_across_receipt_and_tape_reconciliation() {
        let png = sample_png(120, 80);
        let part = png_attachment("paste.png", 120, 80);

        // Case 1: Queue acknowledgement materializes receipt first, then tape arrives.
        let mut app = FileBackedApp::new("/agent/1".into());
        app.queue.owner = "/agent/1".into();
        app.track_local_input(
            "sub-1",
            "/agent/1".into(),
            "check this".into(),
            InputIntent::Agent,
        );
        app.record_submitted_images("sub-1", std::slice::from_ref(&part));
        app.acknowledge_local_input("sub-1", "/agent/1");
        assert_eq!(app.transcript.len(), 2);
        assert!(matches!(app.transcript[0], HistoryCell::User(_)));
        assert!(matches!(app.transcript[1], HistoryCell::Image(_)));

        app.apply_tape_record(TapeRecordV1 {
            end_offset: 64,
            version: 1,
            kind: "message".into(),
            role: "user".into(),
            content: "check this".into(),
            input_intent: Some(InputIntent::Agent),
            submission_id: Some("sub-1".into()),
            related_submission_ids: Vec::new(),
        });
        assert_eq!(app.transcript.len(), 2);
        assert_eq!(app.transcript[0], HistoryCell::User("check this".into()));
        assert_eq!(
            app.transcript[1],
            HistoryCell::Image(InlineImageCell::new(png.clone(), "paste.png"))
        );

        // Case 2: Tape arrives before queue acknowledgement.
        let mut app2 = FileBackedApp::new("/agent/1".into());
        app2.queue.owner = "/agent/1".into();
        app2.track_local_input(
            "sub-2",
            "/agent/1".into(),
            "from tape".into(),
            InputIntent::Agent,
        );
        app2.record_submitted_images("sub-2", &[part]);
        app2.apply_tape_record(TapeRecordV1 {
            end_offset: 64,
            version: 1,
            kind: "message".into(),
            role: "user".into(),
            content: "from tape".into(),
            input_intent: Some(InputIntent::Agent),
            submission_id: Some("sub-2".into()),
            related_submission_ids: Vec::new(),
        });
        assert_eq!(app2.transcript.len(), 2);
        assert_eq!(app2.transcript[0], HistoryCell::User("from tape".into()));
        assert_eq!(
            app2.transcript[1],
            HistoryCell::Image(InlineImageCell::new(png, "paste.png"))
        );
    }

    #[test]
    fn drain_committed_scrollback_items_commits_images_atomically_never_splitting_mid_cell() {
        let png = sample_png(1024, 768);
        let long_label = "very_long_screenshot_filename_that_wraps_on_narrow_width.png";
        let image_cell = InlineImageCell::new(png.clone(), long_label);
        let opts = RenderOpts::new(20, false);
        let wrapped_rows = HistoryCell::Image(image_cell.clone())
            .render_styled_lines(opts)
            .len();
        assert!(
            wrapped_rows >= 2,
            "test requires multi-row wrapped badge at narrow width, got {wrapped_rows}"
        );

        // When drain budget is smaller than the wrapped image badge rows, image stays intact.
        let mut app = FileBackedApp::new("/agent/1".into());
        app.transcript.push(HistoryCell::User("hi".into()));
        app.transcript.push(HistoryCell::Image(image_cell.clone()));
        let base_height = layout::base_region_height(&app, 20);
        // Leave enough room for `wrapped_rows` lines so drain_count = 1 (only User fits).
        let viewport_height = base_height + wrapped_rows;
        let first = app.drain_committed_scrollback_items(20, viewport_height);
        assert_eq!(first.len(), 1);
        assert!(matches!(first[0], ScrollbackItem::Lines(_)));
        assert_eq!(app.transcript, vec![HistoryCell::Image(image_cell.clone())]);

        // Now shrink budget by 1 so drain_count == 1 < wrapped_rows -> MUST NOT split Image!
        let partial = app.drain_committed_scrollback_items(20, base_height + wrapped_rows - 1);
        assert!(
            partial.is_empty(),
            "must not split HistoryCell::Image mid-cell"
        );
        assert_eq!(app.transcript, vec![HistoryCell::Image(image_cell)]);

        // Once full budget drains (>= wrapped_rows), HistoryCell::Image drains atomically.
        app.transcript.push(HistoryCell::Assistant("done".into()));
        let full = app.drain_committed_scrollback_items(20, base_height);
        assert!(
            matches!(
                full.first(),
                Some(ScrollbackItem::Image { bytes, label, dimensions })
                    if bytes == &png && label == long_label && dimensions.is_some()
            ),
            "expected atomic ScrollbackItem::Image, got {full:?}"
        );
        assert!(
            !app.transcript
                .iter()
                .any(|cell| matches!(cell, HistoryCell::Image(_))),
            "drained image cell must be removed from live transcript"
        );
    }

    #[test]
    fn live_layout_draw_emits_only_text_badge_and_zero_graphics_escapes() {
        let png = sample_png(512, 256);
        let mut app = FileBackedApp::new("/agent/1".into());
        app.transcript.push(HistoryCell::User("inspect".into()));
        app.transcript
            .push(HistoryCell::Image(InlineImageCell::new(png, "diagram.png")));

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| layout::draw_at(frame, &app, 0))
            .unwrap();

        let rendered: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            rendered.contains("[image: diagram.png"),
            "live viewport must display calm badge text:\n{rendered}"
        );
        assert!(
            !rendered.contains('\x1b'),
            "live layout draw must never emit ESC / Kitty APC / OSC 1337 bytes"
        );
    }
}
