use super::path_safety::PROTECTED_SUBPATHS;
use super::shell_syntax::{ShellToken, shell_commands};
use alan_agent_protocol::ToolCapability;
use std::ops::Range;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TokenPathRole {
    Check,
    Read,
    Data,
    InlineCode,
    ExecutableData(usize),
}

impl TokenPathRole {
    pub(super) fn capability(
        self,
        command: Option<ToolCapability>,
        os_enforced: bool,
    ) -> Option<ToolCapability> {
        if self == Self::Read && os_enforced {
            Some(ToolCapability::Read)
        } else {
            command
        }
    }
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

    // shortcut: only direct Cargo build/check/test/run manifest inputs before --;
    // extend operand roles when another real supported command needs them.
    if command_name == "cargo"
        && matches!(
            super::command_options::cargo_subcommand(args),
            Some("build" | "check" | "test" | "run")
        )
        && !args.iter().any(|word| word == "--")
        && (args.last().is_some_and(|word| word == "--manifest-path")
            || token.decoded.starts_with("--manifest-path="))
    {
        return TokenPathRole::Read;
    }

    if matches!(command_name, "awk" | "gawk" | "mawk" | "nawk") {
        return match super::command_interpreters::awk_next_argument_role(args, &token.decoded) {
            super::command_interpreters::AwkArgumentRole::Program(offset) => {
                TokenPathRole::ExecutableData(offset)
            }
            super::command_interpreters::AwkArgumentRole::Data => TokenPathRole::Data,
            super::command_interpreters::AwkArgumentRole::Operand => TokenPathRole::Check,
        };
    }

    if matches!(command_name, "echo" | "printf") {
        return TokenPathRole::Data;
    }

