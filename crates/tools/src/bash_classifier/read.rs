use super::super::{
    command_basename, is_builtin_query, is_command_query, is_sed_safe_read_command,
};
use super::git::is_git_read_command;

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
