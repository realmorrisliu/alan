use std::cmp::Reverse;
use std::path::{Component, Path, PathBuf};

use super::{NativeToolExecutionAdapter, longest_namespace_mount};

// ponytail: project known roots at text boundaries; this is presentation, never path authority.
pub(super) fn project_text(adapter: &NativeToolExecutionAdapter, text: &str) -> String {
    project_native_text(adapter, &project_file_urls(adapter, text))
}

fn project_file_urls(adapter: &NativeToolExecutionAdapter, text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut copied = 0;
    for (start, _) in text
        .as_bytes()
        .windows(7)
        .enumerate()
        .filter(|(_, bytes)| bytes.eq_ignore_ascii_case(b"file://"))
    {
        if start < copied
            || text[..start]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '-' | '/'))
        {
            continue;
        }
        let end = text[start..]
            .char_indices()
            .find_map(|(offset, ch)| {
                (ch.is_whitespace()
                    || matches!(
                        ch,
                        '\x1b' | '\x07' | '\'' | '"' | '<' | '>' | '`' | ')' | ']' | '}'
                    ))
                .then_some(start + offset)
            })
            .unwrap_or(text.len());
        let Ok(url) = url::Url::parse(&text[start..end]) else {
            continue;
        };
        let Ok(path) = url.to_file_path() else {
            continue;
        };
        if !adapter
            .mounts
            .iter()
            .any(|mount| path.starts_with(&mount.host_path))
        {
            continue;
        }
        result.push_str(&text[copied..start]);
        result.push_str(&project_native_text(adapter, &path.to_string_lossy()));
        if let Some(query) = url.query() {
            result.push('?');
            result.push_str(query);
        }
        if let Some(fragment) = url.fragment() {
            result.push('#');
            result.push_str(fragment);
        }
        copied = end;
    }
    result.push_str(&text[copied..]);
    result
}

fn project_native_text(adapter: &NativeToolExecutionAdapter, text: &str) -> String {
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
        }
    }
    projected
}

fn is_emphasized_path(text: &str, start: usize, end: usize) -> bool {
    let prefix = strip_trailing_terminal_sequences(&text[..start]);
    let Some(marker @ ('_' | '*')) = prefix.chars().next_back() else {
        return false;
    };
    let opening_length = prefix.chars().rev().take_while(|ch| *ch == marker).count();
    if opening_length == 0 {
        return false;
    }
    let before_opening = &prefix[..prefix.len() - opening_length];
    if !is_path_start(before_opening, before_opening.len()) {
        return false;
    }

    let suffix = strip_leading_terminal_sequences(&text[end..]);
    let closing_length = suffix.chars().take_while(|ch| *ch == marker).count();
    let after_closing = strip_leading_terminal_sequences(&suffix[closing_length..]);
    closing_length == opening_length && is_path_end(after_closing)
}

fn replace_path_prefixes(text: &str, prefix: &str, replacement: &str) -> String {
    let mut projected = String::with_capacity(text.len());
    let mut copied_through = 0;
    for (start, _) in text.match_indices(prefix) {
        let end = start + prefix.len();
        let suffix = strip_leading_terminal_sequences(&text[end..]);
        let emphasized = is_emphasized_path(text, start, end);
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
    let Some(first) = suffix.chars().next() else {
        return true;
    };
    if first.is_whitespace() || first == std::path::MAIN_SEPARATOR {
        return true;
    }
    // A diagnostic location or closing prose delimiter may end a path, but a
    // punctuation-prefixed sibling filename (such as project#backup) does not.
    if first == ':' {
        let location = suffix[1..].trim_start_matches(|ch: char| ch.is_ascii_digit() || ch == ':');
        return location.is_empty() || location.starts_with(char::is_whitespace);
    }
    let rest = suffix.trim_start_matches([',', ';', ')', ']', '}', '\'', '"', '>', '`', '.', '!']);
    rest.len() < suffix.len() && (rest.is_empty() || rest.starts_with(char::is_whitespace))
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
    before.is_none_or(|ch| {
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
