//! Builtin tool implementations for the alan agent runtime.
//!
//! This crate provides 7 built-in tools as independent implementations of the
//! `Tool` trait defined in `alan-agent-engine`.
//!
//! Tool profiles:
//! - Core (default): read_file, write_file, edit_file, bash
//! - Read-only exploration: read_file, grep, glob, list_dir
//! - All: core + read-only exploration tools

mod file_tools;

pub use file_tools::{EditFileTool, ReadFileTool, WriteFileTool};

#[cfg(test)]
use file_tools::{detect_mime, is_image};

use alan_agent_engine::tools::{Sandbox, Tool, ToolContext, ToolRegistry, ToolResult};
use anyhow::{Result, anyhow};
use regex::RegexBuilder;
use serde_json::{Value, json};
use std::fs::FileType;
use std::path::Path;

// ============================================================================
// Bash
// ============================================================================

/// bash - Execute shell commands
#[derive(Default)]
pub struct BashTool;

impl BashTool {
    pub fn new() -> Self {
        Self
    }
}

fn classify_bash_command(command: &str) -> alan_agent_protocol::ToolCapability {
    let normalized = normalize_shell_line_continuations(command).to_lowercase();
    let fragments = split_shell_fragments(&normalized);

    let mut saw_write = false;
    let mut saw_unknown = false;
    for fragment in fragments {
        let capability = classify_bash_fragment(fragment.trim());
        if matches!(capability, alan_agent_protocol::ToolCapability::Network) {
            return alan_agent_protocol::ToolCapability::Network;
        }
        if matches!(capability, alan_agent_protocol::ToolCapability::Write) {
            saw_write = true;
        }
        if matches!(capability, alan_agent_protocol::ToolCapability::Unknown) {
            saw_unknown = true;
        }
    }

    if saw_write {
        alan_agent_protocol::ToolCapability::Write
    } else if saw_unknown {
        alan_agent_protocol::ToolCapability::Unknown
    } else {
        alan_agent_protocol::ToolCapability::Read
    }
}

fn is_shell_word_boundary(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, ';' | '|' | '&' | '(' | ')' | '<' | '>' | '{' | '}')
}

fn normalize_shell_line_continuations(command: &str) -> String {
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

fn split_shell_fragments(command: &str) -> Vec<String> {
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

fn classify_bash_fragment(fragment: &str) -> alan_agent_protocol::ToolCapability {
    if fragment.is_empty() {
        return alan_agent_protocol::ToolCapability::Read;
    }

    let tokens: Vec<&str> = fragment.split_whitespace().collect();
    if tokens.is_empty() {
        return alan_agent_protocol::ToolCapability::Read;
    }
    let effective_tokens = effective_command_tokens(&tokens);
    let effective_tokens = effective_tokens.as_slice();

    if contains_unsupported_shell_form(&tokens) {
        return alan_agent_protocol::ToolCapability::Unknown;
    }
    if is_network_command(fragment, effective_tokens) {
        return alan_agent_protocol::ToolCapability::Network;
    }
    if contains_nested_eval_wrapper(&tokens) {
        return alan_agent_protocol::ToolCapability::Unknown;
    }
    if is_write_command(fragment, effective_tokens) {
        return alan_agent_protocol::ToolCapability::Write;
    }
    if is_safe_read_command(effective_tokens) || is_wrapper_query_command(&tokens) {
        return alan_agent_protocol::ToolCapability::Read;
    }
    alan_agent_protocol::ToolCapability::Unknown
}

fn contains_unsupported_shell_form(tokens: &[&str]) -> bool {
    let Some(command_index) = tokens.iter().position(|word| !is_env_assignment(word)) else {
        return false;
    };

    let command_word = tokens[command_index];
    if is_shell_control_prefix(command_word) {
        return true;
    }

    is_unsupported_shell_wrapper(command_basename(command_word))
}

fn is_network_command(fragment: &str, tokens: &[&str]) -> bool {
    // Match on the basename so path-qualified forms (`/usr/bin/curl`) classify
    // like the bare head; otherwise an approved network call would run with the
    // sandbox network deny still in force and fail.
    let head = command_basename(tokens[0]);
    if matches!(
        head,
        "curl" | "wget" | "ssh" | "scp" | "sftp" | "nc" | "netcat" | "socat" | "telnet" | "ftp"
    ) {
        return true;
    }

    let pair = tokens.get(1).copied().unwrap_or_default();
    if (head == "git" && is_git_network_command(tokens))
        || (head == "docker" && pair == "pull")
        || (head == "npm" && pair == "install")
        || (head == "pnpm" && pair == "add")
        || (head == "yarn" && pair == "add")
        || ((head == "pip" || head == "pip3") && pair == "install")
        || (head == "cargo" && pair == "install")
        || (head == "brew" && pair == "install")
        || ((head == "apt" || head == "apt-get" || head == "yum" || head == "dnf")
            && pair == "install")
    {
        return true;
    }

    // Catch explicit http(s) fetch commands wrapped in generic shells.
    fragment.contains("http://") || fragment.contains("https://")
}

fn is_write_command(fragment: &str, tokens: &[&str]) -> bool {
    // Match on the basename so path-qualified forms (`/bin/rm`) classify like the
    // bare head instead of falling through to Unknown.
    let head = command_basename(tokens[0]);
    if matches!(
        head,
        "rm" | "rmdir" | "mv" | "cp" | "chmod" | "chown" | "mkdir" | "touch" | "truncate"
    ) {
        return true;
    }

    if head == "sed" && sed_in_place_flag(tokens) {
        return true;
    }

    if head == "find" && find_has_write_action(tokens) {
        return true;
    }

    if is_local_verification_command(tokens) {
        return true;
    }

    if head == "git" {
        if is_git_network_command(tokens) {
            return false;
        }
        if !is_git_read_command(tokens) {
            return true;
        }
    }

    contains_output_redirection(fragment)
}

fn sed_in_place_flag(tokens: &[&str]) -> bool {
    tokens.iter().skip(1).copied().any(|token| {
        token == "-i"
            || token == "--in-place"
            || short_option_cluster_contains_flag(token, 'i')
            || token
                .strip_prefix("-i")
                .is_some_and(|suffix| !suffix.is_empty())
            || token.starts_with("--in-place=")
    })
}

fn short_option_cluster_contains_flag(token: &str, flag: char) -> bool {
    token.starts_with('-') && !token.starts_with("--") && token.chars().skip(1).any(|ch| ch == flag)
}

fn find_has_write_action(tokens: &[&str]) -> bool {
    tokens.iter().skip(1).copied().any(|token| {
        matches!(
            token,
            "-exec"
                | "-execdir"
                | "-delete"
                | "-ok"
                | "-okdir"
                | "-fprint"
                | "-fprint0"
                | "-fprintf"
                | "-fls"
        )
    })
}

fn is_local_verification_command(tokens: &[&str]) -> bool {
    let head = command_basename(tokens[0]);
    let pair = tokens.get(1).copied().unwrap_or_default();

    if matches!(head, "tox" | "nox") {
        return !is_tool_query_command(tokens);
    }

    if matches!(head, "pytest" | "py.test" | "nosetests" | "nosetests3") {
        return true;
    }

    if (head == "cargo" && matches!(pair, "test" | "check" | "clippy"))
        || (head == "go" && pair == "test")
        || (matches!(head, "npm" | "pnpm" | "yarn" | "bun") && pair == "test")
        || (matches!(head, "make" | "just") && matches!(pair, "test" | "check"))
    {
        return true;
    }

    if python_module_command(tokens).is_some_and(|module| matches!(module, "pytest" | "unittest")) {
        return true;
    }

    local_verification_subject(tokens)
        .is_some_and(|(command, args)| is_local_verification_entrypoint(command, args))
}

fn is_python_query_command(tokens: &[&str]) -> bool {
    let head = command_basename(tokens.first().copied().unwrap_or_default());
    if !matches!(head, "python" | "python3") {
        return false;
    }

    is_tool_query_command(tokens)
}

fn is_tool_query_command(tokens: &[&str]) -> bool {
    tokens
        .iter()
        .skip(1)
        .copied()
        .find(|token| !token.is_empty())
        .is_some_and(|arg| matches!(arg, "-h" | "--help" | "--version") || arg.starts_with("-V"))
}

fn python_module_command<'a>(tokens: &'a [&'a str]) -> Option<&'a str> {
    let head = command_basename(tokens.first().copied()?);
    if !matches!(head, "python" | "python3") {
        return None;
    }

    let mut index = 1;
    while let Some(token) = tokens.get(index).copied() {
        if token == "-m" {
            return tokens.get(index + 1).copied();
        }
        if !token.starts_with('-') {
            return None;
        }
        index += 1;
    }

    None
}

fn local_verification_subject<'a>(tokens: &'a [&'a str]) -> Option<(&'a str, &'a [&'a str])> {
    let command = tokens.first().copied()?;
    if is_local_command_path(command) {
        return Some((command, &tokens[1..]));
    }

    python_script_command(tokens)
}

fn python_script_command<'a>(tokens: &'a [&'a str]) -> Option<(&'a str, &'a [&'a str])> {
    let head = command_basename(tokens.first().copied()?);
    if !matches!(head, "python" | "python3") {
        return None;
    }

    let mut index = 1;
    while let Some(token) = tokens.get(index).copied() {
        if token == "-m" || token == "-c" || token == "-" {
            return None;
        }
        if !token.starts_with('-') {
            return Some((token, &tokens[index + 1..]));
        }
        index += 1;
    }

    None
}

fn is_local_verification_entrypoint(command: &str, args: &[&str]) -> bool {
    if !is_local_command_path(command) {
        return false;
    }

    if is_verification_entrypoint_name(command_basename(command)) {
        return true;
    }

    first_non_option_arg(args).is_some_and(is_verification_subcommand)
}

fn is_local_command_path(command: &str) -> bool {
    if command.is_empty() {
        return false;
    }

    if command.starts_with("./") || command.starts_with("../") || command.contains('/') {
        return true;
    }

    matches!(
        Path::new(command).extension().and_then(|ext| ext.to_str()),
        Some("py" | "sh" | "rb" | "pl" | "php" | "js")
    ) || matches!(command_basename(command), "gradlew" | "mvnw")
}

fn is_verification_entrypoint_name(command: &str) -> bool {
    let stem = Path::new(command)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(command)
        .to_ascii_lowercase();

    matches!(
        stem.as_str(),
        "runtests" | "run-tests" | "run_tests" | "pytest" | "nosetests" | "nosetests3"
    ) || stem == "test"
        || stem == "tests"
}

fn first_non_option_arg<'a>(args: &'a [&'a str]) -> Option<&'a str> {
    let mut saw_double_dash = false;
    for arg in args {
        if arg.is_empty() {
            continue;
        }
        if !saw_double_dash {
            if *arg == "--" {
                saw_double_dash = true;
                continue;
            }
            if arg.starts_with('-') {
                continue;
            }
        }
        return Some(*arg);
    }

    None
}

fn is_verification_subcommand(arg: &str) -> bool {
    matches!(arg, "test" | "tests" | "check" | "clippy" | "verify")
}

fn contains_nested_eval_wrapper(tokens: &[&str]) -> bool {
    let Some(view) = nested_eval_command_view(tokens) else {
        return false;
    };
    view.opaque_wrapper || leading_eval_flag(view.command, view.args).is_some()
}

fn contains_output_redirection(fragment: &str) -> bool {
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    for ch in fragment.chars() {
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
            '>' => return true,
            _ => {}
        }
    }

    false
}

fn command_basename(command: &str) -> &str {
    Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(command)
}

struct NestedEvalCommandView<'a> {
    command: &'a str,
    args: &'a [&'a str],
    opaque_wrapper: bool,
}

