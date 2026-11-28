use std::path::Path;

pub(super) fn contains_unsupported_shell_form(tokens: &[&str]) -> bool {
    let Some(command_index) = tokens.iter().position(|word| !is_env_assignment(word)) else {
        return false;
    };

    let command_word = tokens[command_index];
    if is_shell_control_prefix(command_word) {
        return true;
    }

    let command = command_basename(command_word);
    if command == "env" {
        let args = &tokens[command_index + 1..];
        if env_split_string_flag(args).is_some() {
            return true;
        }
        let Some(cmd_offset) = env_command_offset(args) else {
            return false;
        };
        if !env_flags_are_safe_hygiene(&args[..cmd_offset]) {
            return true;
        }
        return contains_unsupported_shell_form(&args[cmd_offset..]);
    }

    is_unsupported_shell_wrapper(command)
}

/// Validates the tokens between `env` and the wrapped executable.
///
/// Permitted, in this order:
/// - zero or more safe `-u NAME` / `--unset NAME` / `-uNAME` / `--unset=NAME`
///   flags (see [`is_safe_env_unset_var`]),
/// - zero or more allowlisted diagnostic/build-hygiene `KEY=VALUE`
///   assignments (see [`is_safe_env_assignment`]),
/// - an optional trailing `--` that is immediately before the executable.
///
/// Fails closed: unknown flags, protected or unlisted variable names, unsafe
/// assignment values, a `--` that is not immediately before the executable, and
/// any flag that follows an assignment all reject the wrapper. This keeps
/// `env` from becoming an opaque escape hatch for injecting loader, toolchain,
/// shell-startup or PATH overrides into the wrapped command.
fn env_flags_are_safe_hygiene(flags: &[&str]) -> bool {
    let mut index = 0;
    let mut saw_assignment = false;
    while let Some(flag) = flags.get(index).copied() {
        if is_env_assignment(flag) {
            if !is_safe_env_assignment(flag) {
                return false;
            }
            saw_assignment = true;
            index += 1;
            continue;
        }
        if saw_assignment {
            // Once an assignment is seen, only further safe assignments may
            // precede the wrapped executable.
            return false;
        }
        if flag == "--" {
            // `--` is only permitted immediately before the wrapped executable,
            // never before env assignments inside `flags`.
            return index + 1 == flags.len();
        }
        if flag == "-u" || flag == "--unset" {
            let Some(name) = flags.get(index + 1).copied() else {
                return false;
            };
            if !is_safe_env_unset_var(name) {
                return false;
            }
            index += 2;
            continue;
        }
        if let Some(name) = flag
            .strip_prefix("-u")
            .or_else(|| flag.strip_prefix("--unset="))
            && is_safe_env_unset_var(name)
        {
            index += 1;
            continue;
        }
        return false;
    }
    true
}

/// Variable names that may be assigned through the `env` wrapper.
///
/// The list is a strict, case-insensitive allowlist of diagnostics, build
/// hygiene, terminal/locale and non-code-executing runtime knobs. Anything not
/// listed here fails closed, so loader (`LD_*`, `DYLD_*`), toolchain
/// (`RUSTFLAGS`, `RUSTC*`, `CARGO_TARGET_DIR`, `CARGO_HOME`), shell-startup
/// (`BASH_ENV`, `ENV`) and path (`PATH`) overrides are rejected.
const SAFE_ENV_ASSIGNMENT_NAMES: &[&str] = &[
    // Cargo/Rust build and diagnostic hygiene.
    "cargo_net_offline",
    "cargo_term_color",
    "cargo_term_quiet",
    "cargo_term_verbose",
    "cargo_term_progress_when",
    "cargo_incremental",
    "rust_backtrace",
    "rust_log",
    "rust_log_style",
    "rust_test_threads",
    "rust_test_nocapture",
    // Terminal/locale/CI hygiene.
    "ci",
    "no_color",
    "clicolor",
    "clicolor_force",
    "force_color",
    "colorterm",
    "term",
    "columns",
    "lines",
    "lang",
    "lc_all",
    "lc_ctype",
    "lc_collate",
    "lc_messages",
    "lc_numeric",
    "lc_time",
    "tz",
    // Non-code-executing runtime hygiene.
    "node_env",
    "pythondontwritebytecode",
    "pythonunbuffered",
    "pythonutf8",
    "pythonioencoding",
];

