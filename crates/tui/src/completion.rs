//! Trigger-driven completion shared by `/` client commands, `$` skill
//! references, and `@` file references.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    Command,
    Skill,
    File,
}

impl CompletionKind {
    fn trigger(self) -> char {
        match self {
            CompletionKind::Command => '/',
            CompletionKind::Skill => '$',
            CompletionKind::File => '@',
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionCandidate {
    pub value: String,
    pub label: String,
    pub detail: Option<String>,
}

impl CompletionCandidate {
    pub fn new(value: impl Into<String>, detail: Option<String>) -> Self {
        let value = value.into();
        Self {
            label: value.clone(),
            value,
            detail,
        }
    }
}

/// Candidate sources the active composer can complete against.
#[derive(Debug, Default, Clone)]
pub struct CompletionSources {
    pub commands: Vec<CompletionCandidate>,
    pub skills: Vec<CompletionCandidate>,
    pub files: Vec<CompletionCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionState {
    pub kind: CompletionKind,
    pub token_start: usize,
    pub query: String,
    pub matches: Vec<CompletionCandidate>,
    pub selected: usize,
}

impl CompletionState {
    pub fn selected_candidate(&self) -> Option<&CompletionCandidate> {
        self.matches.get(self.selected)
    }

    pub fn move_down(&mut self) {
        if !self.matches.is_empty() {
            self.selected = (self.selected + 1) % self.matches.len();
        }
    }

    pub fn move_up(&mut self) {
        if !self.matches.is_empty() {
            self.selected = (self.selected + self.matches.len() - 1) % self.matches.len();
        }
    }
}

/// Detect an active completion for the composer text and cursor position.
pub fn detect(text: &str, cursor: usize, sources: &CompletionSources) -> Option<CompletionState> {
    let cursor = cursor.min(text.len());
    let before = &text[..cursor];
    let word_start = before
        .char_indices()
        .rev()
        .find(|(_, ch)| ch.is_whitespace())
        .map(|(idx, ch)| idx + ch.len_utf8())
        .unwrap_or(0);

    let trigger = before[word_start..].chars().next()?;
    let kind = match trigger {
        '/' if word_start == 0 => CompletionKind::Command,
        '$' => CompletionKind::Skill,
        '@' => CompletionKind::File,
        _ => return None,
    };

    let query = &before[word_start + trigger.len_utf8()..];
    let pool = match kind {
        CompletionKind::Command => &sources.commands,
        CompletionKind::Skill => &sources.skills,
        CompletionKind::File => &sources.files,
    };

    let matches = filter(pool, query);
    if matches.is_empty() {
        return None;
    }

    Some(CompletionState {
        kind,
        token_start: word_start,
        query: query.to_string(),
        matches,
        selected: 0,
    })
}

fn filter(pool: &[CompletionCandidate], query: &str) -> Vec<CompletionCandidate> {
    let needle = query.to_ascii_lowercase();
    pool.iter()
        .filter(|candidate| {
            needle.is_empty() || candidate.value.to_ascii_lowercase().contains(&needle)
        })
        .cloned()
        .collect()
}

/// Apply a chosen candidate, returning the new text and cursor position.
pub fn apply(text: &str, state: &CompletionState, value: &str) -> (String, usize) {
    let trigger = state.kind.trigger();
    let prefix = &text[..state.token_start.min(text.len())];
    let suffix_start = state.token_start + trigger.len_utf8() + state.query.len();
    let suffix = text.get(suffix_start.min(text.len())..).unwrap_or("");
    let inserted = format!("{trigger}{value} ");
    let new_cursor = prefix.len() + inserted.len();
    (format!("{prefix}{inserted}{suffix}"), new_cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sources() -> CompletionSources {
        CompletionSources {
            commands: vec![
                CompletionCandidate::new("compact", Some("summarize context".into())),
                CompletionCandidate::new("clear", None),
            ],
            skills: vec![
                CompletionCandidate::new("code-review", Some("review a diff".into())),
                CompletionCandidate::new("commit", None),
            ],
            files: vec![CompletionCandidate::new("src/main.rs", None)],
        }
    }

    #[test]
    fn slash_triggers_command_only_at_start() {
        let state = detect("/co", 3, &sources()).expect("command completion");
        assert_eq!(state.kind, CompletionKind::Command);
        assert_eq!(state.matches.len(), 1);
        assert_eq!(state.matches[0].value, "compact");
        // Not at start -> no command completion.
        assert!(detect("hi /co", 6, &sources()).is_none());
    }

    #[test]
    fn dollar_triggers_skill_inline() {
        let state = detect("review with $co", 15, &sources()).expect("skill completion");
        assert_eq!(state.kind, CompletionKind::Skill);
        let values: Vec<_> = state.matches.iter().map(|c| c.value.as_str()).collect();
        assert_eq!(values, vec!["code-review", "commit"]);
    }

    #[test]
    fn at_triggers_file_inline() {
        let state = detect("open @src", 9, &sources()).expect("file completion");
        assert_eq!(state.kind, CompletionKind::File);
        assert_eq!(state.matches[0].value, "src/main.rs");
    }

    #[test]
    fn empty_pool_degrades_to_no_popup() {
        let mut sources = sources();
        sources.skills.clear();
        assert!(detect("use $foo", 8, &sources).is_none());
    }

    #[test]
    fn apply_replaces_token_inline() {
        let text = "review with $co";
        let state = detect(text, text.len(), &sources()).unwrap();
        let (new_text, cursor) = apply(text, &state, "code-review");
        assert_eq!(new_text, "review with $code-review ");
        assert_eq!(cursor, new_text.len());
    }

    #[test]
    fn apply_preserves_suffix() {
        let text = "a @src end";
        // cursor right after "@src"
        let state = detect(text, 6, &sources()).unwrap();
        let (new_text, _) = apply(text, &state, "src/main.rs");
        assert_eq!(new_text, "a @src/main.rs  end");
    }
}
