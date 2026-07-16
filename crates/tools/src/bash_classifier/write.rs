use super::super::command_basename;
use super::git::{is_git_network_command, is_git_read_command};
use super::read::is_tool_query_command;
use std::path::Path;

pub(super) fn is_write_command(fragment: &str, tokens: &[&str]) -> bool {
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
