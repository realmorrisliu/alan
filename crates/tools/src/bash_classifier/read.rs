use super::git::is_git_read_command;
use super::wrappers::{command_basename, is_builtin_query, is_command_query};

pub(super) fn is_safe_read_command(tokens: &[&str]) -> bool {
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

fn is_python_query_command(tokens: &[&str]) -> bool {
    let head = command_basename(tokens.first().copied().unwrap_or_default());
    if !matches!(head, "python" | "python3") {
        return false;
    }

    is_tool_query_command(tokens)
}

pub(super) fn is_tool_query_command(tokens: &[&str]) -> bool {
    tokens
        .iter()
        .skip(1)
        .copied()
        .find(|token| !token.is_empty())
        .is_some_and(|arg| matches!(arg, "-h" | "--help" | "--version") || arg.starts_with("-V"))
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
