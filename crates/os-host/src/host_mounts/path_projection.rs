use std::cmp::Reverse;
use std::path::{Component, Path, PathBuf};

use super::{NativeToolExecutionAdapter, longest_namespace_mount};

// ponytail: project known roots at text boundaries; this is presentation, never path authority.
pub(super) fn project_text(adapter: &NativeToolExecutionAdapter, text: &str) -> String {
    let Some(active_mount) = longest_namespace_mount(&adapter.mounts, &adapter.namespace_cwd)
    else {
        return text.to_string();
    };
    let cwd = adapter.cwd.to_string_lossy();
    let cwd = cwd.trim_end_matches(std::path::MAIN_SEPARATOR);
    let cwd = if cwd.is_empty() {
        std::path::MAIN_SEPARATOR_STR
    } else {
        cwd
    };
    let mut candidates = vec![(cwd.to_owned(), ".".to_owned())];

    // The filesystem root is public; projecting every leading slash rewrites root-relative URLs.
    if active_mount.host_path != Path::new(std::path::MAIN_SEPARATOR_STR) {
        let mount_from_cwd = relative_path(&adapter.namespace_cwd, &active_mount.namespace_path);
        let host_path = active_mount.host_path.to_string_lossy();
        let replacement = mount_from_cwd.to_string_lossy().into_owned();
        let host_path = host_path.into_owned();
        candidates.push((host_path.clone(), replacement.clone()));
        let shell_escaped_host_path = host_path.replace(' ', "\\ ");
        if shell_escaped_host_path != host_path {
            candidates.push((shell_escaped_host_path, replacement.clone()));
        }
        if let Ok(file_url) = url::Url::from_file_path(&active_mount.host_path)
            && file_url.path() != host_path
        {
            candidates.push((file_url.path().to_owned(), replacement));
        }
    }

    // A shorter root can prefix a sibling whose next character is a space. Resolve longest first.
    candidates.sort_by_key(|(path, _)| Reverse(path.len()));
    let mut projected = text.to_owned();
    for (path, replacement) in candidates {
        projected = replace_path_prefixes(&projected, &path, &replacement);
    }
    if active_mount.host_path == Path::new(std::path::MAIN_SEPARATOR_STR) {
        let physical_cwd =
            dunce::canonicalize(&adapter.cwd).unwrap_or_else(|_| adapter.cwd.clone());
        projected = project_rooted_path_tokens(&projected, &physical_cwd);
    }
    projected
}

// ponytail: handle whitespace-delimited absolute path tokens; parse more formats if needed.
fn project_rooted_path_tokens(text: &str, cwd: &Path) -> String {
    let mut projected = String::with_capacity(text.len());
    for part in text.split_inclusive(char::is_whitespace) {
        let token_end = part.find(char::is_whitespace).unwrap_or(part.len());
        let (token, separator) = part.split_at(token_end);
        let path_start = token
            .char_indices()
            .take_while(|(_, ch)| matches!(ch, '(' | '\'' | '"' | '`'))
            .map(|(index, ch)| index + ch.len_utf8())
            .last()
            .unwrap_or(0);
        let path_and_suffix = &token[path_start..];
        let path_end = path_and_suffix
            .trim_end_matches(|ch| {
                matches!(
                    ch,
                    ',' | ';' | ':' | ')' | ']' | '}' | '\'' | '"' | '>' | '`' | '.' | '!' | '?'
                )
            })
            .len();
        let path_text = &path_and_suffix[..path_end];
        let path = Path::new(path_text);
        if path_text.starts_with(std::path::MAIN_SEPARATOR) && path.is_absolute() {
            let mut relative = relative_path(cwd, path);
            if matches!(relative.components().next(), Some(Component::Normal(_))) {
                relative = Path::new(".").join(relative);
            }
            projected.push_str(&token[..path_start]);
            projected.push_str(&relative.to_string_lossy());
            projected.push_str(&path_and_suffix[path_end..]);
        } else {
            projected.push_str(token);
        }
        projected.push_str(separator);
    }
    projected
}

fn relative_path(from: &Path, to: &Path) -> PathBuf {
    let from_components = from.components().collect::<Vec<_>>();
    let to_components = to.components().collect::<Vec<_>>();
    let common = from_components
        .iter()
        .zip(&to_components)
        .take_while(|(left, right)| left == right)
        .count();
    let mut relative = PathBuf::new();
    for component in &from_components[common..] {
        if matches!(component, Component::Normal(_) | Component::ParentDir) {
            relative.push("..");
        }
    }
    for component in &to_components[common..] {
        if let Component::Normal(part) = component {
            relative.push(part);
        }
    }
    if relative.as_os_str().is_empty() {
        relative.push(".");
    }
    relative
}

