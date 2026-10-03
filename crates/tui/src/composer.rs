use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use alan_agent_protocol::InputIntent;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// A recalled body and its explicitly recorded submission intent.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryEntry {
    /// Exact submitted body, without route framing.
    pub body: String,
    /// Intent recorded at submission time.
    pub intent: InputIntent,
}

impl HistoryEntry {
    fn agent(body: String) -> Self {
        Self {
            body,
            intent: InputIntent::Agent,
        }
    }
}

/// The bottom input editor: readline-style editing plus persisted history recall.
#[derive(Debug, Default, Clone)]
pub struct Composer {
    buffer: String,
    cursor: usize,
    history: Vec<HistoryEntry>,
    /// Index into `history` while recalling; `None` while editing the live buffer.
    history_index: Option<usize>,
    /// Live buffer stashed while recalling history.
    stash: Option<String>,
    history_path: Option<PathBuf>,
    draft_revision: u64,
    history_unavailable: bool,
}

impl Composer {
    /// Build a composer seeded with prior history and a path to append new entries to.
    pub fn with_history(history: Vec<HistoryEntry>, history_path: Option<PathBuf>) -> Self {
        Self {
            history,
            history_path,
            ..Self::default()
        }
    }

    /// Load the configured persistent history with the renderer's recall limit.
    pub fn from_history_path(path: PathBuf) -> Self {
        let (history, history_unavailable) = load_history_status(&path, crate::HISTORY_LIMIT);
        Self {
            history_unavailable,
            ..Self::with_history(history, Some(path))
        }
    }

    /// Safe nonfatal feedback when persistent history is unavailable.
    pub fn history_notice(&self) -> Option<&'static str> {
        self.history_unavailable
            .then_some("Composer history unavailable; recall is session-only")
    }

    pub fn text(&self) -> &str {
        &self.buffer
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub(crate) fn is_recalling(&self) -> bool {
        self.history_index.is_some()
    }

    pub(crate) fn recalled_intent(&self) -> Option<InputIntent> {
        self.history_index.map(|index| self.history[index].intent)
    }

    pub(crate) fn draft_revision(&self) -> u64 {
        self.draft_revision
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.draft_revision += 1;
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
        if self.buffer.trim().is_empty() {
            return None;
        }
        let text = std::mem::take(&mut self.buffer);
        self.cursor = 0;
        self.reset_recall();
        Some(text)
    }

    /// Record a submitted entry into history (adjacent-deduplicated) and persist it.
    pub fn remember(&mut self, entry: &str) {
        self.remember_input(entry, InputIntent::Agent);
    }

    /// Persist an accepted input with intent independent of its text.
    pub fn remember_input(&mut self, body: &str, intent: InputIntent) {
        if body.trim().is_empty() {
            return;
        }
        let entry = HistoryEntry {
            body: body.to_owned(),
            intent,
        };
        if self.history.last() == Some(&entry) {
            self.reset_recall();
            return;
        }
        self.history.push(entry.clone());
        self.reset_recall();
        if let Some(path) = &self.history_path
            && append_history_line(path, &entry).is_err()
        {
            self.history_unavailable = true;
            tracing::warn!("composer history unavailable");
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
        self.draft_revision += 1;
        self.history_index = None;
        self.stash = None;
    }

    fn history_prev(&mut self) {
        self.draft_revision += 1;
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
        self.buffer = self.history[next_index].body.clone();
        self.cursor = self.buffer.len();
    }

    fn history_next(&mut self) {
        self.draft_revision += 1;
        let Some(index) = self.history_index else {
            return;
        };
        if index + 1 < self.history.len() {
            self.history_index = Some(index + 1);
            self.buffer = self.history[index + 1].body.clone();
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

fn history_records_path(path: &std::path::Path, version: u8) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".v{version}.jsonl"));
    PathBuf::from(name)
}

fn append_history_line(path: &std::path::Path, entry: &HistoryEntry) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(history_records_path(path, 2))?;
    let encoded = serde_json::to_string(entry).map_err(std::io::Error::other)?;
    writeln!(file, "{encoded}")
}

/// Load legacy Agent entries, then typed v2 history, oldest first.
/// New records live beside the legacy file with a `.v2.jsonl` suffix.
pub fn load_history(path: &std::path::Path, limit: usize) -> Vec<HistoryEntry> {
    load_history_status(path, limit).0
}

fn load_history_status(path: &std::path::Path, limit: usize) -> (Vec<HistoryEntry>, bool) {
    let mut unavailable = false;
    let mut read = |path: &std::path::Path| match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            unavailable |= error.kind() != std::io::ErrorKind::NotFound;
            String::new()
        }
    };
    let legacy = read(path);
    let mut entries: Vec<HistoryEntry> = legacy
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| HistoryEntry::agent(line.to_owned()))
        .collect();
    {
        let records = read(&history_records_path(path, 1));
        entries.extend(
            records
                .lines()
                .filter_map(|line| serde_json::from_str::<String>(line).ok())
                .filter(|entry| !entry.trim().is_empty())
                .map(HistoryEntry::agent),
        );
    }
    {
        let records = read(&history_records_path(path, 2));
        entries.extend(
            records
                .lines()
                .filter_map(|line| serde_json::from_str::<HistoryEntry>(line).ok())
                .filter(|entry| !entry.body.trim().is_empty()),
        );
    }
    if entries.len() > limit {
        entries.drain(..entries.len() - limit);
    }
    if unavailable {
        tracing::warn!("composer history unavailable");
    }
    (entries, unavailable)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerKeyOutcome {
    Changed,
    Submit,
    Interrupt,
    Ignored,
}

#[cfg(test)]
#[path = "composer_history_tests.rs"]
mod history_tests;

#[cfg(test)]
#[path = "composer_tests.rs"]
mod tests;
