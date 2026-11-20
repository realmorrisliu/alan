//! Narrow standing delegation scopes projected from Host Mount grants.
//!
//! `GitPushDelegationScope` binds an authorized namespace mount root to exact
//! remote and branch patterns for ordinary non-destructive `git push` commands.
//! Force-pushing, ref rewriting (`+refspec`), and remote ref deletion (`:ref`,
//! `--delete`, `--mirror`, `--prune`) remain strictly outside standing scope.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Standing ordinary `git push` publication scope attached to an authorized mount.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitPushDelegationScope {
    /// Grant identifier owning this standing publication scope.
    pub grant_id: String,
    /// Host Mount namespace path root authorized for this grant (e.g. `/mnt/project`).
    pub namespace_root: PathBuf,
    /// Allowed git remote name when specified; `None` matches only the explicit remote `*`?
    /// When `None`, defaults to `origin`.
    #[serde(default)]
    pub remote: Option<String>,
    /// Exact branch/ref names or trailing-`*` prefix patterns (e.g. `"feat/*"`).
    /// An empty list grants NO branches (fail-closed).
    #[serde(default)]
    pub branches: Vec<String>,
}

/// Parsed ordinary `git push` invocation eligible for standing delegation checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedGitPushCommand {
    /// Target remote specified on the command line (or `origin` when explicit/omitted? wait: see parser).
    pub remote: Option<String>,
    /// Explicitly named refspecs (destination branch or local branch).
    pub refspecs: Vec<String>,
}

/// Parse a bash command string if and only if it is a single clean `git push`
/// invocation without shell chaining, substitution, or destructive push flags.
pub fn parse_safe_git_push(command: &str) -> Option<ParsedGitPushCommand> {
    let trimmed = command.trim();
    if trimmed.is_empty()
        || trimmed.contains('\n')
        || trimmed.contains(';')
        || trimmed.contains("&&")
        || trimmed.contains("||")
        || trimmed.contains('|')
        || trimmed.contains('`')
        || trimmed.contains("$(")
    {
        return None;
    }

    let raw_tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let tokens: Vec<String> = raw_tokens
        .iter()
        .map(|t| t.trim_matches(['\'', '"']).to_string())
        .collect();
    if tokens.is_empty() {
        return None;
    }

    // Locate `git ... push` at command head.
    let head = tokens[0].as_str();
    if head != "git" && !head.ends_with("/git") {
        return None;
    }
    let mut idx = 1;
    let mut push_idx = None;
    while idx < tokens.len() {
        let tok = tokens[idx].as_str();
        if tok == "--" {
            if tokens.get(idx + 1).map(String::as_str) == Some("push") {
                push_idx = Some(idx + 1);
            }
            break;
        }
        if !tok.starts_with('-') {
            if tok == "push" {
                push_idx = Some(idx);
            }
            break;
        }
        let takes_value = matches!(
            tok,
            "-c" | "-C"
                | "--exec-path"
                | "--git-dir"
                | "--work-tree"
                | "--namespace"
                | "--super-prefix"
                | "--config-env"
        );
        idx += 1;
        if takes_value && !tok.contains('=') && idx < tokens.len() {
            idx += 1;
        }
    }
    let push_idx = push_idx?;

    let mut remote: Option<String> = None;
    let mut refspecs = Vec::new();
    let mut i = push_idx + 1;
    let mut after_double_dash = false;
    while i < tokens.len() {
        let tok = tokens[i].as_str();
        if !after_double_dash && tok == "--" {
            after_double_dash = true;
            i += 1;
            continue;
        }
        if !after_double_dash && tok.starts_with('-') {
            if is_destructive_push_flag(tok) {
                return None;
            }
            // Reject repo/receive-pack overrides or hooks override flags conservatively.
            if tok == "--all"
                || tok == "--tags"
                || tok == "--repo"
                || tok.starts_with("--repo=")
                || tok.starts_with("--receive-pack")
                || tok.starts_with("--exec=")
            {
                return None;
            }
            let takes_value = matches!(
                tok,
                "-o" | "--push-option" | "--signed" | "--force-if-includes"
            );
            if takes_value
                && !tok.contains('=')
                && i + 1 < tokens.len()
                && tok != "--force-if-includes"
            {
                // Only skip value for flags that take an argument; `-o opt`.
                if matches!(tok, "-o" | "--push-option") {
                    i += 1;
                }
            }
            i += 1;
            continue;
        }
        if remote.is_none() {
            // Reject remote URLs or local filesystem paths in remote position; only
            // explicit symbolic remote names (e.g. `origin`, `upstream`) qualify.
            if tok.contains("://") || tok.contains(':') || tok.contains('/') {
                return None;
            }
            remote = Some(tok.to_string());
        } else {
            // Leading `+` forces update, leading `:` deletes remote ref.
            if tok.starts_with('+') || tok.starts_with(':') {
                return None;
            }
            // Handle `src:dst` refspec conservatively: validate destination branch.
            let target = match tok.split_once(':') {
                Some((_, dst)) if !dst.is_empty() => dst,
                Some(_) => return None,
                None => tok,
            };
            let normalized = target.strip_prefix("refs/heads/").unwrap_or(target);
            if normalized.is_empty() || normalized.contains('*') || normalized.contains("..") {
                return None;
            }
            refspecs.push(normalized.to_string());
        }
        i += 1;
    }

    Some(ParsedGitPushCommand { remote, refspecs })
}