fn nested_eval_command_view<'a>(tokens: &'a [&'a str]) -> Option<NestedEvalCommandView<'a>> {
    let mut command_index = next_command_offset(tokens)?;

    loop {
        let command = command_basename(tokens[command_index]);
        let args = &tokens[command_index + 1..];
        let next_offset = if command == "env" {
            if env_split_string_flag(args).is_some() {
                return Some(NestedEvalCommandView {
                    command,
                    args,
                    opaque_wrapper: true,
                });
            }
            env_command_offset(args)
        } else if is_transparent_command_wrapper(command) {
            transparent_wrapper_offset(command, args)
        } else {
            None
        };

        let Some(next_relative_offset) = next_offset else {
            return Some(NestedEvalCommandView {
                command,
                args,
                opaque_wrapper: false,
            });
        };
        command_index += 1 + next_relative_offset;
    }
}

fn effective_command_tokens<'a>(tokens: &'a [&'a str]) -> Vec<&'a str> {
    let Some(view) = nested_eval_command_view(tokens) else {
        return tokens.to_vec();
    };
    if view.opaque_wrapper {
        return tokens.to_vec();
    }

    let mut effective = Vec::with_capacity(1 + view.args.len());
    effective.push(view.command);
    effective.extend_from_slice(view.args);
    effective
}

fn next_command_offset(tokens: &[&str]) -> Option<usize> {
    let mut index = 0;
    while let Some(word) = tokens.get(index).copied() {
        if is_env_assignment(word) || is_shell_control_prefix(word) {
            index += 1;
            continue;
        }
        return Some(index);
    }
    None
}

fn env_command_offset(args: &[&str]) -> Option<usize> {
    let mut index = 0;
    while let Some(arg) = args.get(index).copied() {
        if arg == "--" {
            index += 1;
            break;
        }
        if is_env_assignment(arg) {
            index += 1;
            continue;
        }
        match env_option_behavior(arg) {
            Some(
                EnvOptionBehavior::Passthrough
                | EnvOptionBehavior::InlineValue
                | EnvOptionBehavior::SplitStringInlineValue,
            ) => {
                index += 1;
                continue;
            }
            Some(EnvOptionBehavior::TakesNextArg | EnvOptionBehavior::SplitStringNextArg) => {
                index += 2;
                continue;
            }
            None => {}
        }
        break;
    }

    args.get(index)?;
    Some(index)
}

fn transparent_wrapper_offset(command: &str, args: &[&str]) -> Option<usize> {
    match command {
        "command" => command_wrapper_offset(args),
        "exec" => exec_wrapper_offset(args),
        "builtin" => builtin_wrapper_offset(args),
        "nice" => nice_wrapper_offset(args),
        "nohup" => nohup_wrapper_offset(args),
        "timeout" => timeout_wrapper_offset(args),
        "stdbuf" => stdbuf_wrapper_offset(args),
        "setsid" => setsid_wrapper_offset(args),
        _ => None,
    }
}

fn command_wrapper_offset(args: &[&str]) -> Option<usize> {
    let mut index = 0;
    while let Some(arg) = args.get(index).copied() {
        if arg == "--" {
            index += 1;
            break;
        }
        if command_wrapper_is_query_flag(arg) {
            return None;
        }
        if command_wrapper_is_exec_flag(arg) {
            index += 1;
            continue;
        }
        break;
    }

    args.get(index)?;
    Some(index)
}

fn builtin_wrapper_offset(args: &[&str]) -> Option<usize> {
    let mut index = 0;
    if let Some(arg) = args.get(index).copied() {
        if arg == "--" {
            index += 1;
        } else if builtin_query_flag(arg) {
            return None;
        }
    }

    args.get(index)?;
    Some(index)
}

fn exec_wrapper_offset(args: &[&str]) -> Option<usize> {
    let mut index = 0;
    while let Some(arg) = args.get(index).copied() {
        if arg == "--" {
            index += 1;
            break;
        }
        if arg == "-a" {
            index += 2;
            continue;
        }
        if has_inline_exec_argv0(arg) || is_exec_wrapper_flag(arg) {
            index += 1;
            continue;
        }
        break;
    }

    args.get(index)?;
    Some(index)
}

fn nice_wrapper_offset(args: &[&str]) -> Option<usize> {
    let mut index = 0;
    while let Some(arg) = args.get(index).copied() {
        if common_wrapper_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            index += 1;
            break;
        }
        if exact_or_inline_option_with_value(arg, &["-n"], &["--adjustment"]) {
            index += if has_attached_option_value(arg) { 1 } else { 2 };
            continue;
        }
        if arg.starts_with('-') {
            index += 1;
            continue;
        }
        break;
    }

    args.get(index)?;
    Some(index)
}

fn nohup_wrapper_offset(args: &[&str]) -> Option<usize> {
    let mut index = 0;
    if let Some(arg) = args.get(index).copied() {
        if common_wrapper_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            index += 1;
        }
    }

    args.get(index)?;
    Some(index)
}

fn timeout_wrapper_offset(args: &[&str]) -> Option<usize> {
    let mut index = 0;
    while let Some(arg) = args.get(index).copied() {
        if common_wrapper_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            index += 1;
            break;
        }
        if exact_or_inline_option_with_value(arg, &["-k", "-s"], &["--kill-after", "--signal"]) {
            index += if has_attached_option_value(arg) { 1 } else { 2 };
            continue;
        }
        if matches!(
            arg,
            "-v" | "--verbose" | "--foreground" | "--preserve-status"
        ) {
            index += 1;
            continue;
        }
        if arg.starts_with('-') {
            index += 1;
            continue;
        }
        break;
    }

    args.get(index)?;
    index += 1;
    args.get(index)?;
    Some(index)
}

fn stdbuf_wrapper_offset(args: &[&str]) -> Option<usize> {
    let mut index = 0;
    while let Some(arg) = args.get(index).copied() {
        if common_wrapper_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            index += 1;
            break;
        }
        if exact_or_inline_option_with_value(arg, &["-i", "-o", "-e"], &[]) {
            index += if has_attached_option_value(arg) { 1 } else { 2 };
            continue;
        }
        if arg.starts_with('-') {
            index += 1;
            continue;
        }
        break;
    }

    args.get(index)?;
    Some(index)
}

fn setsid_wrapper_offset(args: &[&str]) -> Option<usize> {
    let mut index = 0;
    while let Some(arg) = args.get(index).copied() {
        if matches!(arg, "-h" | "-V") || common_wrapper_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            index += 1;
            break;
        }
        if matches!(arg, "-c" | "-f" | "-w") {
            index += 1;
            continue;
        }
        if arg.starts_with('-') {
            index += 1;
            continue;
        }
        break;
    }

    args.get(index)?;
    Some(index)
}

fn is_env_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn is_transparent_command_wrapper(command: &str) -> bool {
    matches!(
        command,
        "command" | "builtin" | "exec" | "nice" | "nohup" | "timeout" | "stdbuf" | "setsid"
    )
}

fn is_shell_control_prefix(word: &str) -> bool {
    matches!(
        word,
        "!" | "if"
            | "then"
            | "elif"
            | "else"
            | "fi"
            | "for"
            | "while"
            | "until"
            | "do"
            | "done"
            | "case"
            | "esac"
            | "select"
            | "function"
    )
}

fn is_unsupported_shell_wrapper(command: &str) -> bool {
    matches!(
        command,
        "env"
            | "command"
            | "builtin"
            | "exec"
            | "time"
            | "nice"
            | "nohup"
            | "timeout"
            | "stdbuf"
            | "setsid"
    )
}

fn common_wrapper_query_flag(arg: &str) -> bool {
    matches!(arg, "--help" | "--version")
}

fn env_split_string_flag<'a>(args: &'a [&'a str]) -> Option<&'a str> {
    let mut index = 0;
    while let Some(arg) = args.get(index).copied() {
        if arg == "--" {
            return None;
        }
        if is_env_assignment(arg) {
            index += 1;
            continue;
        }
        match env_option_behavior(arg) {
            Some(
                EnvOptionBehavior::SplitStringInlineValue | EnvOptionBehavior::SplitStringNextArg,
            ) => return Some(arg),
            Some(EnvOptionBehavior::Passthrough | EnvOptionBehavior::InlineValue) => {
                index += 1;
                continue;
            }
            Some(EnvOptionBehavior::TakesNextArg) => {
                index += 2;
                continue;
            }
            None => {}
        }
        break;
    }
    None
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EnvOptionBehavior {
    Passthrough,
    TakesNextArg,
    InlineValue,
    SplitStringNextArg,
    SplitStringInlineValue,
}

fn env_option_behavior(arg: &str) -> Option<EnvOptionBehavior> {
    if matches!(arg, "--ignore-environment" | "--null") {
        return Some(EnvOptionBehavior::Passthrough);
    }
    if arg == "--split-string" {
        return Some(EnvOptionBehavior::SplitStringNextArg);
    }
    if arg.starts_with("--split-string=") {
        return Some(EnvOptionBehavior::SplitStringInlineValue);
    }
    if matches!(arg, "--unset" | "--chdir") {
        return Some(EnvOptionBehavior::TakesNextArg);
    }
    if arg.starts_with("--unset=") || arg.starts_with("--chdir=") {
        return Some(EnvOptionBehavior::InlineValue);
    }
    env_short_option_behavior(arg)
}

fn env_short_option_behavior(arg: &str) -> Option<EnvOptionBehavior> {
    if arg.starts_with("--") {
        return None;
    }
    let rest = arg.strip_prefix('-')?;
    if rest.is_empty() {
        return None;
    }

    let mut saw_passthrough = false;
    for (index, ch) in rest.char_indices() {
        match ch {
            'i' | '0' => saw_passthrough = true,
            'u' | 'c' | 'C' => {
                return Some(if rest[index + ch.len_utf8()..].is_empty() {
                    EnvOptionBehavior::TakesNextArg
                } else {
                    EnvOptionBehavior::InlineValue
                });
            }
            's' | 'S' => {
                return Some(if rest[index + ch.len_utf8()..].is_empty() {
                    EnvOptionBehavior::SplitStringNextArg
                } else {
                    EnvOptionBehavior::SplitStringInlineValue
                });
            }
            _ => return None,
        }
    }

    saw_passthrough.then_some(EnvOptionBehavior::Passthrough)
}

fn command_wrapper_is_exec_flag(arg: &str) -> bool {
    let Some(rest) = arg.strip_prefix('-') else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|ch| ch == 'p')
}

fn command_wrapper_is_query_flag(arg: &str) -> bool {
    let Some(rest) = arg.strip_prefix('-') else {
        return false;
    };
    !rest.is_empty()
        && rest.chars().all(|ch| matches!(ch, 'p' | 'v' | 'V'))
        && rest.chars().any(|ch| matches!(ch, 'v' | 'V'))
}

fn builtin_query_flag(arg: &str) -> bool {
    arg == "-p"
}

fn is_exec_wrapper_flag(arg: &str) -> bool {
    let Some(rest) = arg.strip_prefix('-') else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|ch| matches!(ch, 'c' | 'l'))
}

