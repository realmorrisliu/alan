use std::cmp::Reverse;
use std::path::{Component, Path, PathBuf};

use super::{NativeToolExecutionAdapter, longest_namespace_mount};

// ponytail: project known roots at text boundaries; this is presentation, never path authority.
pub(super) fn project_text(adapter: &NativeToolExecutionAdapter, text: &str) -> String {
    if longest_namespace_mount(&adapter.mounts, &adapter.namespace_cwd).is_none() {
        return text.to_string();
    }
    let cwd = adapter.cwd.to_string_lossy();
    let cwd = cwd.trim_end_matches(std::path::MAIN_SEPARATOR);
    let mut projected = if cwd.is_empty() {
        text.to_string()
    } else {
        replace_path_prefixes(text, cwd, ".")
    };

    let mut mounts = adapter.mounts.iter().rev().collect::<Vec<_>>();
    mounts.sort_by_key(|mount| Reverse(mount.host_path.components().count()));
    for mount in mounts {
        let common = adapter
            .namespace_cwd
            .components()
            .zip(mount.namespace_path.components())
            .take_while(|(left, right)| left == right)
            .count();
        let mut mount_from_cwd = PathBuf::new();
        for _ in common..adapter.namespace_cwd.components().count() {
            mount_from_cwd.push("..");
        }
        for component in mount.namespace_path.components().skip(common) {
            if let Component::Normal(part) = component {
                mount_from_cwd.push(part);
            }
        }
        if mount_from_cwd.as_os_str().is_empty() {
            mount_from_cwd.push(".");
        }
        if mount.host_path == Path::new("/") {
            let replacement = mount_from_cwd.to_string_lossy();
            let replacement = if replacement == "." {
                "./".to_string()
            } else {
                format!("{replacement}/")
            };
            projected = replace_rooted_path_starts(&projected, &replacement);
        } else {
            let host_path = mount.host_path.to_string_lossy();
            let replacement = mount_from_cwd.to_string_lossy();
            projected = replace_path_prefixes(&projected, host_path.as_ref(), replacement.as_ref());
            let shell_escaped_host_path = host_path.replace(' ', "\\ ");
            if shell_escaped_host_path != host_path {
                projected = replace_path_prefixes(
                    &projected,
                    &shell_escaped_host_path,
                    replacement.as_ref(),
                );
            }
            if let Ok(file_url) = url::Url::from_file_path(&mount.host_path) {
                let uri_path = file_url.path();
                if uri_path != host_path.as_ref() {
                    projected = replace_path_prefixes(&projected, uri_path, replacement.as_ref());
                }
            }
        }
    }
    projected
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
        ch.is_whitespace()
            || ch == std::path::MAIN_SEPARATOR
            || matches!(
                ch,
                ':' | ',' | ';' | ')' | ']' | '}' | '\'' | '"' | '>' | '`' | '*' | '?' | '#'
            )
    }) || is_terminal_sentence_punctuation(suffix, 0)
}

fn is_terminal_sentence_punctuation(text: &str, start: usize) -> bool {
    let mut suffix = text[start..].chars();
    matches!(suffix.next(), Some('.' | '!' | '?'))
        && suffix.next().is_none_or(|ch| {
            ch.is_whitespace() || matches!(ch, ',' | ':' | ';' | ')' | ']' | '}' | '\'' | '"')
        })
}

fn replace_rooted_path_starts(text: &str, replacement: &str) -> String {
    let mut projected = String::with_capacity(text.len());
    let mut copied_through = 0;
    for (start, _) in text.match_indices('/') {
        let after = text[start + 1..].chars().next();
        let uri_authority_delimiter =
            text[..start].ends_with(':') && text[start..].starts_with("//");
        if is_path_start(text, start)
            && !uri_authority_delimiter
            && after.is_some_and(|ch| !ch.is_whitespace() && ch != '/')
        {
            projected.push_str(&text[copied_through..start]);
            projected.push_str(replacement);
            copied_through = start + 1;
        }
    }
    projected.push_str(&text[copied_through..]);
    projected
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
