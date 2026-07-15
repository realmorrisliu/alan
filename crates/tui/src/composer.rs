use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// The bottom input editor: readline-style editing plus persisted history recall.
#[derive(Debug, Default, Clone)]
pub struct Composer {
    buffer: String,
    cursor: usize,
    history: Vec<String>,
    /// Index into `history` while recalling; `None` while editing the live buffer.
    history_index: Option<usize>,
    /// Live buffer stashed while recalling history.
    stash: Option<String>,
    history_path: Option<PathBuf>,
}

impl Composer {
    /// Build a composer seeded with prior history and a path to append new entries to.
    pub fn with_history(history: Vec<String>, history_path: Option<PathBuf>) -> Self {
        Self {
            history,
            history_path,
            ..Self::default()
        }
    }

    pub fn text(&self) -> &str {
        &self.buffer
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.buffer = text.into();
        self.cursor = self.buffer.len();
        self.reset_recall();
    }

    pub fn set_text_with_cursor(&mut self, text: impl Into<String>, cursor: usize) {
        self.buffer = text.into();
        self.cursor = cursor.min(self.buffer.len());
        self.reset_recall();
    }

    pub fn insert_text(&mut self, text: &str) {
        self.buffer.insert_str(self.cursor, text);
        self.cursor += text.len();
        self.reset_recall();
    }

    pub fn take_submit(&mut self) -> Option<String> {
        let text = self.buffer.trim().to_string();
        if text.is_empty() {
            return None;
        }
        self.buffer.clear();
        self.cursor = 0;
        self.reset_recall();
        Some(text)
    }

    /// Record a submitted entry into history (adjacent-deduplicated) and persist it.
    pub fn remember(&mut self, entry: &str) {
        let entry = entry.trim();
        if entry.is_empty() {
            return;
        }
        if self.history.last().map(String::as_str) == Some(entry) {
            self.reset_recall();
            return;
        }
        self.history.push(entry.to_string());
        self.reset_recall();
        if let Some(path) = &self.history_path
            && let Err(err) = append_history_line(path, entry)
        {
            tracing::warn!(%err, "failed to persist composer history");
        }
    }

    pub fn handle_key(&mut self, event: KeyEvent) -> ComposerKeyOutcome {
        let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
        let alt = event.modifiers.contains(KeyModifiers::ALT);
        match event.code {
            KeyCode::Char('c') if ctrl => ComposerKeyOutcome::Interrupt,
            KeyCode::Char('a') if ctrl => {
                self.cursor = 0;
                ComposerKeyOutcome::Changed
            }
            KeyCode::Char('e') if ctrl => {
                self.cursor = self.buffer.len();
                ComposerKeyOutcome::Changed
            }
            KeyCode::Char('u') if ctrl => {
                self.buffer.drain(..self.cursor);
                self.cursor = 0;
                self.reset_recall();
                ComposerKeyOutcome::Changed
            }
            KeyCode::Char('w') if ctrl => {
                self.delete_word_back();
                ComposerKeyOutcome::Changed
            }
            KeyCode::Left if alt => {
                self.cursor = self.word_start_before(self.cursor);
                ComposerKeyOutcome::Changed
            }
            KeyCode::Right if alt => {
                self.cursor = self.word_end_after(self.cursor);
                ComposerKeyOutcome::Changed
            }
            KeyCode::Home => {
                self.cursor = 0;
                ComposerKeyOutcome::Changed
            }
            KeyCode::End => {
                self.cursor = self.buffer.len();
                ComposerKeyOutcome::Changed
            }
            KeyCode::Up => {
                self.history_prev();
                ComposerKeyOutcome::Changed
            }
            KeyCode::Down => {
                self.history_next();
                ComposerKeyOutcome::Changed
            }
            KeyCode::Char(ch) => {
                self.insert_text(&ch.to_string());
                ComposerKeyOutcome::Changed
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    let prev = self.prev_boundary(self.cursor);
                    self.buffer.drain(prev..self.cursor);
                    self.cursor = prev;
                    self.reset_recall();
                    ComposerKeyOutcome::Changed
                } else {
                    ComposerKeyOutcome::Ignored
                }
            }
            KeyCode::Left => {
                if self.cursor > 0 {
                    self.cursor = self.prev_boundary(self.cursor);
                    ComposerKeyOutcome::Changed
                } else {
                    ComposerKeyOutcome::Ignored
                }
            }
            KeyCode::Right => {
                if self.cursor < self.buffer.len() {
                    self.cursor = self.next_boundary(self.cursor);
                    ComposerKeyOutcome::Changed
                } else {
                    ComposerKeyOutcome::Ignored
                }
            }
            KeyCode::Enter if event.modifiers.contains(KeyModifiers::SHIFT) => {
                self.buffer.insert(self.cursor, '\n');
                self.cursor += 1;
                self.reset_recall();
                ComposerKeyOutcome::Changed
            }
            KeyCode::Enter => ComposerKeyOutcome::Submit,
            _ => ComposerKeyOutcome::Ignored,
        }
    }

    fn reset_recall(&mut self) {
        self.history_index = None;
        self.stash = None;
    }

    fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let next_index = match self.history_index {
            None => {
                self.stash = Some(self.buffer.clone());
                self.history.len() - 1
            }
            Some(0) => 0,
            Some(index) => index - 1,
        };
        self.history_index = Some(next_index);
        self.buffer = self.history[next_index].clone();
        self.cursor = self.buffer.len();
    }

    fn history_next(&mut self) {
        let Some(index) = self.history_index else {
            return;
        };
        if index + 1 < self.history.len() {
            self.history_index = Some(index + 1);
            self.buffer = self.history[index + 1].clone();
        } else {
            self.history_index = None;
            self.buffer = self.stash.take().unwrap_or_default();
        }
        self.cursor = self.buffer.len();
    }

    fn delete_word_back(&mut self) {
        let start = self.word_start_before(self.cursor);
        if start < self.cursor {
            self.buffer.drain(start..self.cursor);
            self.cursor = start;
            self.reset_recall();
        }
    }

    fn prev_boundary(&self, index: usize) -> usize {
        self.buffer[..index]
            .char_indices()
            .last()
            .map(|(idx, _)| idx)
            .unwrap_or(0)
    }

    fn next_boundary(&self, index: usize) -> usize {
        index
            + self.buffer[index..]
                .chars()
                .next()
                .map(char::len_utf8)
                .unwrap_or(0)
    }

    fn word_start_before(&self, index: usize) -> usize {
        let bytes = &self.buffer[..index];
        let mut pos = index;
        let mut seen_word = false;
        for (idx, ch) in bytes.char_indices().rev() {
            if ch.is_alphanumeric() || ch == '_' {
                seen_word = true;
                pos = idx;
            } else if seen_word {
                break;
            } else {
                pos = idx;
            }
        }
        pos
    }

    fn word_end_after(&self, index: usize) -> usize {
        let mut pos = index;
        let mut seen_word = false;
        for (idx, ch) in self.buffer[index..].char_indices() {
            let abs = index + idx + ch.len_utf8();
            if ch.is_alphanumeric() || ch == '_' {
                seen_word = true;
                pos = abs;
            } else if seen_word {
                break;
            } else {
                pos = abs;
            }
        }
        pos
    }
}