fn has_inline_exec_argv0(arg: &str) -> bool {
    arg.starts_with("-a") && arg.len() > 2
}

fn is_shell_eval_wrapper(command: &str, flag: &str) -> bool {
    matches!(command, "sh" | "bash" | "dash" | "zsh" | "ksh")
        && short_flag_contains_option(flag, 'c')
}

fn is_code_eval_wrapper(command: &str, flag: &str) -> bool {
    match command {
        "python" | "python3" => short_flag_contains_option(flag, 'c'),
        "node" => {
            short_flag_contains_option(flag, 'e')
                || short_flag_contains_option(flag, 'p')
                || flag == "--print"
        }
        "perl" => short_flag_contains_option(flag, 'e') || short_flag_contains_option(flag, 'E'),
        "ruby" | "lua" => short_flag_contains_option(flag, 'e'),
        "php" => short_flag_contains_option(flag, 'r'),
        _ => false,
    }
}

fn leading_eval_flag<'a>(command: &str, args: &'a [&'a str]) -> Option<&'a str> {
    match command {
        "sh" | "bash" | "dash" | "zsh" | "ksh" => scan_leading_args(
            args,
            |arg| is_shell_eval_wrapper("sh", arg),
            shell_wrapper_advance,
        ),
        "python" | "python3" => scan_leading_args(
            args,
            |arg| is_code_eval_wrapper("python3", arg),
            python_wrapper_advance,
        ),
        "node" => scan_leading_args(
            args,
            |arg| is_code_eval_wrapper("node", arg),
            node_wrapper_advance,
        ),
        "perl" => scan_leading_args(
            args,
            |arg| is_code_eval_wrapper("perl", arg),
            perl_wrapper_advance,
        ),
        "ruby" => scan_leading_args(
            args,
            |arg| is_code_eval_wrapper("ruby", arg),
            ruby_wrapper_advance,
        ),
        "lua" => scan_leading_args(
            args,
            |arg| is_code_eval_wrapper("lua", arg),
            lua_wrapper_advance,
        ),
        "php" => scan_leading_args(
            args,
            |arg| is_code_eval_wrapper("php", arg),
            php_wrapper_advance,
        ),
        _ => None,
    }
}

fn scan_leading_args<'a, F, G>(args: &'a [&'a str], matches_eval: F, advance: G) -> Option<&'a str>
where
    F: Fn(&str) -> bool,
    G: Fn(&str) -> Option<usize>,
{
    let mut index = 0;
    while let Some(arg) = args.get(index).copied() {
        if arg == "--" {
            break;
        }
        if matches_eval(arg) {
            return Some(arg);
        }
        index += advance(arg)?;
    }
    None
}

fn shell_wrapper_advance(arg: &str) -> Option<usize> {
    if exact_or_inline_option_with_value(
        arg,
        &["-o", "+o", "-O", "+O"],
        &["--rcfile", "--init-file"],
    ) {
        Some(if has_attached_option_value(arg) { 1 } else { 2 })
    } else if arg.starts_with('-') || arg.starts_with('+') {
        Some(1)
    } else {
        None
    }
}

fn python_wrapper_advance(arg: &str) -> Option<usize> {
    if exact_or_inline_option_with_value(arg, &["-W", "-X"], &["--check-hash-based-pycs"]) {
        Some(if has_attached_option_value(arg) { 1 } else { 2 })
    } else if matches!(arg, "-m" | "--module" | "-") {
        None
    } else if arg.starts_with('-') {
        Some(1)
    } else {
        None
    }
}

fn node_wrapper_advance(arg: &str) -> Option<usize> {
    if exact_or_inline_option_with_value(
        arg,
        &["-r", "-C"],
        &[
            "--require",
            "--loader",
            "--experimental-loader",
            "--import",
            "--watch-path",
            "--conditions",
            "--input-type",
            "--inspect",
            "--inspect-brk",
            "--inspect-port",
            "--openssl-config",
            "--redirect-warnings",
            "--trace-event-categories",
            "--trace-event-file-pattern",
            "--diagnostic-dir",
            "--icu-data-dir",
            "--title",
        ],
    ) {
        Some(if has_attached_option_value(arg) { 1 } else { 2 })
    } else if arg.starts_with('-') {
        Some(1)
    } else {
        None
    }
}

fn perl_wrapper_advance(arg: &str) -> Option<usize> {
    if exact_or_inline_option_with_value(arg, &["-I", "-M", "-m"], &[]) {
        Some(if has_attached_option_value(arg) { 1 } else { 2 })
    } else if arg.starts_with('-') {
        Some(1)
    } else {
        None
    }
}

fn ruby_wrapper_advance(arg: &str) -> Option<usize> {
    if exact_or_inline_option_with_value(
        arg,
        &["-C", "-E", "-F", "-I", "-r"],
        &["--enable", "--disable", "--encoding"],
    ) {
        Some(if has_attached_option_value(arg) { 1 } else { 2 })
    } else if arg.starts_with('-') {
        Some(1)
    } else {
        None
    }
}

fn lua_wrapper_advance(arg: &str) -> Option<usize> {
    if exact_or_inline_option_with_value(arg, &["-l"], &[]) {
        Some(if has_attached_option_value(arg) { 1 } else { 2 })
    } else if arg.starts_with('-') {
        Some(1)
    } else {
        None
    }
}

fn php_wrapper_advance(arg: &str) -> Option<usize> {
    if exact_or_inline_option_with_value(arg, &["-c", "-d", "-z"], &["--define"]) {
        Some(if has_attached_option_value(arg) { 1 } else { 2 })
    } else if matches!(arg, "-f" | "--file") {
        None
    } else if arg.starts_with('-') {
        Some(1)
    } else {
        None
    }
}

fn exact_or_inline_option_with_value(arg: &str, short: &[&str], long: &[&str]) -> bool {
    short
        .iter()
        .any(|flag| arg == *flag || arg.starts_with(flag))
        || long
            .iter()
            .any(|flag| arg == *flag || arg.starts_with(&format!("{flag}=")))
}

fn has_attached_option_value(arg: &str) -> bool {
    arg.contains('=') || (arg.starts_with('-') && !arg.starts_with("--") && arg.len() > 2)
}

fn short_flag_contains_option(flag: &str, option: char) -> bool {
    if let Some(rest) = flag
        .strip_prefix("--")
        .map(|rest| rest.split_once('=').map_or(rest, |(name, _)| name))
    {
        return matches!(
            (rest, option),
            ("command", 'c') | ("eval", 'e') | ("print", 'p') | ("run", 'r')
        );
    }

    flag.starts_with('-') && flag.chars().skip(1).any(|ch| ch == option)
}

fn is_safe_read_command(tokens: &[&str]) -> bool {
    let head = tokens[0];

    if matches!(
        head,
        "ls" | "pwd"
            | "cd"
            | "pushd"
            | "popd"
            | "dirs"
            | "cat"
            | "head"
            | "tail"
            | "wc"
            | "rg"
            | "grep"
            | "which"
            | "whereis"
            | "basename"
            | "dirname"
            | "realpath"
            | "readlink"
            | "stat"
            | "file"
            | "du"
            | "df"
            | "cut"
            | "tr"
            | "sort"
            | "uniq"
            | "nl"
            | "tree"
            | "find"
            | "echo"
            | "printf"
            | "env"
            | "printenv"
            | "id"
            | "whoami"
            | "uname"
            | "date"
            | "ps"
            | "uptime"
            | "history"
            | "true"
            | "false"
            | "test"
            | "["
    ) {
        return true;
    }

    if head == "sed" {
        return is_sed_safe_read_command(tokens);
    }

    if is_python_query_command(tokens) {
        return true;
    }

    if matches!(head, "tox" | "nox") && is_tool_query_command(tokens) {
        return true;
    }

    if head == "command" {
        return is_command_query(tokens);
    }

    if head == "builtin" {
        return is_builtin_query(tokens);
    }

    if head == "git" {
        return is_git_read_command(tokens);
    }

    false
}

fn is_sed_safe_read_command(tokens: &[&str]) -> bool {
    let mut saw_script = false;
    let mut index = 1;
    while let Some(token) = tokens.get(index).copied() {
        if token == "--" {
            break;
        }
        if matches!(token, "-n" | "--quiet" | "--silent") {
            index += 1;
            continue;
        }
        if token == "-e" {
            let Some(script) = tokens.get(index + 1).copied() else {
                return false;
            };
            if !is_sed_safe_script(script) {
                return false;
            }
            saw_script = true;
            index += 2;
            continue;
        }
        if token.starts_with('-') {
            return false;
        }
        if !saw_script {
            if !is_sed_safe_script(token) {
                return false;
            }
            saw_script = true;
            index += 1;
            continue;
        }
        break;
    }

    saw_script
}

fn is_sed_safe_script(token: &str) -> bool {
    is_sed_line_range_print_script(token) || is_sed_substitute_script(token)
}

fn is_sed_line_range_print_script(token: &str) -> bool {
    let script = token.trim_matches(|ch| ch == '\'' || ch == '"');
    let Some(address) = script.strip_suffix('p') else {
        return false;
    };
    !address.is_empty()
        && address
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, ',' | '$'))
        && address.chars().any(|ch| ch.is_ascii_digit() || ch == '$')
}

fn is_sed_substitute_script(token: &str) -> bool {
    let script = token.trim_matches(|ch| ch == '\'' || ch == '"');
    let mut chars = script.chars();
    if chars.next() != Some('s') {
        return false;
    }

    let delimiter = match chars.next() {
        Some(ch) if !ch.is_ascii_alphanumeric() && !ch.is_ascii_whitespace() => ch,
        _ => return false,
    };
    let body: String = chars.collect();
    let mut delimiter_count = 0;
    let mut escaped = false;
    let mut tail_start = None;

    for (idx, ch) in body.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == delimiter {
            delimiter_count += 1;
            if delimiter_count == 2 {
                tail_start = Some(idx + ch.len_utf8());
                break;
            }
        }
    }

    let Some(tail_index) = tail_start else {
        return false;
    };
    let tail = body[tail_index..].trim();
    !tail.contains('w') && !tail.contains('e')
}

fn is_wrapper_query_command(tokens: &[&str]) -> bool {
    let Some(command) = tokens.first().copied() else {
        return false;
    };

    match command {
        "nice" | "nohup" | "timeout" | "stdbuf" | "setsid" => tokens
            .iter()
            .skip(1)
            .copied()
            .take_while(|token| *token != "--")
            .any(common_wrapper_query_flag),
        _ => false,
    }
}

fn is_command_query(tokens: &[&str]) -> bool {
    let mut index = 1;
    let mut saw_query = false;

    while let Some(token) = tokens.get(index).copied() {
        if token == "--" {
            return saw_query;
        }
        if command_wrapper_is_query_flag(token) {
            saw_query = true;
            index += 1;
            continue;
        }
        if command_wrapper_is_exec_flag(token) {
            index += 1;
            continue;
        }
        break;
    }

    saw_query
}

fn is_builtin_query(tokens: &[&str]) -> bool {
    tokens.get(1).copied().is_some_and(builtin_query_flag)
}

