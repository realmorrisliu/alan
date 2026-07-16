use anyhow::{Result, anyhow};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ShellWordToken {
    pub(super) decoded: String,
    pub(super) raw_start: usize,
    pub(super) raw_end: usize,
}

pub(super) fn validate_shell_features(cmd: &str, backend_name: &str) -> Result<()> {
    let normalized = normalize_shell_line_continuations(cmd);
    let comment_free = strip_shell_comments(&normalized);
    if contains_shell_expansion(&comment_free)
        || contains_shell_brace_expansion(&comment_free)
        || contains_shell_globbing(&comment_free)
    {
        return Err(anyhow!(
            "Sandbox backend {} rejects shell variable, command, brace, or glob expansion because path references cannot be validated safely",
            backend_name
        ));
    }
    Ok(())
}

fn contains_shell_expansion(command: &str) -> bool {
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    for ch in command.chars() {
        if escaped {
            escaped = false;
            continue;
        }

        if in_single {
            if ch == '\'' {
                in_single = false;
            }
            continue;
        }

        if in_double {
            match ch {
                '\\' => escaped = true,
                '"' => in_double = false,
                '$' | '`' => return true,
                _ => {}
            }
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '\'' => in_single = true,
            '"' => in_double = true,
            '$' | '`' => return true,
            _ => {}
        }
    }

    false
}

fn contains_shell_brace_expansion(command: &str) -> bool {
    let chars: Vec<char> = command.chars().collect();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    for (index, ch) in chars.iter().copied().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }

        if in_single {
            if ch == '\'' {
                in_single = false;
            }
            continue;
        }

        if in_double {
            match ch {
                '\\' => escaped = true,
                '"' => in_double = false,
                _ => {}
            }
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '\'' => in_single = true,
            '"' => in_double = true,
            '{' | '}' if is_brace_expansion_position(&chars, index) => return true,
            _ => {}
        }
    }

    false
}

fn contains_shell_globbing(command: &str) -> bool {
    let chars: Vec<char> = command.chars().collect();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    for (index, ch) in chars.iter().copied().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }

        if in_single {
            if ch == '\'' {
                in_single = false;
            }
            continue;
        }

        if in_double {
            match ch {
                '\\' => escaped = true,
                '"' => in_double = false,
                _ => {}
            }
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '\'' => in_single = true,
            '"' => in_double = true,
            '*' | '?' => return true,
            '[' if !is_test_bracket_token(&chars, index) => return true,
            _ => {}
        }
    }

    false
}

fn is_test_bracket_token(chars: &[char], index: usize) -> bool {
    let mut end = index;
    while let Some(ch) = chars.get(end) {
        if ch.is_whitespace() || is_shell_separator(*ch) {
            break;
        }
        end += 1;
    }

    match end.saturating_sub(index) {
        1 => chars[index] == '[',
        2 => chars[index] == '[' && chars.get(index + 1).copied() == Some('['),
        _ => false,
    }
}

fn is_brace_expansion_position(chars: &[char], index: usize) -> bool {
    let prev = index.checked_sub(1).and_then(|i| chars.get(i)).copied();
    let next = chars.get(index + 1).copied();
    brace_neighbor_requires_expansion(prev) || brace_neighbor_requires_expansion(next)
}

fn brace_neighbor_requires_expansion(ch: Option<char>) -> bool {
    matches!(ch, Some(value) if !value.is_whitespace() && !is_shell_separator(value))
}

fn is_shell_separator(ch: char) -> bool {
    matches!(ch, ';' | '|' | '&' | '(' | ')' | '<' | '>')
}

