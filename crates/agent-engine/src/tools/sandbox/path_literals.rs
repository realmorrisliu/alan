use super::super::reified_namespace::ReifiedNamespacePlan;
use super::command_wrappers::is_env_assignment;
use super::path_safety::PROTECTED_SUBPATHS;
use super::shell_syntax::{ShellWordToken, shell_word_tokens_with_spans};
use std::ops::Range;
use std::path::{Component, Path, PathBuf};

pub(super) fn translate_reified_shell_token(
    token: &str,
    plan: &ReifiedNamespacePlan,
) -> Option<String> {
    if let Some(rewritten) = translate_reified_nested_shell_token(token, plan) {
        return Some(shell_quote_token(&rewritten));
    }

    let mut replacements = Vec::new();
    for range in reified_shell_token_path_candidate_ranges(token) {
        if replacements
            .iter()
            .any(|(existing, _)| ranges_overlap(existing, &range))
        {
            continue;
        }

        let candidate = &token[range.clone()];
        let candidate_path = Path::new(candidate);
        if !candidate_path.is_absolute() || is_allowed_absolute_command_path(candidate_path) {
            continue;
        }

        let Some(namespace_path) =
            plan.translate_projected_host_path(candidate_path)
                .or_else(|| {
                    plan.translate_projected_host_path(&lexically_normalize_path(candidate_path))
                })
        else {
            continue;
        };
        replacements.push((range, namespace_path.display().to_string()));
    }

    if replacements.is_empty() {
        return None;
    }

    replacements.sort_by_key(|(range, _)| range.start);
    let replacement_len = replacements
        .iter()
        .map(|(_, replacement)| replacement.len())
        .sum::<usize>();
    let mut rewritten = String::with_capacity(token.len() + replacement_len);
    let mut last = 0;
    for (range, replacement) in replacements {
        rewritten.push_str(&token[last..range.start]);
        rewritten.push_str(&replacement);
        last = range.end;
    }
    rewritten.push_str(&token[last..]);

    Some(shell_quote_reified_token(&rewritten))
}

fn translate_reified_nested_shell_token(
    token: &str,
    plan: &ReifiedNamespacePlan,
) -> Option<String> {
    let tokens = shell_word_tokens_with_spans(token).ok()?;
    if !looks_like_nested_shell_script(&tokens) {
        return None;
    }

    let mut translated = String::with_capacity(token.len());
    let mut last = 0;
    let mut changed = false;
    for nested_token in tokens {
        let Some(rewritten) = translate_reified_shell_token(&nested_token.decoded, plan) else {
            continue;
        };
        translated.push_str(&token[last..nested_token.raw_start]);
        translated.push_str(&rewritten);
        last = nested_token.raw_end;
        changed = true;
    }
    if !changed {
        return None;
    }

    translated.push_str(&token[last..]);
    Some(translated)
}

fn looks_like_nested_shell_script(tokens: &[ShellWordToken]) -> bool {
    if tokens.len() < 2 {
        return false;
    }
    let Some(command) = tokens
        .iter()
        .find(|token| !is_env_assignment(&token.decoded))
    else {
        return false;
    };

    !looks_like_path_token(&command.decoded)
        && !looks_like_bare_protected_subpath_token(&command.decoded)
}

fn shell_quote_reified_token(token: &str) -> String {
    if is_env_assignment(token) {
        let (name, value) = token
            .split_once('=')
            .expect("is_env_assignment requires an equals sign");
        return format!("{name}={}", shell_quote_token(value));
    }
    shell_quote_token(token)
}

fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

fn reified_shell_token_path_candidate_ranges(token: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    for range in colon_separated_absolute_path_component_ranges(token) {
        push_unique_range(&mut ranges, range);
    }
    for range in path_like_subtoken_ranges(token) {
        push_unique_range(&mut ranges, range);
    }
    for range in embedded_absolute_path_literal_ranges(token) {
        push_path_literal_candidate_ranges(token, range, &mut ranges);
    }
    ranges
}

fn push_path_literal_candidate_ranges(
    token: &str,
    range: Range<usize>,
    ranges: &mut Vec<Range<usize>>,
) {
    if let Some(split_end) = first_later_absolute_path_split(token, &range) {
        let first_operand = trim_trailing_whitespace_range(token, range.start..split_end);
        if first_operand.start < first_operand.end {
            if path_literal_range_contains_flag_segment(token, &first_operand) {
                push_whitespace_prefix_ranges(token, first_operand.clone(), ranges);
                push_unique_range(ranges, first_operand);
            } else {
                push_unique_range(ranges, first_operand.clone());
                push_whitespace_prefix_ranges(token, first_operand, ranges);
            }
        }
        return;
    }

    let trimmed = trim_trailing_whitespace_range(token, range.clone());
    if trimmed.start < trimmed.end {
        push_unique_range(ranges, trimmed);
    }

    let literal = &token[range.clone()];
    for (offset, ch) in literal.char_indices() {
        if ch.is_whitespace() && offset > 0 {
            let prefix = range.start..range.start + offset;
            push_unique_range(ranges, prefix);
        }
    }
}

fn push_whitespace_prefix_ranges(token: &str, range: Range<usize>, ranges: &mut Vec<Range<usize>>) {
    let literal = &token[range.clone()];
    for (offset, ch) in literal.char_indices() {
        if ch.is_whitespace() && offset > 0 {
            push_unique_range(ranges, range.start..range.start + offset);
        }
    }
}

fn path_literal_range_contains_flag_segment(token: &str, range: &Range<usize>) -> bool {
    token[range.clone()]
        .split_whitespace()
        .skip(1)
        .any(|segment| segment.starts_with('-'))
}

fn first_later_absolute_path_split(token: &str, range: &Range<usize>) -> Option<usize> {
    let literal = &token[range.clone()];
    let mut whitespace_start = None;
    let mut in_whitespace = false;
    for (offset, ch) in literal.char_indices().skip(1) {
        if ch.is_whitespace() {
            if !in_whitespace {
                whitespace_start = Some(offset);
                in_whitespace = true;
            }
            continue;
        }

        if ch == '/' && !absolute_path_match_has_path_prefix(literal, offset) {
            return whitespace_start.map(|split| range.start + split);
        }

        whitespace_start = None;
        in_whitespace = false;
    }
    None
}

fn trim_trailing_whitespace_range(token: &str, range: Range<usize>) -> Range<usize> {
    let trimmed = token[range.clone()].trim_end_matches(char::is_whitespace);
    range.start..range.start + trimmed.len()
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

fn shell_quote_token(token: &str) -> String {
    if !token.is_empty()
        && token.chars().all(|ch| {
            ch.is_ascii_alphanumeric()
                || matches!(
                    ch,
                    '@' | '%' | '_' | '+' | '=' | ':' | ',' | '.' | '/' | '-'
                )
        })
    {
        return token.to_string();
    }

    let mut quoted = String::from("'");
    for ch in token.chars() {
        if ch == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
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