fn is_underscore_emphasis_path(text: &str, start: usize, end: usize) -> bool {
    let prefix = strip_trailing_terminal_sequences(&text[..start]);
    let opening_length = prefix.chars().rev().take_while(|ch| *ch == '_').count();
    if opening_length == 0 {
        return false;
    }
    let before_opening = &prefix[..prefix.len() - opening_length];
    if !is_path_start(before_opening, before_opening.len()) {
        return false;
    }

    let suffix = strip_leading_terminal_sequences(&text[end..]);
    let closing_length = suffix.chars().take_while(|ch| *ch == '_').count();
    let after_closing = strip_leading_terminal_sequences(&suffix[closing_length..]);
    closing_length == opening_length && is_path_end(after_closing)
}

fn replace_path_prefixes(text: &str, prefix: &str, replacement: &str) -> String {
    let mut projected = String::with_capacity(text.len());
    let mut copied_through = 0;
    for (start, _) in text.match_indices(prefix) {
        let end = start + prefix.len();
        let suffix = strip_leading_terminal_sequences(&text[end..]);
        let emphasized = is_underscore_emphasis_path(text, start, end);
        let boundary_before = is_path_start(text, start) || emphasized;
        let boundary_after = is_path_end(suffix) || emphasized;
        if boundary_before && boundary_after {
            projected.push_str(&text[copied_through..start]);
            projected.push_str(replacement);
            copied_through = end;
        }
    }
    projected.push_str(&text[copied_through..]);
    projected
}

fn is_path_end(suffix: &str) -> bool {
    let after = suffix.chars().next();
    after.is_none_or(|ch| {
        ch.is_whitespace() && (ch != ' ' || !space_continues_path(suffix))
            || ch == std::path::MAIN_SEPARATOR
            || matches!(
                ch,
                ':' | ',' | ';' | ')' | ']' | '}' | '\'' | '"' | '>' | '`' | '*' | '?' | '#'
            )
    }) || is_terminal_sentence_punctuation(suffix, 0)
}

fn space_continues_path(suffix: &str) -> bool {
    let Some(after_space) = suffix.strip_prefix(' ') else {
        return false;
    };
    // ponytail: recognize one unquoted component; parse broader text-path syntax when required.
    after_space
        .trim_start_matches(' ')
        .split_whitespace()
        .next()
        .is_some_and(|component| component.contains(std::path::MAIN_SEPARATOR))
}

fn is_terminal_sentence_punctuation(text: &str, start: usize) -> bool {
    let mut suffix = text[start..].chars();
    matches!(suffix.next(), Some('.' | '!' | '?'))
        && suffix.next().is_none_or(|ch| {
            ch.is_whitespace() || matches!(ch, ',' | ':' | ';' | ')' | ']' | '}' | '\'' | '"')
        })
}

fn is_path_start(text: &str, start: usize) -> bool {
    let prefix = strip_trailing_terminal_sequences(&text[..start]);
    let before = prefix.chars().next_back();
    let file_uri_delimiter = prefix
        .get(prefix.len().saturating_sub("file://".len())..)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("file://"));
    file_uri_delimiter
        || before.is_none_or(|ch| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    '=' | ':' | '\'' | '"' | '(' | '[' | '{' | ',' | '<' | '`' | '*'
                )
        })
}

fn strip_trailing_terminal_sequences(mut prefix: &str) -> &str {
    while let Some(start) = [prefix.rfind("\x1b["), prefix.rfind("\x1b]")]
        .into_iter()
        .flatten()
        .filter(|start| terminal_sequence_end(&prefix[*start..]) == Some(prefix.len() - start))
        .max()
    {
        prefix = &prefix[..start];
    }
    prefix
}

fn strip_leading_terminal_sequences(mut suffix: &str) -> &str {
    while let Some(end) = terminal_sequence_end(suffix)
        .or_else(|| suffix.strip_prefix("\x1b\\").map(|_| 2))
        .or_else(|| suffix.strip_prefix('\x07').map(|_| 1))
    {
        suffix = &suffix[end..];
    }
    suffix
}

fn terminal_sequence_end(text: &str) -> Option<usize> {
    if let Some(body) = text.strip_prefix("\x1b[") {
        let final_byte = body
            .bytes()
            .position(|byte| (0x40..=0x7e).contains(&byte))?;
        return body.as_bytes()[..final_byte]
            .iter()
            .all(|byte| (0x20..=0x3f).contains(byte))
            .then_some(final_byte + 3);
    }
    let body = text.strip_prefix("\x1b]")?;
    body.find('\x07')
        .map(|index| index + 3)
        .into_iter()
        .chain(body.find("\x1b\\").map(|index| index + 4))
        .min()
}
