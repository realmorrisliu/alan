use super::command_interpreters::{
    leading_eval_flag, opaque_command_dispatcher_display, opaque_script_interpreter_display,
};
use super::command_options::{exact_or_inline_option_with_value, has_attached_option_value};
use anyhow::{Result, anyhow};
use std::path::Path;

pub(super) fn validate_nested_command_evaluators(
    commands: &[Vec<String>],
    backend_name: &str,
) -> Result<()> {
    for words in commands {
        let Some(view) = nested_evaluator_view(words) else {
            continue;
        };
        if let Some(display) = view.opaque_wrapper_display.as_deref() {
            return Err(anyhow!(
                "Sandbox backend {} rejects nested command evaluators like {} because inner paths cannot be validated safely",
                backend_name,
                display
            ));
        }
        if is_shell_eval_builtin(view.command) {
            return Err(anyhow!(
                "Sandbox backend {} rejects nested command evaluators like {} because inner paths cannot be validated safely",
                backend_name,
                view.display
            ));
        }
        if let Some(dispatcher) =
            opaque_command_dispatcher_display(&view.display, view.command, view.args)
        {
            return Err(anyhow!(
                "Sandbox backend {} rejects opaque command dispatchers like {} because child command paths cannot be validated safely",
                backend_name,
                dispatcher
            ));
        }
        if let Some(flag) = leading_eval_flag(view.command, view.args) {
            return Err(anyhow!(
                "Sandbox backend {} rejects nested command evaluators like {} {} because inner paths cannot be validated safely",
                backend_name,
                view.display,
                flag
            ));
        }
        if let Some(interpreter) =
            opaque_script_interpreter_display(&view.display, view.command, view.args)
        {
            return Err(anyhow!(
                "Sandbox backend {} rejects opaque script interpreters like {} because script bodies cannot be validated safely",
                backend_name,
                interpreter
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_direct_command_shapes(
    commands: &[Vec<String>],
    backend_name: &str,
) -> Result<()> {
    for words in commands {
        let Some(command_index) = words.iter().position(|word| !is_env_assignment(word)) else {
            continue;
        };

        let command_word = words[command_index].as_str();
        if is_shell_control_prefix(command_word) {
            return Err(anyhow!(
                "Sandbox backend {} rejects shell control flow like {} because host_mount_path_guard only supports direct commands with statically checkable paths",
                backend_name,
                command_word
            ));
        }

        let command = command_basename(command_word);
        if is_unsupported_shell_wrapper(command) {
            return Err(anyhow!(
                "Sandbox backend {} rejects shell wrappers like {} because host_mount_path_guard only supports direct commands with statically checkable paths",
                backend_name,
                command
            ));
        }
    }

    Ok(())
}

/// Extract the inline script of a shell wrapper command (`sh -c <script>`,
/// `bash -lc <script>`, …) so it can be recursively inspected. Returns `None`
/// for non-wrapper commands or wrappers without an inline script argument.
pub(super) fn shell_wrapper_inline_script(words: &[String]) -> Option<String> {
    // Peel transparent wrappers (`env VAR=x`, `command`, `timeout 5`, `nice`,
    // `nohup`, `stdbuf`, `setsid`, ...) so the inline script is found even when the
    // shell is not the direct head — e.g. `env bash -lc '...'`. Otherwise the
    // quoted script stays an opaque token and its `.git`/out-of-host_mount paths
    // escape the ProtectedOnly checks.
    let view = nested_evaluator_view(words)?;
    if !matches!(view.command, "sh" | "bash" | "zsh" | "dash" | "ksh") {
        return None;
    }
    // The script follows the first short-flag cluster containing `c` (e.g. `-c`,
    // `-lc`, `-ic`).
    let mut index = 0;
    while index < view.args.len() {
        let word = &view.args[index];
        if word.starts_with('-') && !word.starts_with("--") && word.contains('c') {
            return view.args.get(index + 1).cloned();
        }
        index += 1;
    }
    None
}

fn command_basename(command: &str) -> &str {
    Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(command)
}

struct NestedEvaluatorView<'a> {
    display: String,
    command: &'a str,
    args: &'a [String],
    opaque_wrapper_display: Option<String>,
}

fn nested_evaluator_view(words: &[String]) -> Option<NestedEvaluatorView<'_>> {
    let mut command_index = next_command_offset(words)?;
    let mut display = command_basename(&words[command_index]).to_string();

    loop {
        let command = command_basename(&words[command_index]);
        let args = &words[command_index + 1..];
        let next_offset = if command == "env" {
            if let Some(flag) = env_split_string_flag(args) {
                return Some(NestedEvaluatorView {
                    display: display.clone(),
                    command,
                    args,
                    opaque_wrapper_display: Some(format!("{display} {flag}")),
                });
            }
            env_command_offset(args)
        } else if is_transparent_command_wrapper(command) {
            transparent_wrapper_offset(command, args)
        } else {
            None
        };

        let Some(next_relative_offset) = next_offset else {
            return Some(NestedEvaluatorView {
                display,
                command,
                args,
                opaque_wrapper_display: None,
            });
        };

        command_index += 1 + next_relative_offset;
        display.push(' ');
        display.push_str(command_basename(&words[command_index]));
    }
}

fn next_command_offset(words: &[String]) -> Option<usize> {
    let mut index = 0;
    while let Some(word) = words.get(index).map(|word| word.as_str()) {
        if is_env_assignment(word) || is_shell_control_prefix(word) {
            index += 1;
            continue;
        }
        return Some(index);
    }
    None
}

fn env_command_offset(args: &[String]) -> Option<usize> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
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

fn transparent_wrapper_offset(command: &str, args: &[String]) -> Option<usize> {
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

fn command_wrapper_offset(args: &[String]) -> Option<usize> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
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

fn builtin_wrapper_offset(args: &[String]) -> Option<usize> {
    let mut index = 0;
    if let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if arg == "--" {
            index += 1;
        } else if builtin_query_flag(arg) {
            return None;
        }
    }

    args.get(index)?;
    Some(index)
}

fn exec_wrapper_offset(args: &[String]) -> Option<usize> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
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

fn nice_wrapper_offset(args: &[String]) -> Option<usize> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
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

fn nohup_wrapper_offset(args: &[String]) -> Option<usize> {
    let mut index = 0;
    if let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
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

fn timeout_wrapper_offset(args: &[String]) -> Option<usize> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
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

fn stdbuf_wrapper_offset(args: &[String]) -> Option<usize> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
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

fn setsid_wrapper_offset(args: &[String]) -> Option<usize> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
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

pub(super) fn is_env_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn is_shell_eval_builtin(command: &str) -> bool {
    matches!(command, "eval" | "." | "source")
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

fn is_transparent_command_wrapper(command: &str) -> bool {
    matches!(
        command,
        "command" | "builtin" | "exec" | "nice" | "nohup" | "timeout" | "stdbuf" | "setsid"
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

fn env_split_string_flag(args: &[String]) -> Option<&str> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
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
            'u' | 'C' => {
                return Some(if rest[index + ch.len_utf8()..].is_empty() {
                    EnvOptionBehavior::TakesNextArg
                } else {
                    EnvOptionBehavior::InlineValue
                });
            }
            'S' => {
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
