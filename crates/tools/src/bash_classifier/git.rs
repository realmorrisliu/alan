use super::wrappers::command_basename;

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

pub(super) fn is_git_network_command(tokens: &[&str]) -> bool {
    let Some((_, subcommand)) = git_subcommand(tokens) else {
        return false;
    };

    matches!(
        subcommand,
        "clone" | "fetch" | "pull" | "push" | "ls-remote"
    ) || is_git_remote_network(tokens)
        || is_git_submodule_network(tokens)
}

pub(super) fn is_git_read_command(tokens: &[&str]) -> bool {
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