/// Characters permitted in a safe assignment value after stripping at most one
/// matching pair of outer quotes.
const SAFE_ENV_ASSIGNMENT_VALUE_CHARS: &[char] = &['_', '-', '.', ':', ',', '=', '/', '+'];

fn is_safe_env_assignment(assignment: &str) -> bool {
    let Some((name, value)) = assignment.split_once('=') else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    SAFE_ENV_ASSIGNMENT_NAMES.contains(&lower.as_str()) && is_safe_env_assignment_value(value)
}

fn is_safe_env_assignment_value(value: &str) -> bool {
    strip_matching_outer_quotes(value)
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || SAFE_ENV_ASSIGNMENT_VALUE_CHARS.contains(&ch))
}

/// Strips at most one matching pair of outer single or double quotes.
///
/// Unmatched quotes are left in place and then rejected by the value character
/// check, so `FOO="bar` or `FOO="a'b"` never pass.
fn strip_matching_outer_quotes(value: &str) -> &str {
    for quote in ['\'', '"'] {
        if value.len() >= 2 && value.starts_with(quote) && value.ends_with(quote) {
            return &value[1..value.len() - 1];
        }
    }
    value
}

fn is_safe_env_unset_var(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_')
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return false;
    }
    !is_protected_env_unset_var(name)
}

fn is_protected_env_unset_var(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "path"
            | "home"
            | "user"
            | "shell"
            | "tmpdir"
            | "cargo_home"
            | "rustup_home"
            | "rustup_toolchain"
            | "rustc"
            | "rustc_wrapper"
            | "rustc_workspace_wrapper"
            | "rustdoc"
            | "rustfmt"
            | "cargo_build_rustc"
            | "cargo_build_rustc_wrapper"
            | "cargo_build_rustc_workspace_wrapper"
            | "cargo_build_rustdoc"
    ) || lower.starts_with("ld_")
        || lower.starts_with("dyld_")
        || lower.starts_with("cargo_alias_")
}

pub(super) fn contains_nested_eval_wrapper(tokens: &[&str]) -> bool {
    let Some(view) = nested_eval_command_view(tokens) else {
        return false;
    };
    view.opaque_wrapper || leading_eval_flag(view.command, view.args).is_some()
}

pub(super) fn command_basename(command: &str) -> &str {
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

pub(super) fn effective_command_tokens<'a>(tokens: &'a [&'a str]) -> Vec<&'a str> {
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
        if let Some(skip) = redirection_token_span(arg) {
            index += skip;
            continue;
        }
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

    while let Some(arg) = args.get(index).copied() {
        if let Some(skip) = redirection_token_span(arg) {
            index += skip;
            continue;
        }
        if is_env_assignment(arg) {
            index += 1;
            continue;
        }
        break;
    }
    args.get(index)?;
    Some(index)
}

fn redirection_token_span(token: &str) -> Option<usize> {
    let rest = token
        .trim_start_matches(|ch: char| ch.is_ascii_digit())
        .strip_prefix('&')
        .unwrap_or_else(|| token.trim_start_matches(|ch: char| ch.is_ascii_digit()));
    if matches!(
        rest,
        ">" | ">>" | ">|" | "<>" | "<" | "<<" | "<<<" | "<&" | ">&"
    ) {
        Some(2)
    } else if rest.starts_with('>') || rest.starts_with('<') {
        Some(1)
    } else {
        None
    }
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

pub(super) fn is_wrapper_query_command(tokens: &[&str]) -> bool {
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

pub(super) fn is_command_query(tokens: &[&str]) -> bool {
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

pub(super) fn is_builtin_query(tokens: &[&str]) -> bool {
    tokens.get(1).copied().is_some_and(builtin_query_flag)
}