fn git_subcommand<'a>(tokens: &'a [&'a str]) -> Option<(usize, &'a str)> {
    // Match on the basename so path-qualified git (`/usr/bin/git -C repo push`)
    // is classified like bare `git`; otherwise a push/fetch misses network
    // classification and runs with the sandbox network deny in force.
    match tokens.first() {
        Some(first) if command_basename(first) == "git" => {}
        _ => return None,
    }

    let mut idx = 1;
    while idx < tokens.len() {
        let token = tokens[idx];
        if token == "--" {
            return tokens
                .get(idx + 1)
                .copied()
                .map(|subcommand| (idx + 1, subcommand));
        }
        if !token.starts_with('-') {
            return Some((idx, token));
        }

        let takes_value = matches!(
            token,
            "-c" | "-C"
                | "--exec-path"
                | "--git-dir"
                | "--work-tree"
                | "--namespace"
                | "--super-prefix"
                | "--config-env"
        );
        idx += 1;
        if takes_value && !token.contains('=') && idx < tokens.len() {
            idx += 1;
        }
    }

    None
}

fn is_git_network_command(tokens: &[&str]) -> bool {
    let Some((_, subcommand)) = git_subcommand(tokens) else {
        return false;
    };

    matches!(
        subcommand,
        "clone" | "fetch" | "pull" | "push" | "ls-remote"
    ) || is_git_remote_network(tokens)
        || is_git_submodule_network(tokens)
}

fn is_git_read_command(tokens: &[&str]) -> bool {
    let Some((_, subcommand)) = git_subcommand(tokens) else {
        return true;
    };

    if subcommand == "submodule" {
        return is_git_submodule_read(tokens);
    }

    match subcommand {
        "status" | "diff" | "log" | "show" | "rev-parse" | "ls-files" | "ls-tree" | "blame"
        | "grep" | "shortlog" | "describe" => true,
        "branch" => is_git_branch_read(tokens),
        "remote" => is_git_remote_read(tokens),
        "tag" => is_git_tag_read(tokens),
        _ => false,
    }
}

fn is_git_branch_read(tokens: &[&str]) -> bool {
    let Some((branch_idx, subcommand)) = git_subcommand(tokens) else {
        return false;
    };
    if subcommand != "branch" {
        return false;
    }

    const WRITE_FLAGS: &[&str] = &[
        "-c",
        "-C",
        "-d",
        "-D",
        "-f",
        "-m",
        "-M",
        "--copy",
        "--delete",
        "--move",
        "--edit-description",
        "--set-upstream-to",
        "--track",
        "--unset-upstream",
    ];
    if tokens
        .iter()
        .skip(branch_idx + 1)
        .any(|token| WRITE_FLAGS.contains(token) || token.starts_with("--set-upstream-to="))
    {
        return false;
    }

    let list_mode = tokens
        .iter()
        .skip(branch_idx + 1)
        .any(|token| matches!(*token, "-l" | "--list"));
    let has_positional = tokens
        .iter()
        .skip(branch_idx + 1)
        .any(|token| !token.starts_with('-'));

    !has_positional || list_mode
}

fn git_remote_subcommand<'a>(tokens: &'a [&'a str]) -> Option<&'a str> {
    let (remote_idx, subcommand) = git_subcommand(tokens)?;
    if subcommand != "remote" {
        return None;
    }

    tokens
        .iter()
        .skip(remote_idx + 1)
        .find_map(|token| (!token.starts_with('-')).then_some(*token))
}

fn is_git_remote_network(tokens: &[&str]) -> bool {
    let Some((remote_idx, subcommand)) = git_subcommand(tokens) else {
        return false;
    };
    if subcommand != "remote" {
        return false;
    }

    matches!(git_remote_subcommand(tokens), Some("show" | "update")) && !tokens.contains(&"-n")
        || (matches!(git_remote_subcommand(tokens), Some("add"))
            && tokens
                .iter()
                .skip(remote_idx + 1)
                .any(|token| matches!(*token, "-f" | "--fetch")))
}

fn is_git_remote_read(tokens: &[&str]) -> bool {
    let Some((_, subcommand)) = git_subcommand(tokens) else {
        return false;
    };
    if subcommand != "remote" {
        return false;
    }

    match git_remote_subcommand(tokens) {
        None => true,
        Some("get-url") => true,
        Some("show") => tokens.contains(&"-n"),
        _ => false,
    }
}

fn is_git_tag_read(tokens: &[&str]) -> bool {
    let Some((tag_idx, subcommand)) = git_subcommand(tokens) else {
        return false;
    };
    if subcommand != "tag" {
        return false;
    }

    const WRITE_FLAGS: &[&str] = &[
        "-a",
        "-d",
        "-f",
        "-m",
        "-s",
        "-u",
        "--annotate",
        "--delete",
        "--force",
        "--local-user",
        "--message",
        "--sign",
    ];
    if tokens
        .iter()
        .skip(tag_idx + 1)
        .any(|token| WRITE_FLAGS.contains(token) || token.starts_with("--message="))
    {
        return false;
    }

    let read_flag = tokens.iter().skip(tag_idx + 1).any(|token| {
        matches!(
            *token,
            "-l" | "-n"
                | "-v"
                | "--list"
                | "--contains"
                | "--merged"
                | "--no-merged"
                | "--points-at"
                | "--sort"
                | "--column"
                | "--color"
                | "--verify"
        )
    });
    let has_positional = tokens
        .iter()
        .skip(tag_idx + 1)
        .any(|token| !token.starts_with('-'));

    !has_positional || read_flag
}

fn git_submodule_subcommand<'a>(tokens: &'a [&'a str]) -> Option<(usize, &'a str)> {
    let (submodule_idx, subcommand) = git_subcommand(tokens)?;
    if subcommand != "submodule" {
        return None;
    }

    tokens
        .iter()
        .enumerate()
        .skip(submodule_idx + 1)
        .find_map(|(idx, token)| (!token.starts_with('-')).then_some((idx, *token)))
}

fn is_git_submodule_network(tokens: &[&str]) -> bool {
    let Some((subcommand_idx, subcommand)) = git_submodule_subcommand(tokens) else {
        return false;
    };

    match subcommand {
        "update" => !tokens.contains(&"--no-fetch"),
        "add" => tokens
            .iter()
            .skip(subcommand_idx + 1)
            .any(|token| token.contains("://") || token.starts_with("git@")),
        _ => false,
    }
}

fn is_git_submodule_read(tokens: &[&str]) -> bool {
    if tokens
        .iter()
        .any(|token| matches!(*token, "-h" | "--help" | "help"))
    {
        return true;
    }

    let Some((_, subcommand)) = git_submodule_subcommand(tokens) else {
        return true;
    };

    matches!(subcommand, "status" | "summary")
}
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute shell commands from the Process cwd, subject to namespace authority, policy, and execution-backend constraints. Prefer direct commands like rg, sed, git status, or curl. Avoid opaque interpreter wrappers like python -, python -c, bash -c, or sh -c unless genuinely required, because sandbox preflight may reject them conservatively."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to execute. Prefer direct commands instead of wrappers like python -, python -c, bash -c, or sh -c."
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds (max 300)",
                    "minimum": 1,
                    "maximum": 300,
                    "default": 60
                }
            }
        })
    }

    fn execute(&self, args: Value, ctx: &ToolContext) -> ToolResult {
        let sandbox = match ctx.sandbox() {
            Ok(sandbox) => sandbox,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let cwd = ctx.cwd.clone();
        let host_mounts = ctx.host_mounts.clone();
        let command = args["command"].as_str().unwrap_or("").to_string();
        let capability = classify_bash_command(&command);
        let timeout_secs = args["timeout"].as_u64().unwrap_or(60).clamp(1, 300);

        Box::pin(async move {
            let result = sandbox
                .exec_with_timeout_and_capability(
                    &command,
                    &cwd,
                    Some(std::time::Duration::from_secs(timeout_secs)),
                    Some(capability),
                )
                .await?;

            Ok(json!({
                "stdout": project_host_paths(&result.stdout, &host_mounts),
                "stderr": project_host_paths(&result.stderr, &host_mounts),
                "exit_code": result.exit_code,
                "success": result.exit_code == 0
            }))
        })
    }

    fn capability(&self, args: &Value) -> alan_agent_protocol::ToolCapability {
        let command = args["command"].as_str().unwrap_or("");
        classify_bash_command(command)
    }

    fn capability_is_argument_dependent(&self) -> bool {
        true
    }

    fn timeout_secs(&self) -> usize {
        300 // Must be >= user-configurable timeout upper bound in schema
    }
}

// ============================================================================
// Grep
// ============================================================================

/// grep - Search file contents
#[derive(Default)]
pub struct GrepTool;

impl GrepTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search for patterns in files using regex."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["pattern", "path"],
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search in"
                },
                "case_sensitive": {
                    "type": "boolean",
                    "description": "Case sensitive search",
                    "default": false
                }
            }
        })
    }

    fn execute(&self, args: Value, ctx: &ToolContext) -> ToolResult {
        let sandbox = match ctx.sandbox() {
            Ok(sandbox) => sandbox,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let path = match ctx.resolve_path(args["path"].as_str().unwrap_or("")) {
            Ok(path) => path,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let host_mounts = ctx.host_mounts.clone();
        let pattern = args["pattern"].as_str().unwrap_or("").to_string();
        let case_sensitive = args["case_sensitive"].as_bool().unwrap_or(false);

        Box::pin(async move {
            let regex = RegexBuilder::new(&pattern)
                .case_insensitive(!case_sensitive)
                .build()
                .map_err(|e| anyhow!("Invalid regex pattern: {}", e))?;

            let mut matches = Vec::new();

            if path.is_file() {
                let content = sandbox.read_string(&path).await?;
                for (line_num, line) in content.lines().enumerate() {
                    if regex.is_match(line) {
                        matches.push(json!({
                            "path": visible_host_path(&path, &host_mounts),
                            "line": line_num + 1,
                            "content": line
                        }));
                    }
                }
            } else if path.is_dir() {
                // Recursive search
                search_directory(&sandbox, &path, &regex, &host_mounts, &mut matches).await?;
            }

            Ok(json!({
                "matches": matches,
                "total": matches.len()
            }))
        })
    }

    fn capability(&self, _args: &Value) -> alan_agent_protocol::ToolCapability {
        alan_agent_protocol::ToolCapability::Read
    }
}

async fn search_directory(
    sandbox: &Sandbox,
    dir: &Path,
    regex: &regex::Regex,
    host_mounts: &[alan_agent_engine::HostMountGrant],
    matches: &mut Vec<Value>,
) -> Result<()> {
    let entries = sandbox.list_dir(dir).await?;

    for entry in entries {
        let path = entry.path();
        let file_type: FileType = entry.file_type().await?;

        if file_type.is_dir() {
            // Skip hidden directories
            if let Some(name) = path.file_name()
                && name.to_string_lossy().starts_with('.')
            {
                continue;
            }
            Box::pin(search_directory(
                sandbox,
                &path,
                regex,
                host_mounts,
                matches,
            ))
            .await?;
        } else if file_type.is_file() {
            // Skip binary files
            if is_binary_file(&path) {
                continue;
            }

            if let Ok(content) = sandbox.read_string(&path).await {
                for (line_num, line) in content.lines().enumerate() {
                    if regex.is_match(line) {
                        matches.push(json!({
                            "path": visible_host_path(&path, host_mounts),
                            "line": line_num + 1,
                            "content": line
                        }));
                    }
                }
            }
        }
    }

    Ok(())
}

// ============================================================================
// Glob
// ============================================================================

/// glob - Find files matching patterns
#[derive(Default)]
pub struct GlobTool;

impl GlobTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Find files matching a glob pattern."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["pattern"],
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern (e.g., '**/*.rs', 'src/*.txt')"
                },
                "path": {
                    "type": "string",
                    "description": "Base directory (default: Process cwd)",
                    "default": "."
                }
            }
        })
    }

    fn execute(&self, args: Value, ctx: &ToolContext) -> ToolResult {
        let sandbox = match ctx.sandbox() {
            Ok(sandbox) => sandbox,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let base_path = if let Some(path) = args["path"].as_str() {
            match ctx.resolve_path(path) {
                Ok(path) => path,
                Err(err) => return Box::pin(async move { Err(err) }),
            }
        } else {
            ctx.cwd.clone()
        };
        let pattern = args["pattern"].as_str().unwrap_or("").to_string();
        let host_mounts = ctx.host_mounts.clone();

        Box::pin(async move {
            if !sandbox.is_readable(&base_path) {
                return Err(anyhow!(
                    "Path outside the Process file view: {}",
                    base_path.to_string_lossy()
                ));
            }

            if Path::new(&pattern).is_absolute() {
                return Err(anyhow!("Glob pattern must be relative to base path"));
            }

            let pattern_str = base_path.join(&pattern);
            let pattern_str = pattern_str.to_string_lossy();

            let mut matches = Vec::new();

            // Use glob crate for pattern matching
            for path in glob::glob(&pattern_str)?.flatten() {
                if path.is_file() && sandbox.is_readable(&path) {
                    matches.push(visible_host_path(&path, &host_mounts));
                }
            }

            Ok(json!({
                "matches": matches,
                "total": matches.len()
            }))
        })
    }

    fn capability(&self, _args: &Value) -> alan_agent_protocol::ToolCapability {
        alan_agent_protocol::ToolCapability::Read
    }
}

