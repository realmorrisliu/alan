//! Composer image paste, clipboard attach, Backspace pop, and preview strip helpers.

use super::presentation::truncate_middle;
use super::*;
use crate::history::clamp_text_width;
use unicode_width::UnicodeWidthStr;

impl FileBackedApp {
    pub(super) fn handle_composer_paste(&mut self, text: &str) {
        match super::super::attachment_io::try_extract_pasted_image_attachments(text) {
            super::super::attachment_io::PasteImageOutcome::Attached(parts) => {
                self.attach_pending_images(parts);
            }
            super::super::attachment_io::PasteImageOutcome::Warning(message) => {
                self.notice = Some(Notice::warning(message));
            }
            super::super::attachment_io::PasteImageOutcome::NotPureImagePaste => {
                self.insert_input_text(text);
                self.refresh_completion();
            }
        }
    }

    pub(super) fn attach_pending_images(&mut self, parts: Vec<alan_agent_protocol::ContentPart>) {
        let mut seen_hashes: std::collections::BTreeSet<String> = self
            .pending_attachments
            .iter()
            .filter_map(super::super::attachment_io::attachment_hash)
            .map(str::to_string)
            .collect();
        let mut added = 0usize;
        for part in parts {
            if super::super::attachment_io::push_deduped_attachment(
                &mut self.pending_attachments,
                &mut seen_hashes,
                part,
            ) {
                added += 1;
            }
        }
        if added > 0 {
            self.attachment_revision += 1;
            if self.notice.as_ref().is_some_and(|n| {
                n.submission.is_none() && (!n.runtime_owned || n.kind == UiNoticeKind::Warning)
            }) {
                self.notice = None;
            }
        } else {
            self.notice = Some("image already attached".into());
        }
    }

    pub(super) fn attach_from_clipboard(&mut self) {
        #[cfg(target_os = "macos")]
        {
            match super::super::attachment_io::load_clipboard_image_attachment() {
                Ok(part) => self.attach_pending_images(vec![part]),
                Err(message) => self.notice = Some(Notice::warning(message)),
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            self.notice = Some(Notice::warning(
                "clipboard image paste is only supported on macOS; paste an image path instead",
            ));
        }
    }

    pub(super) fn pop_last_pending_attachment(&mut self) {
        if self.pending_attachments.pop().is_some() {
            self.attachment_revision += 1;
            if self
                .notice
                .as_deref()
                .is_some_and(|text| text == "image already attached")
            {
                self.notice = None;
            }
        }
    }

    pub(in crate::file_backed) fn attachment_preview_lines(
        &self,
        width: usize,
    ) -> Vec<Line<'static>> {
        if self.pending_attachments.is_empty() {
            return Vec::new();
        }
        let width = width.max(1);
        let count = self.pending_attachments.len();
        let count_label = if count == 1 {
            "1 image: ".to_string()
        } else {
            format!("{count} images: ")
        };
        let prefix = "  + ";
        let lead = format!("{prefix}{count_label}");
        let lead_width = UnicodeWidthStr::width(lead.as_str());

        let has_sizes = self
            .pending_attachments
            .iter()
            .any(|part| super::super::attachment_io::attachment_byte_len(part).is_some());
        let total_bytes: u64 = self
            .pending_attachments
            .iter()
            .filter_map(super::super::attachment_io::attachment_byte_len)
            .sum();
        let size_suffix = if has_sizes {
            format!(
                " ({})",
                super::super::attachment_io::format_byte_size(total_bytes)
            )
        } else {
            String::new()
        };
        let show_remove_hint =
            self.composer.text().is_empty() && self.input_intent != InputIntent::Command;
        let remove_hint = if show_remove_hint {
            " · Backspace remove"
        } else {
            ""
        };

        let names_raw = if count == 1 {
            super::super::attachment_io::attachment_filename(&self.pending_attachments[0])
                .to_string()
        } else {
            self.pending_attachments
                .iter()
                .map(super::super::attachment_io::attachment_filename)
                .collect::<Vec<_>>()
                .join(", ")
        };
        let suffix_with_hint = format!("{size_suffix}{remove_hint}");
        let suffix_width_with_hint = UnicodeWidthStr::width(suffix_with_hint.as_str());
        let (names, suffix) = if lead_width + suffix_width_with_hint + 8 <= width {
            let budget = width.saturating_sub(lead_width + suffix_width_with_hint);
            (truncate_middle(&names_raw, budget), suffix_with_hint)
        } else if lead_width + UnicodeWidthStr::width(size_suffix.as_str()) + 6 <= width {
            let budget =
                width.saturating_sub(lead_width + UnicodeWidthStr::width(size_suffix.as_str()));
            (truncate_middle(&names_raw, budget), size_suffix)
        } else {
            let budget = width.saturating_sub(lead_width);
            (truncate_middle(&names_raw, budget), String::new())
        };

        let body = clamp_text_width(
            &format!("{count_label}{names}{suffix}"),
            width.saturating_sub(UnicodeWidthStr::width(prefix)),
        );
        vec![Line::from(vec![
            Span::styled(prefix, self.theme.accent_style()),
            Span::styled(body, self.theme.muted_style()),
        ])]
    }
}
