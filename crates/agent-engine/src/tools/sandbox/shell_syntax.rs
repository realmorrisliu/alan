use anyhow::{Result, anyhow};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ShellToken {
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
        if matches!(ch, ' ' | '\t' | '\n') || is_shell_separator(*ch) {
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
    matches!(ch, Some(value) if !matches!(value, ' ' | '\t' | '\n') && !is_shell_separator(value))
}

fn is_shell_separator(ch: char) -> bool {
    matches!(ch, ';' | '|' | '&' | '(' | ')' | '<' | '>')
}

fn is_shell_word_boundary(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n') || is_shell_separator(ch) || matches!(ch, '{' | '}')
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

fn push_double_quoted_escape(word: &mut String, next: char) {
    if next == '\n' {
        return;
    }
    if !matches!(next, '$' | '`' | '"' | '\\') {
        word.push('\\');
    }
    word.push(next);
}

pub(super) fn shell_tokens_with_spans(command: &str) -> Result<Vec<ShellToken>> {
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
                        push_double_quoted_escape(&mut current, next);
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
            ' ' | '\t' | '\n' => {
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
                let mut operator = String::from(ch);
                let mut operator_end = index + ch.len_utf8();
                match (ch, chars.peek().copied()) {
                    ('<', Some((_, '<' | '>' | '&'))) | ('>', Some((_, '>' | '&' | '|'))) => {
                        if let Some((operator_index, operator_char)) = chars.next() {
                            operator.push(operator_char);
                            operator_end = operator_index + operator_char.len_utf8();
                        }
                        if operator == "<<"
                            && matches!(chars.peek(), Some((_, '-')))
                            && let Some((operator_index, operator_char)) = chars.next()
                        {
                            operator.push(operator_char);
                            operator_end = operator_index + operator_char.len_utf8();
                        }
                    }
                    _ => {}
                }
                tokens.push(ShellToken {
                    decoded: operator,
                    raw_start: index,
                    raw_end: operator_end,
                });
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
    tokens: &mut Vec<ShellToken>,
    current: &mut String,
    raw_start: &mut Option<usize>,
    raw_end: usize,
) {
    let Some(start) = raw_start.take() else {
        return;
    };
    tokens.push(ShellToken {
        decoded: std::mem::take(current),
        raw_start: start,
        raw_end,
    });
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
                        push_double_quoted_escape(&mut current_word, next);
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
            ' ' | '\t' => {
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

pub(crate) fn parse_standalone_cd(command: &str) -> Result<Option<PathBuf>> {
    // Parse syntax only: this never invokes a shell or evaluates substitutions.
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_bash::LANGUAGE.into())?;
    let tree = parser
        .parse(command, None)
        .ok_or_else(|| anyhow!("shell syntax parsing failed"))?;
    let root = tree.root_node();
    if root.has_error() {
        return Ok(None); // Native execution reports malformed shell syntax.
    }
    let mut cursor = root.walk();
    let statements = root
        .children(&mut cursor)
        .filter(|node| node.kind() != "comment")
        .collect::<Vec<_>>();
    let [statement] = statements.as_slice() else {
        return Ok(None);
    };
    if statement.kind() != "command" {
        return Ok(None);
    }
    let Some(name) = statement.child_by_field_name("name") else {
        return Ok(None);
    };
    let name = shell_tokens_with_spans(&command[name.byte_range()])?;
    if name.len() != 1 || name[0].decoded != "cd" {
        return Ok(None);
    }
    let mut cursor = statement.walk();
    if statement
        .named_children(&mut cursor)
        .any(|node| node.kind() == "variable_assignment")
    {
        return Ok(None);
    }
    // Expansions are rejected before decoding words; nested syntax cannot turn
    // a standalone cd into an ordinary native script with hidden side effects.
    let normalized = normalize_shell_line_continuations(command);
    let comment_free = strip_shell_comments(&normalized);
    if contains_shell_expansion(&comment_free)
        || contains_shell_brace_expansion(&comment_free)
        || contains_shell_globbing(&comment_free)
        || statement
            .named_children(&mut statement.walk())
            .any(|node| node.kind() == "process_substitution")
    {
        return Err(anyhow!(
            "standalone cd accepts a literal path; variables, substitutions, and globs are unsupported"
        ));
    }
    let words = shell_tokens_with_spans(&normalized)?;
    if words.len() != 2 {
        return Err(anyhow!(
            "standalone cd requires exactly one directory argument"
        ));
    }
    let directory = &words[1].decoded;
    if directory.is_empty() {
        return Err(anyhow!(
            "standalone cd requires a non-empty directory argument"
        ));
    }
    if directory.starts_with('-') {
        return Err(anyhow!("standalone cd does not support options or `-`"));
    }
    if normalized[words[1].raw_start..words[1].raw_end].starts_with('~') {
        return Err(anyhow!(
            "standalone cd does not support home-directory expansion"
        ));
    }
    Ok(Some(PathBuf::from(directory)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_shell_readers_preserve_double_quoted_backslashes() {
        let script = r#"printf "foo\bar" "foo\\bar" "foo\$bar""#;
        let expected = vec!["printf", r"foo\bar", r"foo\bar", "foo$bar"];
        assert_eq!(shell_commands(script).unwrap()[0], expected);
        assert_eq!(
            shell_tokens_with_spans(script)
                .unwrap()
                .iter()
                .map(|token| token.decoded.as_str())
                .collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn redirection_dash_belongs_to_the_target_except_for_stripped_heredocs() {
        for (operator, target) in [("<>", "-/mnt/project/file"), ("<&", "-"), ("<<-", "EOF")] {
            let script = format!("cat {operator}{target}");
            let tokens = shell_tokens_with_spans(&script).unwrap();
            assert_eq!(
                tokens
                    .iter()
                    .map(|token| token.decoded.as_str())
                    .collect::<Vec<_>>(),
                ["cat", operator, target]
            );
            assert_eq!(&script[tokens[2].raw_start..tokens[2].raw_end], target);
        }
    }

    #[test]
    fn tokens_keep_empty_words_unicode_and_redirection_spans() {
        let script = "printf '' '中 文' 2>out # ignored\ncat <out";
        let tokens = shell_tokens_with_spans(script).unwrap();
        assert_eq!(
            tokens
                .iter()
                .map(|token| token.decoded.as_str())
                .collect::<Vec<_>>(),
            ["printf", "", "中 文", "2", ">", "out", "cat", "<", "out"]
        );
        assert_eq!(
            tokens
                .iter()
                .map(|token| &script[token.raw_start..token.raw_end])
                .collect::<Vec<_>>(),
            [
                "printf",
                "''",
                "'中 文'",
                "2",
                ">",
                "out",
                "cat",
                "<",
                "out"
            ]
        );
    }
}