// ============================================================================
// ListDir
// ============================================================================

/// list_dir - List directory contents
#[derive(Default)]
pub struct ListDirTool;

impl ListDirTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for ListDirTool {
    fn name(&self) -> &str {
        "list_dir"
    }

    fn description(&self) -> &str {
        "List contents of a directory."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory path (default: current directory)",
                    "default": "."
                }
            }
        })
    }

    fn execute(&self, args: Value, ctx: &ToolContext) -> ToolResult {
        let sandbox = match ctx.sandbox() {
            Ok(sandbox) => sandbox,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let path = if let Some(p) = args["path"].as_str() {
            match ctx.resolve_path(p) {
                Ok(path) => path,
                Err(err) => return Box::pin(async move { Err(err) }),
            }
        } else {
            ctx.cwd.clone()
        };
        let visible_path = ctx.visible_path(&path).to_string_lossy().to_string();

        Box::pin(async move {
            let entries = sandbox.list_dir(&path).await?;
            let mut items = Vec::new();

            for entry in entries {
                let file_type = entry.file_type().await?;
                let metadata = entry.metadata().await?;
                let name = entry.file_name().to_string_lossy().to_string();

                items.push(json!({
                    "name": name,
                    "type": if file_type.is_dir() { "directory" } else { "file" },
                    "size": metadata.len()
                }));
            }

            // Sort: directories first, then by name
            items.sort_by(|a, b| {
                let a_is_dir = a["type"] == "directory";
                let b_is_dir = b["type"] == "directory";
                match (a_is_dir, b_is_dir) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a["name"].as_str().cmp(&b["name"].as_str()),
                }
            });

            Ok(json!({
                "path": visible_path,
                "entries": items,
                "total": items.len()
            }))
        })
    }

    fn capability(&self, _args: &Value) -> alan_agent_protocol::ToolCapability {
        alan_agent_protocol::ToolCapability::Read
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn visible_host_path(path: &Path, mounts: &[alan_agent_engine::HostMountGrant]) -> String {
    if mounts.is_empty() {
        return path.to_string_lossy().to_string();
    }
    mounts
        .iter()
        .filter_map(|grant| {
            let requested =
                dunce::canonicalize(path).unwrap_or_else(|_| dunce::simplified(path).to_path_buf());
            let root = dunce::canonicalize(&grant.host_path)
                .unwrap_or_else(|_| dunce::simplified(&grant.host_path).to_path_buf());
            let suffix = requested.strip_prefix(&root).ok()?;
            Some((
                root.components().count(),
                Path::new(&grant.namespace_path).join(suffix),
            ))
        })
        .max_by_key(|(prefix_len, _)| *prefix_len)
        .map(|(_, path)| path.to_string_lossy().to_string())
        .unwrap_or_else(|| "<unmapped-host-path>".to_string())
}

fn project_host_paths(text: &str, mounts: &[alan_agent_engine::HostMountGrant]) -> String {
    let mut projected = text.to_string();
    let mut mounts = mounts.iter().collect::<Vec<_>>();
    mounts.sort_by_key(|grant| std::cmp::Reverse(grant.host_path.as_os_str().len()));
    for grant in mounts {
        for root in [
            grant.host_path.clone(),
            dunce::canonicalize(&grant.host_path).unwrap_or_else(|_| grant.host_path.clone()),
        ] {
            projected = projected.replace(root.to_string_lossy().as_ref(), &grant.namespace_path);
        }
    }
    projected
}

fn is_binary_file(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        matches!(
            ext.as_str(),
            "exe"
                | "dll"
                | "so"
                | "dylib"
                | "bin"
                | "o"
                | "a"
                | "zip"
                | "tar"
                | "gz"
                | "png"
                | "jpg"
                | "jpeg"
                | "gif"
                | "webp"
                | "mp3"
                | "mp4"
                | "pdf"
        )
    } else {
        false
    }
}

// ============================================================================
// Factory
// ============================================================================

/// Register built-in tool catalog factories.
fn register_builtin_tool_factories(registry: &mut ToolRegistry) {
    registry.register_tool_factory("read_file", || Box::new(ReadFileTool::new()));
    registry.register_tool_factory("write_file", || Box::new(WriteFileTool::new()));
    registry.register_tool_factory("edit_file", || Box::new(EditFileTool::new()));
    registry.register_tool_factory("bash", || Box::new(BashTool::new()));
    registry.register_tool_factory("grep", || Box::new(GrepTool::new()));
    registry.register_tool_factory("glob", || Box::new(GlobTool::new()));
    registry.register_tool_factory("list_dir", || Box::new(ListDirTool::new()));
}

/// Register the built-in tool catalog on an existing registry.
pub fn register_builtin_tool_catalog(registry: &mut ToolRegistry) {
    register_builtin_tool_factories(registry);
}

/// Create the default core toolset (4 tools).
///
/// Core tools:
/// - read_file
/// - write_file
/// - edit_file
/// - bash
pub fn create_core_tools() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(ReadFileTool::new()),
        Box::new(WriteFileTool::new()),
        Box::new(EditFileTool::new()),
        Box::new(BashTool::new()),
    ]
}

/// Create the read-only exploration toolset (4 tools).
///
/// Read-only tools:
/// - read_file
/// - grep
/// - glob
/// - list_dir
pub fn create_read_only_tools() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(ReadFileTool::new()),
        Box::new(GrepTool::new()),
        Box::new(GlobTool::new()),
        Box::new(ListDirTool::new()),
    ]
}

/// Create all 7 built-in tools.
pub fn create_all_tools() -> Vec<Box<dyn Tool>> {
    let mut tools = create_core_tools();
    tools.push(Box::new(GrepTool::new()));
    tools.push(Box::new(GlobTool::new()));
    tools.push(Box::new(ListDirTool::new()));
    tools
}

/// Create a ToolRegistry with the 4 core tools pre-registered.
pub fn create_tool_registry_with_core_tools(host_root: std::path::PathBuf) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    register_builtin_tool_catalog(&mut registry);
    registry.set_default_execution_binding(
        alan_agent_engine::tools::ToolExecutionBinding::new(
            host_root.clone(),
            host_root.join(".alan-runtime-tmp"),
        )
        .with_sandbox_spec(alan_agent_engine::tools::SandboxSpec::seed(host_root)),
    );

    for tool in create_core_tools() {
        registry.register_boxed(tool);
    }

    registry
}

/// Create a ToolRegistry with the 4 read-only tools pre-registered.
pub fn create_tool_registry_with_read_only_tools(host_root: std::path::PathBuf) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    register_builtin_tool_catalog(&mut registry);
    registry.set_default_execution_binding(
        alan_agent_engine::tools::ToolExecutionBinding::new(
            host_root.clone(),
            host_root.join(".alan-runtime-tmp"),
        )
        .with_sandbox_spec(alan_agent_engine::tools::SandboxSpec::seed(host_root)),
    );

    for tool in create_read_only_tools() {
        registry.register_boxed(tool);
    }

    registry
}

