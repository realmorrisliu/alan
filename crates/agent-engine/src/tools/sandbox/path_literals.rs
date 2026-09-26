use super::path_safety::PROTECTED_SUBPATHS;
use super::shell_syntax::{ShellToken, shell_commands};
use std::ops::Range;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TokenPathRole {
    Check,
    Data,
    ExecutableData,
}

pub(super) fn token_path_role(command: &str, token: &ShellToken) -> TokenPathRole {
    let prefix = command[..token.raw_start].trim_end();
    if prefix.ends_with('>') || prefix.ends_with('<') {
        return TokenPathRole::Check;
    }

    let Ok(commands) = shell_commands(&command[..token.raw_start]) else {
        return TokenPathRole::Check;
    };
    let Some(words) = commands.last() else {
        return TokenPathRole::Check;
    };
    let Some((command_name, args)) = super::command_wrappers::command_and_args(words) else {
        return TokenPathRole::Check;
    };
    let command_name = Path::new(command_name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(command_name);

    if matches!(command_name, "awk" | "gawk" | "mawk" | "nawk") {
        return match super::command_interpreters::awk_next_argument_role(args, &token.decoded) {
            super::command_interpreters::AwkArgumentRole::Program => TokenPathRole::ExecutableData,
            super::command_interpreters::AwkArgumentRole::Data => TokenPathRole::Data,
            super::command_interpreters::AwkArgumentRole::Operand => TokenPathRole::Check,
        };
    }

    if matches!(command_name, "echo" | "printf") {
        return TokenPathRole::Data;
    }

    // ponytail: recognize only known literal-data operands; add broader shell
    // argument semantics only if real commands are blocked by this ceiling.
    let git_commit = command_name == "git" && args.iter().any(|word| word == "commit");
    let git_commit_message = git_commit
        && (matches!(args.last().map(String::as_str), Some("-m" | "--message"))
            || token.decoded.starts_with("--message=")
            || token.decoded.starts_with("-m"));
    if git_commit_message {
        TokenPathRole::Data
    } else {
        TokenPathRole::Check
    }
}

fn push_unique_range(ranges: &mut Vec<Range<usize>>, range: Range<usize>) {
    if !ranges.contains(&range) {
        ranges.push(range);
    }
}

fn path_like_subtoken_ranges(token: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    if looks_like_path_token(token) || looks_like_bare_protected_subpath_token(token) {
        ranges.push(0..token.len());
    }
    if let Some(index) = token.rfind('=') {
        let start = index + 1;
        if start < token.len() {
            ranges.push(start..token.len());
        }
    }
    if let Some(range) = short_option_attached_path_subtoken_range(token)
        && !ranges.contains(&range)
    {
        ranges.push(range);
    }
    ranges
}

fn colon_separated_absolute_path_component_ranges(token: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    if token.starts_with('/') {
        push_colon_separated_absolute_path_components(token, 0..token.len(), &mut ranges);
    }
    if let Some(index) = token.rfind('=') {
        let start = index + 1;
        if start < token.len() {
            push_colon_separated_absolute_path_components(token, start..token.len(), &mut ranges);
        }
    }
    ranges
}

fn push_colon_separated_absolute_path_components(
    token: &str,
    range: Range<usize>,
    ranges: &mut Vec<Range<usize>>,
) {
    let value = &token[range.clone()];
    let mut component_start = range.start;
    for (offset, ch) in value.char_indices() {
        if ch != ':' {
            continue;
        }
        push_absolute_path_component_range(token, component_start..range.start + offset, ranges);
        component_start = range.start + offset + ch.len_utf8();
    }
    push_absolute_path_component_range(token, component_start..range.end, ranges);
}

fn push_absolute_path_component_range(
    token: &str,
    range: Range<usize>,
    ranges: &mut Vec<Range<usize>>,
) {
    if range.start >= range.end {
        return;
    }
    if token[range.clone()].starts_with('/') {
        push_unique_range(ranges, range);
    }
}

pub(super) fn absolute_path_literal_candidates(token: &str) -> Vec<Vec<String>> {
    let mut literals = Vec::new();
    for range in colon_separated_absolute_path_component_ranges(token) {
        push_absolute_path_literal_candidates(token, range, &mut literals);
    }
    for range in path_like_subtoken_ranges(token) {
        push_absolute_path_literal_candidates(token, range, &mut literals);
    }

    for range in embedded_absolute_path_literal_ranges(token) {
        push_absolute_path_literal_candidates(token, range, &mut literals);
    }

    literals
}

pub(super) fn quoted_absolute_path_literal_candidates(token: &str) -> Vec<Vec<String>> {
    let mut literals = Vec::new();
    let mut string_start = None;
    let mut escaped = false;
    for (index, ch) in token.char_indices() {
        if let Some(start) = string_start {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                literals.extend(absolute_path_literal_candidates(&token[start..index]));
                string_start = None;
            }
        } else if ch == '"' {
            string_start = Some(index + ch.len_utf8());
        }
    }
    literals
}

