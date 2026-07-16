fn is_shell_word_boundary(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, ';' | '|' | '&' | '(' | ')' | '<' | '>' | '{' | '}')
}

pub(super) fn normalize_shell_line_continuations(command: &str) -> String {
    let mut normalized = String::with_capacity(command.len());
    let mut chars = command.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    let mut in_comment = false;
    let mut escaped = false;
    let mut word_started = false;

    while let Some(ch) = chars.next() {
        if in_comment {
            normalized.push(ch);
            if matches!(ch, '\n' | '\r') {
                in_comment = false;
                word_started = false;
            }
            continue;
        }

        if escaped {
            normalized.push(ch);
            escaped = false;
            word_started = true;
            continue;
        }

        if in_single {
            if ch == '\'' {
                in_single = false;
            }
            normalized.push(ch);
            word_started = true;
            continue;
        }

        if in_double {
            match ch {
                '\\' => {
                    if consume_shell_line_continuation(&mut chars) {
                        continue;
                    }
                    normalized.push(ch);
                    escaped = true;
                }
                '"' => {
                    in_double = false;
                    normalized.push(ch);
                    word_started = true;
                }
                _ => {
                    normalized.push(ch);
                    word_started = true;
                }
            }
            continue;
        }

        match ch {
            '\\' => {
                if consume_shell_line_continuation(&mut chars) {
                    continue;
                }
                normalized.push(ch);
                escaped = true;
                word_started = true;
            }
            '\'' => {
                in_single = true;
                normalized.push(ch);
                word_started = true;
            }
            '"' => {
                in_double = true;
                normalized.push(ch);
                word_started = true;
            }
            '#' if !word_started => {
                in_comment = true;
                normalized.push(ch);
            }
            c if is_shell_word_boundary(c) => {
                normalized.push(c);
                word_started = false;
            }
            _ => {
                normalized.push(ch);
                word_started = true;
            }
        }
    }

    normalized
}

pub(super) fn split_shell_fragments(command: &str) -> Vec<String> {
    let mut fragments = Vec::new();
    let mut current = String::with_capacity(command.len());
    let mut chars = command.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    let mut in_comment = false;
    let mut escaped = false;
    let mut word_started = false;

    while let Some(ch) = chars.next() {
        if in_comment {
            if matches!(ch, '\n' | '\r') {
                push_shell_fragment(&mut fragments, &mut current);
                in_comment = false;
                word_started = false;
            }
            continue;
        }

        if escaped {
            current.push(ch);
            escaped = false;
            word_started = true;
            continue;
        }

        if in_single {
            if ch == '\'' {
                in_single = false;
            }
            current.push(ch);
            word_started = true;
            continue;
        }

        if in_double {
            match ch {
                '\\' => {
                    current.push(ch);
                    escaped = true;
                }
                '"' => {
                    in_double = false;
                    current.push(ch);
                    word_started = true;
                }
                _ => {
                    current.push(ch);
                    word_started = true;
                }
            }
            continue;
        }

        match ch {
            '\\' => {
                current.push(ch);
                escaped = true;
                word_started = true;
            }
            '\'' => {
                in_single = true;
                current.push(ch);
                word_started = true;
            }
            '"' => {
                in_double = true;
                current.push(ch);
                word_started = true;
            }
            '#' if !word_started => {
                in_comment = true;
            }
            '&' if matches!(chars.peek(), Some('&')) => {
                chars.next();
                push_shell_fragment(&mut fragments, &mut current);
                word_started = false;
            }
            '|' if matches!(chars.peek(), Some('|')) => {
                chars.next();
                push_shell_fragment(&mut fragments, &mut current);
                word_started = false;
            }
            ';' | '\n' | '\r' | '|' => {
                push_shell_fragment(&mut fragments, &mut current);
                word_started = false;
            }
            c if is_shell_word_boundary(c) => {
                current.push(c);
                word_started = false;
            }
            _ => {
                current.push(ch);
                word_started = true;
            }
        }
    }

    push_shell_fragment(&mut fragments, &mut current);
    fragments
}

fn push_shell_fragment(fragments: &mut Vec<String>, current: &mut String) {
    if current.trim().is_empty() {
        current.clear();
        return;
    }

    fragments.push(std::mem::take(current));
}

fn consume_shell_line_continuation<I>(chars: &mut std::iter::Peekable<I>) -> bool
where
    I: Iterator<Item = char>,
{
    match chars.peek().copied() {
        Some('\n') => {
            chars.next();
            true
        }
        Some('\r') => {
            chars.next();
            if matches!(chars.peek(), Some('\n')) {
                chars.next();
            }
            true
        }
        _ => false,
    }
}