    if let Some(role) = inline_code_argument_role(command_name, args, &token.decoded) {
        return role;
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

fn inline_code_argument_role(
    command_name: &str,
    args: &[String],
    current: &str,
) -> Option<TokenPathRole> {
    // Shell wrappers (`sh`, `bash`, `zsh`, `dash`, `ksh`) are deliberately excluded
    // because `command_wrappers` extracts and checks their inline scripts directly.
    if is_python_command(command_name) {
        return interpreter_flag_code_role(args, current, &["-c"], &[], &['c']);
    }
    if matches!(command_name, "node" | "deno" | "bun") {
        return interpreter_flag_code_role(
            args,
            current,
            &["-e", "-p", "--eval", "--print"],
            &["--eval=", "--print="],
            &[],
        );
    }
    if command_name == "ruby" {
        return interpreter_flag_code_role(args, current, &["-e"], &[], &['e']);
    }
    if command_name == "perl" {
        return interpreter_flag_code_role(args, current, &["-e", "-E"], &[], &['e', 'E']);
    }
    if command_name == "php" {
        return interpreter_flag_code_role(args, current, &["-r"], &[], &['r']);
    }
    if command_name == "jq" {
        return jq_argument_role(args, current);
    }
    if command_name == "sed" {
        return sed_argument_role(args, current);
    }
    if matches!(command_name, "grep" | "egrep" | "fgrep" | "rg") {
        return search_pattern_argument_role(args, current);
    }
    None
}

fn is_python_command(name: &str) -> bool {
    matches!(name, "python" | "python3")
        || name
            .strip_prefix("python3.")
            .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
}

fn interpreter_flag_code_role(
    args: &[String],
    current: &str,
    exact_flags: &[&str],
    equals_prefixes: &[&str],
    attached_short_chars: &[char],
) -> Option<TokenPathRole> {
    if args.iter().any(|word| word == "--") {
        return None;
    }
    if args
        .last()
        .is_some_and(|prev| exact_flags.iter().any(|flag| prev == flag))
    {
        return Some(TokenPathRole::InlineCode);
    }
    if equals_prefixes
        .iter()
        .any(|prefix| current.starts_with(prefix))
    {
        return Some(TokenPathRole::InlineCode);
    }
    if !current.starts_with("--")
        && let Some(short) = current.strip_prefix('-')
        && let Some(first) = short.chars().next()
        && attached_short_chars.contains(&first)
        && short.len() > first.len_utf8()
    {
        return Some(TokenPathRole::InlineCode);
    }
    None
}

fn jq_argument_role(args: &[String], current: &str) -> Option<TokenPathRole> {
    let mut filter_seen = false;
    let mut from_file_seen = false;
    let mut expect_file_or_dir = false;
    let mut expect_data_args = 0usize;
    let mut options_ended = false;

    for arg in args {
        if expect_data_args > 0 {
            expect_data_args -= 1;
            continue;
        }
        if expect_file_or_dir {
            expect_file_or_dir = false;
            continue;
        }
        if !options_ended && arg == "--" {
            options_ended = true;
            continue;
        }
        if !options_ended {
            if matches!(arg.as_str(), "-f" | "--from-file") {
                from_file_seen = true;
                expect_file_or_dir = true;
                continue;
            }
            if arg.starts_with("--from-file=") || (arg.starts_with("-f") && arg.len() > 2) {
                from_file_seen = true;
                continue;
            }
            if arg == "-L" {
                expect_file_or_dir = true;
                continue;
            }
            if matches!(
                arg.as_str(),
                "--arg" | "--argjson" | "--slurpfile" | "--rawfile"
            ) {
                expect_data_args = if matches!(arg.as_str(), "--arg" | "--argjson") {
                    2
                } else {
                    1
                };
                if matches!(arg.as_str(), "--slurpfile" | "--rawfile") {
                    expect_file_or_dir = true;
                }
                continue;
            }
            if arg == "--indent" {
                expect_data_args = 1;
                continue;
            }
            if arg.starts_with('-') {
                continue;
            }
        }
        filter_seen = true;
    }

    if expect_data_args > 0 {
        return Some(TokenPathRole::Data);
    }
    if expect_file_or_dir {
        return Some(TokenPathRole::Check);
    }
    if !from_file_seen && !filter_seen && (options_ended || !current.starts_with('-')) {
        return Some(TokenPathRole::InlineCode);
    }
    None
}

fn sed_argument_role(args: &[String], current: &str) -> Option<TokenPathRole> {
    let mut script_supplied = false;
    let mut expect_expression = false;
    let mut expect_file = false;
    let mut options_ended = false;

    for arg in args {
        if expect_expression {
            expect_expression = false;
            script_supplied = true;
            continue;
        }
        if expect_file {
            expect_file = false;
            script_supplied = true;
            continue;
        }
        if !options_ended && arg == "--" {
            options_ended = true;
            continue;
        }
        if !options_ended {
            if matches!(arg.as_str(), "-e" | "--expression") {
                expect_expression = true;
                continue;
            }
            if arg.starts_with("--expression=") || (arg.starts_with("-e") && arg.len() > 2) {
                script_supplied = true;
                continue;
            }
            if matches!(arg.as_str(), "-f" | "--file") {
                expect_file = true;
                continue;
            }
            if arg.starts_with("--file=") || (arg.starts_with("-f") && arg.len() > 2) {
                script_supplied = true;
                continue;
            }
            if matches!(arg.as_str(), "-i" | "--in-place") || arg.starts_with('-') {
                continue;
            }
        }
        script_supplied = true;
    }

    if expect_expression
        || current.starts_with("--expression=")
        || (!options_ended && current.starts_with("-e") && current.len() > 2)
    {
        return Some(TokenPathRole::InlineCode);
    }
    if expect_file {
        return Some(TokenPathRole::Check);
    }
    if !script_supplied && (options_ended || !current.starts_with('-')) {
        return Some(TokenPathRole::InlineCode);
    }
    None
}

fn search_pattern_argument_role(args: &[String], current: &str) -> Option<TokenPathRole> {
    let mut pattern_supplied = false;
    let mut expect_pattern = false;
    let mut expect_value_flag = false;
    let mut options_ended = false;

    for arg in args {
        if expect_pattern {
            expect_pattern = false;
            pattern_supplied = true;
            continue;
        }
        if expect_value_flag {
            expect_value_flag = false;
            continue;
        }
        if !options_ended && arg == "--" {
            options_ended = true;
            continue;
        }
        if !options_ended {
            if matches!(arg.as_str(), "-e" | "--regexp") {
                expect_pattern = true;
                continue;
            }
            if arg.starts_with("--regexp=") || (arg.starts_with("-e") && arg.len() > 2) {
                pattern_supplied = true;
                continue;
            }
            if matches!(arg.as_str(), "-f" | "--file") {
                pattern_supplied = true;
                expect_value_flag = true;
                continue;
            }
            if arg.starts_with("--file=") || (arg.starts_with("-f") && arg.len() > 2) {
                pattern_supplied = true;
                continue;
            }
            if matches!(
                arg.as_str(),
                "-A" | "-B"
                    | "-C"
                    | "-m"
                    | "-g"
                    | "-t"
                    | "-T"
                    | "-M"
                    | "-j"
                    | "-d"
                    | "-D"
                    | "--after-context"
                    | "--before-context"
                    | "--context"
                    | "--max-count"
                    | "--glob"
                    | "--iglob"
                    | "--type"
                    | "--type-not"
                    | "--type-add"
                    | "--max-columns"
                    | "--max-filesize"
                    | "--include"
                    | "--exclude"
                    | "--exclude-dir"
                    | "--color"
                    | "--colors"
                    | "--sort"
                    | "--sortr"
                    | "--encoding"
            ) {
                expect_value_flag = true;
                continue;
            }
            if arg.starts_with('-') {
                continue;
            }
        }
        pattern_supplied = true;
    }

    if expect_pattern
        || current.starts_with("--regexp=")
        || (!options_ended && current.starts_with("-e") && current.len() > 2)
    {
        return Some(TokenPathRole::InlineCode);
    }
    if expect_value_flag {
        return Some(TokenPathRole::Check);
    }
    if !pattern_supplied && (options_ended || !current.starts_with('-')) {
        return Some(TokenPathRole::InlineCode);
    }
    None
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