/// Detect destructive `git push` flags that MUST remain always-human.
pub fn is_destructive_push_flag(tok: &str) -> bool {
    if tok == "--force"
        || tok.starts_with("--force-with-lease")
        || tok == "--force-if-includes"
        || tok == "--mirror"
        || tok == "--delete"
        || tok == "--prune"
    {
        return true;
    }
    if tok.starts_with('-') && !tok.starts_with("--") && tok.len() > 1 {
        return tok.chars().skip(1).any(|c| c == 'f' || c == 'd');
    }
    false
}

/// Evaluate whether `command` at `current_cwd` is authorized by `scopes`.
pub fn match_git_push_delegation<'a>(
    scopes: &'a [GitPushDelegationScope],
    command: &str,
    current_cwd: Option<&Path>,
) -> Option<&'a GitPushDelegationScope> {
    let cwd = current_cwd?;
    let parsed = parse_safe_git_push(command)?;
    let cmd_remote = parsed.remote.as_deref()?;
    if parsed.refspecs.is_empty() {
        return None;
    }

    scopes.iter().find(|scope| {
        cwd_is_within_namespace_root(cwd, &scope.namespace_root)
            && !scope.branches.is_empty()
            && scope_remote_matches(scope.remote.as_deref(), cmd_remote)
            && parsed.refspecs.iter().all(|branch| {
                scope
                    .branches
                    .iter()
                    .any(|pat| branch_pattern_matches(pat, branch))
            })
    })
}

fn cwd_is_within_namespace_root(cwd: &Path, root: &Path) -> bool {
    cwd == root || cwd.starts_with(root)
}

fn scope_remote_matches(scope_remote: Option<&str>, cmd_remote: &str) -> bool {
    match scope_remote {
        Some(expected) => expected == cmd_remote,
        None => cmd_remote == "origin",
    }
}

fn branch_pattern_matches(pattern: &str, branch: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        !prefix.is_empty() && branch.starts_with(prefix)
    } else {
        pattern == branch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(branches: &[&str]) -> GitPushDelegationScope {
        GitPushDelegationScope {
            grant_id: "grant-1".to_string(),
            namespace_root: PathBuf::from("/mnt/project"),
            remote: Some("origin".to_string()),
            branches: branches.iter().map(|b| (*b).to_string()).collect(),
        }
    }

    #[test]
    fn matches_ordinary_git_push_within_delegated_scope() {
        let scopes = [scope(&["feat/*", "main"])];
        let matched = match_git_push_delegation(
            &scopes,
            "git push origin feat/m3-trust",
            Some(Path::new("/mnt/project/crates")),
        );
        assert_eq!(matched.map(|s| s.grant_id.as_str()), Some("grant-1"));

        let upstream = match_git_push_delegation(
            &scopes,
            "git push -u origin feat/m3-trust",
            Some(Path::new("/mnt/project")),
        );
        assert_eq!(upstream.map(|s| s.grant_id.as_str()), Some("grant-1"));
    }

    #[test]
    fn rejects_destructive_force_delete_mirror_and_refspec_abuse() {
        let scopes = [scope(&["feat/*", "main"])];
        let cwd = Some(Path::new("/mnt/project"));
        for cmd in [
            "git push --force origin feat/m3-trust",
            "git push -f origin feat/m3-trust",
            "git push -uf origin feat/m3-trust",
            "git push --force-with-lease origin feat/m3-trust",
            "git push --force-with-lease=refs/heads/feat/m3-trust origin feat/m3-trust",
            "git push origin +feat/m3-trust",
            "git push origin :feat/m3-trust",
            "git push --delete origin feat/m3-trust",
            "git push -d origin feat/m3-trust",
            "git push --mirror origin",
            "git push --prune origin feat/m3-trust",
            "git push upstream feat/m3-trust",
            "git push origin release/v1",
            "git push origin",
            "git push https://evil.example/repo.git feat/m3-trust",
            "git push origin feat/m3-trust && rm -rf /",
        ] {
            assert!(
                match_git_push_delegation(&scopes, cmd, cwd).is_none(),
                "must NOT match standing delegation for `{cmd}`"
            );
        }
    }
}