fn push_absolute_path_literal_candidates(
    token: &str,
    range: Range<usize>,
    literals: &mut Vec<Vec<String>>,
) {
    let literal = &token[range];
    if !Path::new(literal).is_absolute() {
        return;
    }

    let mut candidates = vec![literal.to_string()];
    for (offset, ch) in literal.char_indices() {
        if ch.is_whitespace() && offset > 0 {
            let prefix = literal[..offset].to_string();
            if !candidates.contains(&prefix) {
                candidates.push(prefix);
            }
        }
    }
    if !literals.iter().any(|existing| existing == &candidates) {
        literals.push(candidates);
    }
}

fn embedded_absolute_path_literal_ranges(token: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let indices = token.char_indices().collect::<Vec<_>>();
    for (position, &(start, ch)) in indices.iter().enumerate() {
        if ch != '/' || absolute_path_match_has_path_prefix(token, start) {
            continue;
        }

        let mut end = token.len();
        for &(index, next) in &indices[position + 1..] {
            if is_absolute_path_literal_terminator(next) {
                end = index;
                break;
            }
        }
        ranges.push(start..end);
    }
    ranges
}

fn absolute_path_match_has_path_prefix(text: &str, start: usize) -> bool {
    if start == 0 {
        return false;
    }
    let prev = text.as_bytes()[start - 1];
    prev == b':'
        || prev == b'.'
        || prev == b'/'
        || prev == b'_'
        || prev == b'-'
        || prev == b'*'
        || prev == b'?'
        || prev == b']'
        || prev.is_ascii_alphanumeric()
}

fn is_absolute_path_literal_terminator(ch: char) -> bool {
    matches!(
        ch,
        '"' | '\'' | '`' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '|' | '&' | ';' | ','
    )
}

pub(super) fn looks_like_path_token(token: &str) -> bool {
    token.starts_with('/')
        || token.starts_with("./")
        || token.starts_with("../")
        || token == "."
        || token == ".."
        || token.contains('/')
}

pub(super) fn looks_like_bare_protected_subpath_token(token: &str) -> bool {
    PROTECTED_SUBPATHS
        .iter()
        .copied()
        .any(|protected| token.trim_end_matches('/') == protected)
}

pub(super) fn path_like_subtokens(token: &str) -> Vec<&str> {
    let mut candidates = vec![token];
    if let Some((_, rhs)) = token.rsplit_once('=')
        && !rhs.is_empty()
    {
        candidates.push(rhs);
    }
    if let Some(attached) = short_option_attached_path_subtoken(token)
        && !candidates.contains(&attached)
    {
        candidates.push(attached);
    }
    candidates
}

fn short_option_attached_path_subtoken(token: &str) -> Option<&str> {
    let range = short_option_attached_path_subtoken_range(token)?;
    Some(&token[range])
}

fn short_option_attached_path_subtoken_range(token: &str) -> Option<Range<usize>> {
    if token.starts_with("--") {
        return None;
    }
    let rest = token.strip_prefix('-')?;
    if rest.len() < 2 {
        return None;
    }

    rest.char_indices().skip(1).find_map(|(index, _)| {
        let candidate = &rest[index..];
        if candidate.starts_with('~')
            || looks_like_path_token(candidate)
            || looks_like_bare_protected_subpath_token(candidate)
        {
            Some((index + 1)..token.len())
        } else {
            None
        }
    })
}

pub(super) fn is_file_redirection_operator(token: &str) -> bool {
    matches!(token, "<" | ">" | ">>" | "<>" | ">|")
}

pub(super) fn is_allowed_absolute_command_path(path: &Path) -> bool {
    matches!(
        path.to_str(),
        Some("/dev/null" | "/dev/stdin" | "/dev/stdout" | "/dev/stderr")
    )
}

pub(super) fn lexically_normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}
