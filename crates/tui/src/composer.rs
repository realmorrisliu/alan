use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Composer {
    buffer: String,
    cursor: usize,
}

impl Composer {
    pub fn text(&self) -> &str {
        &self.buffer
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.buffer = text.into();
        self.cursor = self.buffer.len();
    }

    pub fn insert_text(&mut self, text: &str) {
        self.buffer.insert_str(self.cursor, text);
        self.cursor += text.len();
    }

    pub fn take_submit(&mut self) -> Option<String> {
        let text = self.buffer.trim().to_string();
        if text.is_empty() {
            return None;
        }
        self.buffer.clear();
        self.cursor = 0;
        Some(text)
    }

    pub fn handle_key(&mut self, event: KeyEvent) -> ComposerKeyOutcome {
        match event.code {
            KeyCode::Char('c') if event.modifiers.contains(KeyModifiers::CONTROL) => {
                ComposerKeyOutcome::Interrupt
            }
            KeyCode::Char(ch) => {
                self.insert_text(&ch.to_string());
                ComposerKeyOutcome::Changed
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    let prev = self.buffer[..self.cursor]
                        .char_indices()
                        .last()
                        .map(|(idx, _)| idx)
                        .unwrap_or(0);
                    self.buffer.drain(prev..self.cursor);
                    self.cursor = prev;
                    ComposerKeyOutcome::Changed
                } else {
                    ComposerKeyOutcome::Ignored
                }
            }
            KeyCode::Left => {
                if self.cursor > 0 {
                    self.cursor = self.buffer[..self.cursor]
                        .char_indices()
                        .last()
                        .map(|(idx, _)| idx)
                        .unwrap_or(0);
                    ComposerKeyOutcome::Changed
                } else {
                    ComposerKeyOutcome::Ignored
                }
            }
            KeyCode::Right => {
                if self.cursor < self.buffer.len() {
                    self.cursor += self.buffer[self.cursor..]
                        .chars()
                        .next()
                        .map(char::len_utf8)
                        .unwrap_or(0);
                    ComposerKeyOutcome::Changed
                } else {
                    ComposerKeyOutcome::Ignored
                }
            }
            KeyCode::Enter if event.modifiers.contains(KeyModifiers::SHIFT) => {
                self.buffer.insert(self.cursor, '\n');
                self.cursor += 1;
                ComposerKeyOutcome::Changed
            }
            KeyCode::Enter => ComposerKeyOutcome::Submit,
            _ => ComposerKeyOutcome::Ignored,
        }
    }
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
}