/// Create a ToolRegistry with all 7 built-in tools pre-registered.
pub fn create_tool_registry_with_all_tools(host_root: std::path::PathBuf) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    register_builtin_tool_catalog(&mut registry);
    registry.set_default_execution_binding(
        alan_agent_engine::tools::ToolExecutionBinding::new(
            host_root.clone(),
            host_root.join(".alan-runtime-tmp"),
        )
        .with_sandbox_spec(alan_agent_engine::tools::SandboxSpec::seed(host_root)),
    );

    for tool in create_all_tools() {
        registry.register_boxed(tool);
    }

    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_agent_engine::Config;
    use alan_agent_engine::tools::ToolContext;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn tool_context_with_root(
        root: PathBuf,
        scratch_dir: PathBuf,
        config: Arc<Config>,
    ) -> ToolContext {
        ToolContext::from_binding(
            alan_agent_engine::tools::ToolExecutionBinding::new(root.clone(), scratch_dir)
                .with_sandbox_spec(alan_agent_engine::tools::SandboxSpec::seed(root)),
            config,
        )
    }

    #[tokio::test]
    async fn test_read_file_tool() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        // Create test file
        tokio::fs::write(mount_root.join("test.txt"), "line1\nline2\nline3\n")
            .await
            .unwrap();

        let tool = ReadFileTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"path": "test.txt"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["type"], "text");
        assert!(result["content"].as_str().unwrap().contains("line1"));
    }

    #[tokio::test]
    async fn test_read_file_tool_uses_mount_root_binding_from_context() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().join("mount_root");
        tokio::fs::create_dir_all(&mount_root).await.unwrap();
        tokio::fs::write(mount_root.join("test.txt"), "bound\n")
            .await
            .unwrap();

        let tool = ReadFileTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"path": "test.txt"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["path"], json!(mount_root.join("test.txt")));
        assert_eq!(result["content"], json!("bound"));
    }

    #[tokio::test]
    async fn test_read_file_tool_requires_explicit_sandbox_grant() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();
        tokio::fs::write(mount_root.join("test.txt"), "hello\n")
            .await
            .unwrap();

        let tool = ReadFileTool::new();
        let config = Arc::new(Config::default());
        let ctx = ToolContext::new(mount_root.clone(), mount_root.join("tmp"), config);

        let err = tool
            .execute(json!({"path": "test.txt"}), &ctx)
            .await
            .unwrap_err();

        assert!(
            err.to_string()
                .contains("Tool Process has no explicit sandbox grant")
        );
    }

    #[tokio::test]
    async fn test_read_file_with_offset_and_limit() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        tokio::fs::write(
            mount_root.join("lines.txt"),
            "line1\nline2\nline3\nline4\nline5\n",
        )
        .await
        .unwrap();

        let tool = ReadFileTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        // Read from line 2, max 2 lines
        let args = json!({"path": "lines.txt", "offset": 2, "limit": 2});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["content"], "line2\nline3");
        assert_eq!(result["start_line"], 2);
        assert_eq!(result["end_line"], 3);
        assert_eq!(result["total_lines"], 5);
        assert!(result["truncated"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn test_read_file_offset_beyond_content() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        tokio::fs::write(mount_root.join("short.txt"), "one line")
            .await
            .unwrap();

        let tool = ReadFileTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"path": "short.txt", "offset": 10});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["content"], "");
        assert_eq!(result["total_lines"], 1);
    }

    #[tokio::test]
    async fn test_read_file_not_found() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        let tool = ReadFileTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"path": "nonexistent.txt"});
        let result = tool.execute(args, &ctx).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_read_image_file() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        // Create a fake PNG file (just the header bytes)
        let png_header = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        tokio::fs::write(mount_root.join("test.png"), png_header)
            .await
            .unwrap();

        let tool = ReadFileTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"path": "test.png"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["type"], "image");
        assert_eq!(result["mime_type"], "image/png");
        assert_eq!(result["size_bytes"], 8);
    }

    #[tokio::test]
    async fn test_write_and_read_file() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        let write_tool = WriteFileTool::new();
        let read_tool = ReadFileTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        // Write
        let write_args = json!({"path": "output.txt", "content": "Hello World"});
        let write_result = write_tool.execute(write_args, &ctx).await.unwrap();
        assert!(write_result["success"].as_bool().unwrap());

        // Read back
        let read_args = json!({"path": "output.txt"});
        let read_result = read_tool.execute(read_args, &ctx).await.unwrap();
        assert_eq!(read_result["content"], "Hello World");
    }

    #[tokio::test]
    async fn test_write_file_creates_parent_dirs() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        let tool = WriteFileTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"path": "a/b/c/deep.txt", "content": "deep content"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert!(result["success"].as_bool().unwrap());

        // Verify file exists
        let content = tokio::fs::read_to_string(mount_root.join("a/b/c/deep.txt"))
            .await
            .unwrap();
        assert_eq!(content, "deep content");
    }

    #[tokio::test]
    async fn test_write_file_empty_content() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        let tool = WriteFileTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"path": "empty.txt", "content": ""});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert!(result["success"].as_bool().unwrap());
        assert_eq!(result["bytes_written"], 0);
    }

    #[tokio::test]
    async fn test_write_file_overwrites_existing() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        // Create existing file
        tokio::fs::write(mount_root.join("existing.txt"), "old content")
            .await
            .unwrap();

        let tool = WriteFileTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"path": "existing.txt", "content": "new content"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert!(result["success"].as_bool().unwrap());

        let content = tokio::fs::read_to_string(mount_root.join("existing.txt"))
            .await
            .unwrap();
        assert_eq!(content, "new content");
    }

    #[tokio::test]
    async fn test_edit_file() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        // Create file
        tokio::fs::write(mount_root.join("edit.txt"), "Hello World")
            .await
            .unwrap();

        let edit_tool = EditFileTool::new();
        let read_tool = ReadFileTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        // Edit
        let edit_args = json!({"path": "edit.txt", "old_string": "World", "new_string": "Rust"});
        let edit_result = edit_tool.execute(edit_args, &ctx).await.unwrap();
        assert!(edit_result["success"].as_bool().unwrap());

        // Verify
        let read_args = json!({"path": "edit.txt"});
        let read_result = read_tool.execute(read_args, &ctx).await.unwrap();
        assert_eq!(read_result["content"], "Hello Rust");
    }

    #[tokio::test]
    async fn test_edit_file_not_found() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        let tool = EditFileTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({
            "path": "nonexistent.txt",
            "old_string": "old",
            "new_string": "new"
        });
        let result = tool.execute(args, &ctx).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_edit_file_old_string_not_found() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        tokio::fs::write(mount_root.join("file.txt"), "content here")
            .await
            .unwrap();

        let tool = EditFileTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({
            "path": "file.txt",
            "old_string": "not present",
            "new_string": "replacement"
        });
        let result = tool.execute(args, &ctx).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_edit_file_multiline_replacement() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        tokio::fs::write(mount_root.join("multi.txt"), "start\nmiddle\nend")
            .await
            .unwrap();

        let tool = EditFileTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({
            "path": "multi.txt",
            "old_string": "start\nmiddle",
            "new_string": "begin\ncenter"
        });
        let result = tool.execute(args, &ctx).await.unwrap();

        assert!(result["success"].as_bool().unwrap());

        let content = tokio::fs::read_to_string(mount_root.join("multi.txt"))
            .await
            .unwrap();
        assert_eq!(content, "begin\ncenter\nend");
    }

    #[tokio::test]
    async fn test_edit_file_only_first_occurrence() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        tokio::fs::write(mount_root.join("repeat.txt"), "foo foo foo")
            .await
            .unwrap();

        let tool = EditFileTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({
            "path": "repeat.txt",
            "old_string": "foo",
            "new_string": "bar"
        });
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["replacements"], 1);

        let content = tokio::fs::read_to_string(mount_root.join("repeat.txt"))
            .await
            .unwrap();
        assert_eq!(content, "bar foo foo");
    }

    #[tokio::test]
    async fn test_bash_tool() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        let tool = BashTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"command": "echo test_output"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert!(result["success"].as_bool().unwrap());
        assert!(result["stdout"].as_str().unwrap().contains("test_output"));
    }

    #[tokio::test]
    async fn test_bash_tool_failure() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        let tool = BashTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"command": "exit 42"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert!(!result["success"].as_bool().unwrap());
        assert_eq!(result["exit_code"], 42);
    }

    #[tokio::test]
    async fn test_bash_tool_stderr() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        let tool = BashTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"command": "echo error_msg >&2"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert!(result["success"].as_bool().unwrap());
        assert!(result["stderr"].as_str().unwrap().contains("error_msg"));
    }

    #[tokio::test]
    async fn test_bash_tool_working_directory() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        // Create subdirectory
        tokio::fs::create_dir(mount_root.join("subdir"))
            .await
            .unwrap();

        let tool = BashTool::new();
        let config = Arc::new(Config::default());
        let ctx = ToolContext::from_binding(
            alan_agent_engine::tools::ToolExecutionBinding::new(
                mount_root.join("subdir"),
                mount_root.join("tmp"),
            )
            .with_sandbox_spec(alan_agent_engine::tools::SandboxSpec::seed(
                mount_root.clone(),
            )),
            config,
        );

        let args = json!({"command": "pwd"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert!(result["stdout"].as_str().unwrap().contains("subdir"));
    }

    #[tokio::test]
    async fn test_grep_tool() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        // Create test file
        tokio::fs::write(
            mount_root.join("search.txt"),
            "hello world\nfoo bar\nhello rust",
        )
        .await
        .unwrap();

        let tool = GrepTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "hello", "path": "search.txt"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 2);
    }

    #[tokio::test]
    async fn test_grep_tool_case_insensitive() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        tokio::fs::write(mount_root.join("case.txt"), "Hello\nHELLO\nhello")
            .await
            .unwrap();

        let tool = GrepTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "hello", "path": "case.txt", "case_sensitive": false});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 3);
    }

    #[tokio::test]
    async fn test_grep_tool_case_sensitive() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        tokio::fs::write(mount_root.join("case.txt"), "Hello\nHELLO\nhello")
            .await
            .unwrap();

        let tool = GrepTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "hello", "path": "case.txt", "case_sensitive": true});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 1);
        assert_eq!(result["matches"][0]["content"], "hello");
    }

    #[tokio::test]
    async fn test_grep_tool_directory_recursive() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        tokio::fs::create_dir(mount_root.join("src")).await.unwrap();
        tokio::fs::write(mount_root.join("src/a.rs"), "fn main() {}")
            .await
            .unwrap();
        tokio::fs::write(mount_root.join("src/b.rs"), "fn helper() {}")
            .await
            .unwrap();
        tokio::fs::write(mount_root.join("root.txt"), "fn root() {}")
            .await
            .unwrap();

        let tool = GrepTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "fn ", "path": "."});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 3);
    }

    #[tokio::test]
    async fn test_grep_tool_no_matches() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        tokio::fs::write(mount_root.join("file.txt"), "content here")
            .await
            .unwrap();

        let tool = GrepTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "nomatch", "path": "file.txt"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 0);
        assert!(result["matches"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_grep_tool_invalid_regex() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        let tool = GrepTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "[invalid", "path": "."});
        let result = tool.execute(args, &ctx).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid regex"));
    }

    #[tokio::test]
    async fn test_grep_tool_skips_hidden_dirs() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        tokio::fs::create_dir(mount_root.join(".hidden"))
            .await
            .unwrap();
        tokio::fs::write(mount_root.join(".hidden/secret.txt"), "secret content")
            .await
            .unwrap();
        tokio::fs::write(mount_root.join("visible.txt"), "visible content")
            .await
            .unwrap();

        let tool = GrepTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "content", "path": "."});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 1);
        assert!(
            result["matches"][0]["path"]
                .as_str()
                .unwrap()
                .contains("visible.txt")
        );
    }

    #[tokio::test]
    async fn test_grep_tool_skips_binary_files() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        // Create a binary file with some pattern in it
        let binary_content = vec![0x00, 0x01, 0x02, 0x03];
        tokio::fs::write(mount_root.join("data.bin"), binary_content)
            .await
            .unwrap();
        tokio::fs::write(mount_root.join("text.txt"), "test data")
            .await
            .unwrap();

        let tool = GrepTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "data", "path": "."});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 1);
        assert!(
            result["matches"][0]["path"]
                .as_str()
                .unwrap()
                .contains("text.txt")
        );
    }

    #[tokio::test]
    async fn test_glob_tool() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        tokio::fs::write(mount_root.join("a.rs"), "").await.unwrap();
        tokio::fs::write(mount_root.join("b.rs"), "").await.unwrap();
        tokio::fs::write(mount_root.join("c.txt"), "")
            .await
            .unwrap();

        let tool = GlobTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "*.rs"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 2);
    }

    #[tokio::test]
    async fn test_glob_tool_recursive() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        tokio::fs::create_dir(mount_root.join("src")).await.unwrap();
        tokio::fs::create_dir(mount_root.join("src/nested"))
            .await
            .unwrap();
        tokio::fs::write(mount_root.join("src/a.rs"), "")
            .await
            .unwrap();
        tokio::fs::write(mount_root.join("src/nested/b.rs"), "")
            .await
            .unwrap();
        tokio::fs::write(mount_root.join("root.rs"), "")
            .await
            .unwrap();

        let tool = GlobTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "**/*.rs"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 3);
    }

    #[tokio::test]
    async fn test_glob_tool_with_path() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        tokio::fs::create_dir(mount_root.join("subdir"))
            .await
            .unwrap();
        tokio::fs::write(mount_root.join("subdir/file.txt"), "")
            .await
            .unwrap();
        tokio::fs::write(mount_root.join("root.txt"), "")
            .await
            .unwrap();

        let tool = GlobTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "*.txt", "path": "subdir"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 1);
        assert!(result["matches"][0].as_str().unwrap().contains("subdir"));
    }

    #[tokio::test]
    async fn test_glob_tool_no_matches() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        let tool = GlobTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "*.nonexistent"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 0);
        assert!(result["matches"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_list_dir_tool() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        // Create some files
        tokio::fs::write(mount_root.join("file1.txt"), "")
            .await
            .unwrap();
        tokio::fs::create_dir(mount_root.join("dir1"))
            .await
            .unwrap();

        let tool = ListDirTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"path": "."});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 2);
    }

    #[tokio::test]
    async fn test_list_dir_default_path() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        tokio::fs::write(mount_root.join("file.txt"), "")
            .await
            .unwrap();

        let tool = ListDirTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        // No path argument, should use cwd
        let args = json!({});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 1);
    }

    #[tokio::test]
    async fn test_list_dir_empty() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        let tool = ListDirTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"path": "."});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 0);
        assert!(result["entries"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_list_dir_sorting() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        // Create files and dirs in non-sorted order
        tokio::fs::write(mount_root.join("z.txt"), "")
            .await
            .unwrap();
        tokio::fs::create_dir(mount_root.join("a_dir"))
            .await
            .unwrap();
        tokio::fs::write(mount_root.join("m.txt"), "")
            .await
            .unwrap();
        tokio::fs::create_dir(mount_root.join("z_dir"))
            .await
            .unwrap();

        let tool = ListDirTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"path": "."});
        let result = tool.execute(args, &ctx).await.unwrap();

        let entries = result["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 4);
        // Directories first, sorted alphabetically
        assert_eq!(entries[0]["name"], "a_dir");
        assert_eq!(entries[0]["type"], "directory");
        assert_eq!(entries[1]["name"], "z_dir");
        assert_eq!(entries[1]["type"], "directory");
        // Then files
        assert_eq!(entries[2]["name"], "m.txt");
        assert_eq!(entries[2]["type"], "file");
        assert_eq!(entries[3]["name"], "z.txt");
        assert_eq!(entries[3]["type"], "file");
    }

    #[tokio::test]
    async fn test_list_dir_not_found() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        let tool = ListDirTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"path": "nonexistent"});
        let result = tool.execute(args, &ctx).await;

        assert!(result.is_err());
    }

    // Helper function tests
    #[test]
    fn test_is_image() {
        assert!(is_image(Path::new("test.png")));
        assert!(is_image(Path::new("test.jpg")));
        assert!(is_image(Path::new("test.JPEG")));
        assert!(is_image(Path::new("test.gif")));
        assert!(is_image(Path::new("test.webp")));
        assert!(is_image(Path::new("test.svg")));
        assert!(is_image(Path::new("test.bmp")));
        assert!(!is_image(Path::new("test.txt")));
        assert!(!is_image(Path::new("test")));
        assert!(!is_image(Path::new("")));
    }

    #[test]
    fn test_detect_mime() {
        assert_eq!(detect_mime(Path::new("test.png")), "image/png");
        assert_eq!(detect_mime(Path::new("test.jpg")), "image/jpeg");
        assert_eq!(detect_mime(Path::new("test.jpeg")), "image/jpeg");
        assert_eq!(detect_mime(Path::new("test.gif")), "image/gif");
        assert_eq!(detect_mime(Path::new("test.webp")), "image/webp");
        assert_eq!(detect_mime(Path::new("test.svg")), "image/svg+xml");
        assert_eq!(detect_mime(Path::new("test.bmp")), "image/bmp");
        assert_eq!(
            detect_mime(Path::new("test.unknown")),
            "application/octet-stream"
        );
        assert_eq!(detect_mime(Path::new("test")), "application/octet-stream");
    }

    #[test]
    fn test_is_binary_file() {
        assert!(is_binary_file(Path::new("test.exe")));
        assert!(is_binary_file(Path::new("test.dll")));
        assert!(is_binary_file(Path::new("test.so")));
        assert!(is_binary_file(Path::new("test.dylib")));
        assert!(is_binary_file(Path::new("test.bin")));
        assert!(is_binary_file(Path::new("test.o")));
        assert!(is_binary_file(Path::new("test.a")));
        assert!(is_binary_file(Path::new("test.zip")));
        assert!(is_binary_file(Path::new("test.tar")));
        assert!(is_binary_file(Path::new("test.gz")));
        assert!(is_binary_file(Path::new("test.png")));
        assert!(is_binary_file(Path::new("test.pdf")));
        assert!(!is_binary_file(Path::new("test.txt")));
        assert!(!is_binary_file(Path::new("test.rs")));
        assert!(!is_binary_file(Path::new("test")));
    }

    // Tool trait method tests
    #[test]
    fn test_read_file_tool_metadata() {
        let tool = ReadFileTool::new();
        assert_eq!(tool.name(), "read_file");
        assert_eq!(
            tool.capability(&json!({})),
            alan_agent_protocol::ToolCapability::Read
        );
    }

    #[test]
    fn test_write_file_tool_metadata() {
        let tool = WriteFileTool::new();
        assert_eq!(tool.name(), "write_file");
        assert_eq!(
            tool.capability(&json!({})),
            alan_agent_protocol::ToolCapability::Write
        );
    }

    #[test]
    fn test_edit_file_tool_metadata() {
        let tool = EditFileTool::new();
        assert_eq!(tool.name(), "edit_file");
        assert_eq!(
            tool.capability(&json!({})),
            alan_agent_protocol::ToolCapability::Write
        );
    }

    #[test]
    fn test_bash_tool_metadata() {
        let tool = BashTool::new();
        assert_eq!(tool.name(), "bash");
        assert_eq!(
            tool.capability(&json!({"command":"ls -la"})),
            alan_agent_protocol::ToolCapability::Read
        );
        assert_eq!(
            tool.capability(&json!({"command":"mkdir build"})),
            alan_agent_protocol::ToolCapability::Write
        );
        assert_eq!(
            tool.capability(&json!({"command":"curl https://example.com"})),
            alan_agent_protocol::ToolCapability::Network
        );
        assert_eq!(tool.timeout_secs(), 300);
    }

    #[test]
    fn test_bash_tool_description_warns_about_eval_wrappers() {
        let tool = BashTool::new();
        let description = tool.description();
        assert!(description.contains("python -c"));
        assert!(description.contains("bash -c"));
        assert!(description.contains("Prefer direct commands"));
    }

    #[test]
    fn test_classify_bash_command_priority_network_over_write() {
        let cap = classify_bash_command("mkdir out && curl https://example.com");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
    }

    #[test]
    fn test_classify_path_qualified_network_tool() {
        // Path-qualified executables classify by basename so an approved network
        // call isn't run with the sandbox network deny still in force.
        assert_eq!(
            classify_bash_command("/usr/bin/curl example.com"),
            alan_agent_protocol::ToolCapability::Network
        );
        assert_eq!(
            classify_bash_command("/usr/bin/wget https://example.com/x"),
            alan_agent_protocol::ToolCapability::Network
        );
        // Path-qualified write tools likewise classify by basename.
        assert_eq!(
            classify_bash_command("/bin/rm file.txt"),
            alan_agent_protocol::ToolCapability::Write
        );
        // Path-qualified git subcommands classify via the basename gate too.
        assert_eq!(
            classify_bash_command("/usr/bin/git -C repo push"),
            alan_agent_protocol::ToolCapability::Network
        );
        assert_eq!(
            classify_bash_command("/usr/bin/git fetch origin"),
            alan_agent_protocol::ToolCapability::Network
        );
    }

    #[test]
    fn test_classify_bash_command_write() {
        let cap = classify_bash_command("git reset --hard");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_read() {
        let cap = classify_bash_command("rg TODO src");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_treats_regex_pipe_inside_quotes_as_read() {
        let cap = classify_bash_command(
            "rg -n \"resolve_redirects|303|307|308|redirect\" requests tests",
        );
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_treats_cd_then_read_as_read() {
        let cap = classify_bash_command("cd /tmp/repo && ls");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_treats_cd_then_network_as_network() {
        let cap = classify_bash_command("cd /tmp/repo && curl https://example.com");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
    }

    #[test]
    fn test_classify_bash_command_treats_cd_then_write_as_write() {
        let cap = classify_bash_command("cd /tmp/repo && rm -f artifact.txt");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_defaults_ambiguous_python_eval_to_unknown() {
        let cap = classify_bash_command("python -c \"print('hi')\"");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_python_script_file_as_unknown() {
        let cap = classify_bash_command("python3 script.py");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_shell_script_file_as_unknown() {
        let cap = classify_bash_command("bash script.sh");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_awk_script_file_as_unknown() {
        let cap = classify_bash_command("awk -f script.awk input.txt");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_shell_eval_wrappers_as_unknown() {
        let cap = classify_bash_command("bash -lc \"rg TODO src\"");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_shell_eval_wrappers_with_leading_options_as_unknown() {
        let cap = classify_bash_command("bash --noprofile -c 'rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_python_eval_wrappers_with_leading_options_as_unknown() {
        let cap = classify_bash_command("python3 -B -c 'print(\"hi\")'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_node_inline_long_eval_wrapper_as_unknown() {
        let cap =
            classify_bash_command("node --eval='require(\"fs\").writeFileSync(\"x\", \"y\")'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_shell_inline_long_command_wrapper_as_unknown() {
        let cap = classify_bash_command("sh --command='rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_eval_wrapper_with_line_continuation_as_unknown() {
        let cap = classify_bash_command("s\\\nh -c 'rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_node_print_eval_wrappers_as_unknown() {
        let cap = classify_bash_command(
            "node --trace-warnings -p 'require(\"fs\").writeFileSync(\"x\", \"y\")'",
        );
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_allows_literal_sh_dash_c_arguments() {
        let cap = classify_bash_command("printf '%s %s' sh -c");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_treats_multiline_eval_wrapper_as_unknown() {
        let cap = classify_bash_command("echo ok\nsh -c 'rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_post_comment_line_continuation_network_as_network() {
        let cap = classify_bash_command("echo ok #\\\ncurl https://example.com");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
    }

    #[test]
    fn test_classify_bash_command_treats_env_shell_eval_wrappers_as_unknown() {
        let cap = classify_bash_command("env FOO=bar sh -c 'rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_bang_prefixed_shell_eval_as_unknown() {
        let cap = classify_bash_command("! sh -c 'rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_then_prefixed_shell_eval_as_unknown() {
        let cap = classify_bash_command("if true; then sh -c 'rg TODO src'; fi");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_command_wrapper_shell_eval_as_unknown() {
        let cap = classify_bash_command("command -p sh -c 'rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_nice_wrapper_as_unknown() {
        let cap = classify_bash_command("nice -n 5 sh -c 'rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_time_wrapper_as_unknown() {
        let cap = classify_bash_command("time sh -c 'rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_command_query_mode_as_unknown() {
        let cap = classify_bash_command("command -v sh -c");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_timeout_query_mode_as_unknown() {
        let cap = classify_bash_command("timeout --version");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_timeout_query_with_line_continuation_as_unknown() {
        let cap = classify_bash_command("time\\\nout --ver\\\nsion");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_builtin_query_mode_as_unknown() {
        let cap = classify_bash_command("builtin -p eval");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_exec_wrapper_shell_eval_with_argv0_as_unknown() {
        let cap = classify_bash_command("exec -a alan sh -c 'rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_stdbuf_wrapped_read_command_as_unknown() {
        let cap = classify_bash_command("stdbuf -oL rg TODO src");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_env_split_string_as_unknown() {
        let cap = classify_bash_command("env -S 'sh -c rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_clustered_env_split_string_as_unknown() {
        let cap = classify_bash_command("env -iS 'sh -c rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_direct_command_with_leading_env_assignment_as_read() {
        let cap = classify_bash_command("ALAN_TEST=1 rg TODO src");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_redirection_without_whitespace_is_write() {
        let cap = classify_bash_command("echo x>.git/config");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_git_fetch_is_network() {
        let cap = classify_bash_command("git fetch origin main");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
    }

    #[test]
    fn test_classify_bash_command_git_fetch_with_global_options_is_network() {
        let cap = classify_bash_command("git -C /tmp/repo fetch --depth=1 origin main");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
    }

    #[test]
    fn test_classify_bash_command_git_rev_parse_with_global_options_is_read() {
        let cap = classify_bash_command("git -C /tmp/repo rev-parse --verify --quiet head");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_git_submodule_status_is_read() {
        let cap = classify_bash_command("git submodule status");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_git_submodule_init_is_write() {
        let cap = classify_bash_command("git submodule init");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_git_submodule_update_is_network() {
        let cap = classify_bash_command("git submodule update");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
    }

    #[test]
    fn test_classify_bash_command_git_submodule_update_no_fetch_is_write() {
        let cap = classify_bash_command("git submodule update --no-fetch");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_git_mutations_are_write() {
        let cap = classify_bash_command("git add .");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_git_branch_creation_is_write() {
        let cap = classify_bash_command("git branch release");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_git_branch_list_with_global_options_is_read() {
        let cap = classify_bash_command("git -C /tmp/repo branch --list");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_git_branch_edit_description_is_write() {
        let cap = classify_bash_command("git branch --edit-description");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_git_tag_creation_is_write() {
        let cap = classify_bash_command("git tag v1.2.3");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_git_tag_list_with_global_options_is_read() {
        let cap = classify_bash_command("git -C /tmp/repo tag --list");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_git_remote_add_is_write() {
        let cap =
            classify_bash_command("git remote add origin git@github.com:realmorrisliu/Alan.git");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_git_remote_add_fetch_is_network() {
        let cap = classify_bash_command("git remote add -f origin https://example.com/repo.git");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
    }

    #[test]
    fn test_classify_bash_command_git_remote_add_long_fetch_is_network() {
        let cap =
            classify_bash_command("git remote add --fetch origin https://example.com/repo.git");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
    }

    #[test]
    fn test_classify_bash_command_git_ls_remote_is_network() {
        let cap = classify_bash_command("git ls-remote origin");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
    }

    #[test]
    fn test_classify_bash_command_git_push_is_network() {
        let cap = classify_bash_command("git push origin main");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
    }

    #[test]
    fn test_classify_bash_command_sed_in_place_is_write() {
        let cap = classify_bash_command("sed -i 's/foo/bar/' src/lib.rs");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_sed_clustered_ei_is_write() {
        let cap = classify_bash_command("sed -Ei 's/foo/bar/' src/lib.rs");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_sed_clustered_ni_is_write() {
        let cap = classify_bash_command("sed -ni 's/foo/bar/' src/lib.rs");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_find_exec_is_write() {
        let cap = classify_bash_command("find . -name '*.tmp' -exec rm -f {} \\;");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_find_fprint_is_write() {
        let cap = classify_bash_command("find . -name '*.rs' -fprint /tmp/files.txt");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_find_fprint0_is_write() {
        let cap = classify_bash_command("find . -name '*.rs' -fprint0 /tmp/files.bin");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_find_name_defaults_to_read() {
        let cap = classify_bash_command("find . -name '*.rs'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_find_pipeline_is_read() {
        let cap = classify_bash_command(
            "find . -maxdepth 3 \\( -path './test*' -o -path './tests*' \\) -type d | sort",
        );
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_pytest_is_write() {
        let cap = classify_bash_command("pytest tests/test_requests.py -k redirect");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_python_module_pytest_is_write() {
        let cap = classify_bash_command("python -B -m pytest tests/test_requests.py -k redirect");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_local_runtests_script_is_write() {
        let cap = classify_bash_command("./tests/runtests.py utils_tests.test_html");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_python_local_runtests_script_is_write() {
        let cap = classify_bash_command("python3 -B tests/runtests.py utils_tests.test_html");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_manage_py_test_is_write() {
        let cap = classify_bash_command("python manage.py test auth_tests");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_manage_py_shell_stays_unknown() {
        let cap = classify_bash_command("python manage.py shell");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_local_gradle_test_wrapper_is_write() {
        let cap = classify_bash_command("./gradlew test");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_tox_version_is_read() {
        let cap = classify_bash_command("tox --version");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_nox_help_is_read() {
        let cap = classify_bash_command("nox --help");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_tox_run_is_write() {
        let cap = classify_bash_command("tox -e py");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_nox_run_is_write() {
        let cap = classify_bash_command("nox -s tests");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_python_version_is_read() {
        let cap = classify_bash_command("python --version");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_sed_print_is_read() {
        let cap = classify_bash_command("sed -n '1,80p' test_requests.py");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_sed_substitute_is_read() {
        let cap = classify_bash_command("sed 's#^./##' test_requests.py");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_read_only_find_sed_pipeline_is_read() {
        let cap = classify_bash_command(
            "find . -maxdepth 2 -type f | sed 's#^./##' | sort | rg \"(^test|tests|requests/test)\"",
        );
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_sed_write_script_is_unknown() {
        let cap = classify_bash_command("sed -n '1,80w /tmp/out' test_requests.py");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_cargo_test_is_write() {
        let cap = classify_bash_command("cargo test -p alan-agent-engine delegated_skill --lib");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_grep_tool_metadata() {
        let tool = GrepTool::new();
        assert_eq!(tool.name(), "grep");
        assert_eq!(
            tool.capability(&json!({})),
            alan_agent_protocol::ToolCapability::Read
        );
    }

    #[test]
    fn test_glob_tool_metadata() {
        let tool = GlobTool::new();
        assert_eq!(tool.name(), "glob");
        assert_eq!(
            tool.capability(&json!({})),
            alan_agent_protocol::ToolCapability::Read
        );
    }

    #[test]
    fn test_list_dir_tool_metadata() {
        let tool = ListDirTool::new();
        assert_eq!(tool.name(), "list_dir");
        assert_eq!(
            tool.capability(&json!({})),
            alan_agent_protocol::ToolCapability::Read
        );
    }

    #[test]
    fn test_parameter_schemas_are_valid() {
        let tools: Vec<Box<dyn Tool>> = vec![
            Box::new(ReadFileTool::new()),
            Box::new(WriteFileTool::new()),
            Box::new(EditFileTool::new()),
            Box::new(BashTool::new()),
            Box::new(GrepTool::new()),
            Box::new(GlobTool::new()),
            Box::new(ListDirTool::new()),
        ];

        for tool in tools {
            let schema = tool.parameters_schema();
            assert_eq!(
                schema["type"],
                "object",
                "{} schema missing type",
                tool.name()
            );
            assert!(
                schema.get("properties").is_some(),
                "{} schema missing properties",
                tool.name()
            );
        }
    }

    #[test]
    fn test_create_core_tools() {
        let tools = create_core_tools();
        assert_eq!(tools.len(), 4);

        let tool_names: Vec<&str> = tools.iter().map(|tool| tool.name()).collect();
        assert!(tool_names.contains(&"read_file"));
        assert!(tool_names.contains(&"write_file"));
        assert!(tool_names.contains(&"edit_file"));
        assert!(tool_names.contains(&"bash"));
    }

    #[test]
    fn test_create_read_only_tools() {
        let tools = create_read_only_tools();
        assert_eq!(tools.len(), 4);

        let tool_names: Vec<&str> = tools.iter().map(|tool| tool.name()).collect();
        assert!(tool_names.contains(&"read_file"));
        assert!(tool_names.contains(&"grep"));
        assert!(tool_names.contains(&"glob"));
        assert!(tool_names.contains(&"list_dir"));
    }

    #[test]
    fn test_create_all_tools() {
        let tools = create_all_tools();
        assert_eq!(tools.len(), 7);
    }

    #[test]
    fn test_create_tool_registry_with_core_tools() {
        let registry = create_tool_registry_with_core_tools(PathBuf::from("/tmp"));
        assert!(registry.get("read_file").is_some());
        assert!(registry.get("write_file").is_some());
        assert!(registry.get("edit_file").is_some());
        assert!(registry.get("bash").is_some());
        assert!(registry.get("grep").is_none());
        assert!(registry.get("glob").is_none());
        assert!(registry.get("list_dir").is_none());
    }

    #[tokio::test]
    async fn test_core_registry_materializes_missing_read_only_tool_for_child_mount_root() {
        let temp = TempDir::new().unwrap();
        let parent_mount_root = temp.path().join("parent");
        let child_mount_root = temp.path().join("child");
        tokio::fs::create_dir_all(&parent_mount_root).await.unwrap();
        tokio::fs::create_dir_all(&child_mount_root).await.unwrap();
        tokio::fs::write(child_mount_root.join("notes.txt"), "mount_root inspect\n")
            .await
            .unwrap();

        let registry = create_tool_registry_with_core_tools(parent_mount_root);
        assert!(registry.get("grep").is_none());

        let grep_tool = registry
            .materialize("grep")
            .expect("core registry should materialize grep from the catalog");
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(
            child_mount_root.clone(),
            child_mount_root.join("tmp"),
            config,
        );
        let result = grep_tool
            .execute(
                json!({
                    "pattern": "inspect",
                    "path": "."
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert_eq!(result["total"], json!(1));
    }

    #[test]
    fn test_create_tool_registry_with_read_only_tools() {
        let registry = create_tool_registry_with_read_only_tools(PathBuf::from("/tmp"));
        assert!(registry.get("read_file").is_some());
        assert!(registry.get("grep").is_some());
        assert!(registry.get("glob").is_some());
        assert!(registry.get("list_dir").is_some());
        assert!(registry.get("write_file").is_none());
        assert!(registry.get("edit_file").is_none());
        assert!(registry.get("bash").is_none());
    }

    #[test]
    fn test_create_tool_registry_with_all_tools() {
        let registry = create_tool_registry_with_all_tools(PathBuf::from("/tmp"));
        assert!(registry.get("read_file").is_some());
        assert!(registry.get("write_file").is_some());
        assert!(registry.get("edit_file").is_some());
        assert!(registry.get("bash").is_some());
        assert!(registry.get("grep").is_some());
        assert!(registry.get("glob").is_some());
        assert!(registry.get("list_dir").is_some());
    }
}