fn is_shell_word_boundary(ch: char) -> bool {
    ch.is_whitespace() || is_shell_separator(ch) || matches!(ch, '{' | '}')
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

fn strip_shell_comments(command: &str) -> String {
    let mut stripped = String::with_capacity(command.len());
    let mut in_single = false;
    let mut in_double = false;
    let mut in_comment = false;
    let mut escaped = false;
    let mut word_started = false;

    for ch in command.chars() {
        if in_comment {
            if matches!(ch, '\n' | '\r') {
                stripped.push(ch);
                in_comment = false;
                word_started = false;
            }
            continue;
        }

        if escaped {
            stripped.push(ch);
            escaped = false;
            word_started = true;
            continue;
        }

        if in_single {
            if ch == '\'' {
                in_single = false;
            }
            stripped.push(ch);
            word_started = true;
            continue;
        }

        if in_double {
            match ch {
                '\\' => {
                    stripped.push(ch);
                    escaped = true;
                }
                '"' => {
                    in_double = false;
                    stripped.push(ch);
                    word_started = true;
                }
                _ => {
                    stripped.push(ch);
                    word_started = true;
                }
            }
            continue;
        }

        match ch {
            '\\' => {
                stripped.push(ch);
                escaped = true;
                word_started = true;
            }
            '\'' => {
                in_single = true;
                stripped.push(ch);
                word_started = true;
            }
            '"' => {
                in_double = true;
                stripped.push(ch);
                word_started = true;
            }
            '#' if !word_started => in_comment = true,
            c if is_shell_word_boundary(c) => {
                stripped.push(c);
                word_started = false;
            }
            _ => {
                stripped.push(ch);
                word_started = true;
            }
        }
    }

    stripped
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

pub(super) fn shell_word_tokens_with_spans(command: &str) -> Result<Vec<ShellWordToken>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = command.char_indices().peekable();
    let mut in_single = false;
    let mut in_double = false;
    let mut in_comment = false;
    let mut escaped = false;
    let mut word_started = false;
    let mut raw_start = None;

    while let Some((index, ch)) = chars.next() {
        if in_comment {
            if matches!(ch, '\n' | '\r') {
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
            } else {
                current.push(ch);
            }
            word_started = true;
            continue;
        }

        if in_double {
            match ch {
                '\\' => {
                    if let Some((_, next)) = chars.next() {
                        current.push(next);
                        word_started = true;
                    } else {
                        return Err(anyhow!("Command ends with an incomplete escape sequence"));
                    }
                }
                '"' => {
                    in_double = false;
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
                raw_start.get_or_insert(index);
                if let Some((_, next)) = chars.next() {
                    current.push(next);
                    word_started = true;
                } else {
                    return Err(anyhow!("Command ends with an incomplete escape sequence"));
                }
            }
            '\'' => {
                raw_start.get_or_insert(index);
                in_single = true;
                word_started = true;
            }
            '"' => {
                raw_start.get_or_insert(index);
                in_double = true;
                word_started = true;
            }
            '#' if !word_started => in_comment = true,
            c if c.is_whitespace() => {
                push_shell_word_token(&mut tokens, &mut current, &mut raw_start, index);
                word_started = false;
            }
            ';' | '(' | ')' | '{' | '}' => {
                push_shell_word_token(&mut tokens, &mut current, &mut raw_start, index);
                word_started = false;
            }
            '&' | '|' => {
                push_shell_word_token(&mut tokens, &mut current, &mut raw_start, index);

                if matches!(chars.peek(), Some((_, next)) if *next == ch) {
                    chars.next();
                }
                word_started = false;
            }
            '<' | '>' => {
                push_shell_word_token(&mut tokens, &mut current, &mut raw_start, index);

                match (ch, chars.peek().copied()) {
                    ('<', Some((_, '<' | '>' | '&'))) | ('>', Some((_, '>' | '&' | '|'))) => {
                        chars.next();
                        if ch == '<' && matches!(chars.peek(), Some((_, '-'))) {
                            chars.next();
                        }
                    }
                    _ => {}
                }
                word_started = false;
            }
            _ => {
                raw_start.get_or_insert(index);
                current.push(ch);
                word_started = true;
            }
        }
    }

    if escaped {
        return Err(anyhow!("Command ends with an incomplete escape sequence"));
    }
    if in_single || in_double {
        return Err(anyhow!("Command contains an unterminated quoted string"));
    }
    push_shell_word_token(&mut tokens, &mut current, &mut raw_start, command.len());

    Ok(tokens)
}

fn push_shell_word_token(
    tokens: &mut Vec<ShellWordToken>,
    current: &mut String,
    raw_start: &mut Option<usize>,
    raw_end: usize,
) {
    let Some(start) = raw_start.take() else {
        return;
    };
    tokens.push(ShellWordToken {
        decoded: std::mem::take(current),
        raw_start: start,
        raw_end,
    });
}

pub(super) fn shell_word_tokens(command: &str) -> Result<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = command.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    let mut in_comment = false;
    let mut escaped = false;
    let mut word_started = false;

    while let Some(ch) = chars.next() {
        if in_comment {
            if matches!(ch, '\n' | '\r') {
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
            } else {
                current.push(ch);
            }
            word_started = true;
            continue;
        }

        if in_double {
            match ch {
                '\\' => {
                    if let Some(next) = chars.next() {
                        current.push(next);
                        word_started = true;
                    } else {
                        return Err(anyhow!("Command ends with an incomplete escape sequence"));
                    }
                }
                '"' => {
                    in_double = false;
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
                if let Some(next) = chars.next() {
                    current.push(next);
                    word_started = true;
                } else {
                    return Err(anyhow!("Command ends with an incomplete escape sequence"));
                }
            }
            '\'' => {
                in_single = true;
                word_started = true;
            }
            '"' => {
                in_double = true;
                word_started = true;
            }
            '#' if !word_started => in_comment = true,
            c if c.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
                word_started = false;
            }
            ';' | '(' | ')' | '{' | '}' => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
                word_started = false;
            }
            '&' | '|' => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }

                if matches!(chars.peek(), Some(next) if *next == ch) {
                    chars.next();
                }
                word_started = false;
            }
            '<' | '>' => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }

                let mut operator = String::new();
                operator.push(ch);
                match (ch, chars.peek().copied()) {
                    ('<', Some('<')) => {
                        operator.push('<');
                        chars.next();
                        if matches!(chars.peek(), Some('-')) {
                            operator.push('-');
                            chars.next();
                        }
                    }
                    ('<', Some('>')) => {
                        operator.push('>');
                        chars.next();
                    }
                    ('<', Some('&')) => {
                        operator.push('&');
                        chars.next();
                    }
                    ('>', Some('>')) => {
                        operator.push('>');
                        chars.next();
                    }
                    ('>', Some('&')) => {
                        operator.push('&');
                        chars.next();
                    }
                    ('>', Some('|')) => {
                        operator.push('|');
                        chars.next();
                    }
                    _ => {}
                }
                tokens.push(operator);
                word_started = false;
            }
            _ => {
                current.push(ch);
                word_started = true;
            }
        }
    }

    if escaped {
        return Err(anyhow!("Command ends with an incomplete escape sequence"));
    }
    if in_single || in_double {
        return Err(anyhow!("Command contains an unterminated quoted string"));
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    Ok(tokens)
}