fn append_history_line(path: &PathBuf, entry: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{}", entry.replace('\n', " "))
}

/// Load history entries from a file, oldest first. Missing file yields empty history.
pub fn load_history(path: &PathBuf, limit: usize) -> Vec<String> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut entries: Vec<String> = contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    if entries.len() > limit {
        entries.drain(..entries.len() - limit);
    }
    entries
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerKeyOutcome {
    Changed,
    Submit,
    Interrupt,
    Ignored,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn key_mod(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn composer_submits_and_clears_text() {
        let mut composer = Composer::default();
        composer.handle_key(key(KeyCode::Char('h')));
        composer.handle_key(key(KeyCode::Char('i')));
        assert_eq!(
            composer.handle_key(key(KeyCode::Enter)),
            ComposerKeyOutcome::Submit
        );
        assert_eq!(composer.take_submit(), Some("hi".into()));
        assert_eq!(composer.text(), "");
    }

    #[test]
    fn composer_inserts_paste_at_cursor() {
        let mut composer = Composer::default();
        composer.set_text("ac");
        composer.handle_key(key(KeyCode::Left));
        composer.insert_text("b\n");
        assert_eq!(composer.text(), "ab\nc");
    }

    #[test]
    fn ctrl_a_and_ctrl_e_jump_to_line_ends() {
        let mut composer = Composer::default();
        composer.set_text("hello");
        composer.handle_key(key_mod(KeyCode::Char('a'), KeyModifiers::CONTROL));
        assert_eq!(composer.cursor(), 0);
        composer.handle_key(key_mod(KeyCode::Char('e'), KeyModifiers::CONTROL));
        assert_eq!(composer.cursor(), 5);
    }

    #[test]
    fn ctrl_w_deletes_previous_word() {
        let mut composer = Composer::default();
        composer.set_text("hello world");
        composer.handle_key(key_mod(KeyCode::Char('w'), KeyModifiers::CONTROL));
        assert_eq!(composer.text(), "hello ");
    }

    #[test]
    fn ctrl_u_deletes_to_line_start() {
        let mut composer = Composer::default();
        composer.set_text("hello world");
        composer.handle_key(key(KeyCode::Left));
        composer.handle_key(key_mod(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert_eq!(composer.text(), "d");
    }

    #[test]
    fn alt_left_moves_by_word() {
        let mut composer = Composer::default();
        composer.set_text("hello world");
        composer.handle_key(key_mod(KeyCode::Left, KeyModifiers::ALT));
        assert_eq!(composer.cursor(), 6);
        composer.handle_key(key_mod(KeyCode::Left, KeyModifiers::ALT));
        assert_eq!(composer.cursor(), 0);
    }

    #[test]
    fn history_recall_walks_previous_submissions() {
        let mut composer = Composer::default();
        composer.remember("first");
        composer.remember("second");
        composer.set_text("draft");
        composer.handle_key(key(KeyCode::Up));
        assert_eq!(composer.text(), "second");
        composer.handle_key(key(KeyCode::Up));
        assert_eq!(composer.text(), "first");
        composer.handle_key(key(KeyCode::Down));
        assert_eq!(composer.text(), "second");
        composer.handle_key(key(KeyCode::Down));
        assert_eq!(composer.text(), "draft");
    }

    #[test]
    fn history_dedupes_adjacent_entries() {
        let mut composer = Composer::default();
        composer.remember("same");
        composer.remember("same");
        composer.handle_key(key(KeyCode::Up));
        assert_eq!(composer.text(), "same");
        composer.handle_key(key(KeyCode::Up));
        // only one entry, stays put
        assert_eq!(composer.text(), "same");
    }

    #[test]
    fn history_persists_across_launches_via_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tui_history");
        {
            let mut composer = Composer::with_history(Vec::new(), Some(path.clone()));
            composer.remember("persisted entry");
        }
        let loaded = load_history(&path, 100);
        assert_eq!(loaded, vec!["persisted entry".to_string()]);
        let mut next = Composer::with_history(loaded, Some(path));
        next.handle_key(key(KeyCode::Up));
        assert_eq!(next.text(), "persisted entry");
    }
}