pub(super) fn shell_commands(command: &str) -> Result<Vec<Vec<String>>> {
    let mut commands = Vec::new();
    let mut current_command = Vec::new();
    let mut current_word = String::new();
    let mut chars = command.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    let mut in_comment = false;
    let mut escaped = false;
    let mut word_started = false;

    while let Some(ch) = chars.next() {
        if in_comment {
            if matches!(ch, '\n' | '\r') {
                if !current_word.is_empty() {
                    current_command.push(std::mem::take(&mut current_word));
                }
                if !current_command.is_empty() {
                    commands.push(std::mem::take(&mut current_command));
                }
                in_comment = false;
                word_started = false;
            }
            continue;
        }

        if escaped {
            current_word.push(ch);
            escaped = false;
            word_started = true;
            continue;
        }

        if in_single {
            if ch == '\'' {
                in_single = false;
            } else {
                current_word.push(ch);
            }
            word_started = true;
            continue;
        }

        if in_double {
            match ch {
                '\\' => {
                    if let Some(next) = chars.next() {
                        current_word.push(next);
                        word_started = true;
                    } else {
                        return Err(anyhow!("Command ends with an incomplete escape sequence"));
                    }
                }
                '"' => {
                    in_double = false;
                    word_started = true;
                }
                _ => {
                    current_word.push(ch);
                    word_started = true;
                }
            }
            continue;
        }

        match ch {
            '\\' => {
                if let Some(next) = chars.next() {
                    current_word.push(next);
                    word_started = true;
                } else {
                    return Err(anyhow!("Command ends with an incomplete escape sequence"));
                }
            }
            '\'' => {
                in_single = true;
                word_started = true;
            }
            '"' => {
                in_double = true;
                word_started = true;
            }
            '#' if !word_started => in_comment = true,
            '\n' | '\r' => {
                if !current_word.is_empty() {
                    current_command.push(std::mem::take(&mut current_word));
                }
                if !current_command.is_empty() {
                    commands.push(std::mem::take(&mut current_command));
                }
                word_started = false;
            }
            c if c.is_whitespace() => {
                if !current_word.is_empty() {
                    current_command.push(std::mem::take(&mut current_word));
                }
                word_started = false;
            }
            ';' | '|' | '&' | '(' | ')' | '{' | '}' => {
                if !current_word.is_empty() {
                    current_command.push(std::mem::take(&mut current_word));
                }
                if !current_command.is_empty() {
                    commands.push(std::mem::take(&mut current_command));
                }
                if matches!(chars.peek(), Some(next) if *next == ch && matches!(ch, '|' | '&')) {
                    chars.next();
                }
                word_started = false;
            }
            _ => {
                current_word.push(ch);
                word_started = true;
            }
        }
    }

    if escaped {
        return Err(anyhow!("Command ends with an incomplete escape sequence"));
    }
    if in_single || in_double {
        return Err(anyhow!("Command contains an unterminated quoted string"));
    }
    if !current_word.is_empty() {
        current_command.push(current_word);
    }
    if !current_command.is_empty() {
        commands.push(current_command);
    }

    Ok(commands)
}
