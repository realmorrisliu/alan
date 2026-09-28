use std::cmp::Reverse;
use std::path::{Component, Path, PathBuf};

use super::{NativeToolExecutionAdapter, NativeToolMount, longest_namespace_mount};

// ponytail: project known roots at text boundaries; this is presentation, never path authority.
pub(super) fn project_text(adapter: &NativeToolExecutionAdapter, text: &str) -> String {
    let Some(active_mount) = longest_namespace_mount(&adapter.mounts, &adapter.namespace_cwd)
    else {
        return text.to_string();
    };
    let physical_cwd = dunce::canonicalize(&adapter.cwd).unwrap_or_else(|_| adapter.cwd.clone());
    let is_filesystem_root = Path::new(std::path::MAIN_SEPARATOR_STR);
    let has_root_grant = active_mount.host_path == is_filesystem_root;
    let cwd = physical_cwd.to_string_lossy();
    let cwd = cwd.trim_end_matches(std::path::MAIN_SEPARATOR);
    let cwd = if cwd.is_empty() {
        std::path::MAIN_SEPARATOR_STR
    } else {
        cwd
    };
    let mut candidates = Vec::new();
    if !has_root_grant {
        candidates.push((cwd.to_owned(), ".".to_owned()));
        for mount in &adapter.mounts {
            if mount.host_path == is_filesystem_root {
                continue;
            }
            let mount_from_cwd = relative_path(&physical_cwd, &mount.host_path);
            let host_path = mount.host_path.to_string_lossy().into_owned();
            let replacement = mount_from_cwd.to_string_lossy().into_owned();
            candidates.push((host_path.clone(), replacement.clone()));
            let shell_escaped_host_path = host_path.replace(' ', "\\ ");
            if shell_escaped_host_path != host_path {
                candidates.push((shell_escaped_host_path, replacement.clone()));
            }
        }
    }

    // A shorter root can prefix a sibling whose next character is a space. Resolve longest first.
    candidates.sort_by_key(|(path, _)| Reverse(path.len()));
    let mut projected =
        project_rooted_path_tokens(text, &physical_cwd, has_root_grant, &adapter.mounts);
    for (path, replacement) in candidates {
        projected = replace_path_prefixes(&projected, &path, &replacement, &physical_cwd);
    }
    projected
}

// ponytail: recognize common absolute-path boundaries; extend the formatter grammar if needed.
fn project_rooted_path_tokens(
    text: &str,
    cwd: &Path,
    root_grant: bool,
    mounts: &[NativeToolMount],
) -> String {
    let mut projected = String::with_capacity(text.len());
    let mut cursor = 0;
    while cursor < text.len() {
        let first = text[cursor..].chars().next().expect("cursor is in text");
        if first.is_whitespace() {
            let end = text[cursor..]
                .find(|ch: char| !ch.is_whitespace())
                .map_or(text.len(), |offset| cursor + offset);
            projected.push_str(&text[cursor..end]);
            cursor = end;
            continue;
        }
        let token_end = text[cursor..]
            .find(char::is_whitespace)
            .map_or(text.len(), |offset| cursor + offset);
        let token = &text[cursor..token_end];
        let lowercase_token = token.to_ascii_lowercase();
        if let Some(url_start) = non_file_url_start(token) {
            let field_start = token[..url_start]
                .char_indices()
                .rev()
                .find_map(|(index, ch)| matches!(ch, ',' | ';').then_some(index));
            if root_grant && let Some(field_start) = field_start {
                projected.push_str(&project_rooted_path_tokens(
                    &token[..field_start],
                    cwd,
                    root_grant,
                    mounts,
                ));
                projected.push_str(&token[field_start..]);
            } else {
                projected.push_str(token);
            }
            cursor = token_end;
            continue;
        }
        let path_start = lowercase_token.find("file://").or_else(|| {
            token.char_indices().find_map(|(index, ch)| {
                (ch == std::path::MAIN_SEPARATOR && is_path_start(text, cursor + index))
                    .then_some(index)
            })
        });
        let Some(path_start) = path_start else {
            projected.push_str(token);
            cursor = token_end;
            continue;
        };
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
        if let Ok(file_url) = url::Url::parse(path_text)
            && file_url.scheme() == "file"
            && let Ok(path) = file_url.to_file_path()
        {
            let canonical_path = dunce::canonicalize(&path).unwrap_or_else(|_| path.clone());
            let delegated = root_grant
                || mounts.iter().any(|mount| {
                    mount.host_path != Path::new(std::path::MAIN_SEPARATOR_STR)
                        && canonical_path.starts_with(&mount.host_path)
                });
            if !delegated {
                projected.push_str(token);
                cursor = token_end;
                continue;
            }
            let path = if root_grant { &path } else { &canonical_path };
            let relative = display_relative_path(cwd, path);
            projected.push_str(&token[..path_start]);
            projected.push_str(&relative.to_string_lossy());
            if let Some(query) = file_url.query() {
                projected.push('?');
                projected.push_str(query);
            }
            if let Some(fragment) = file_url.fragment() {
                projected.push('#');
                projected.push_str(fragment);
            }
            projected.push_str(&path_and_suffix[path_end..]);
            cursor = token_end;
            continue;
        }
        let path = Path::new(path_text);
        if root_grant
            && path_text.starts_with(std::path::MAIN_SEPARATOR)
            && path.is_absolute()
            && !is_root_relative_url(text, cursor + path_start)
        {
            let rooted_start = cursor + path_start;
            let rooted_end = extend_root_path_through_space(text, rooted_start + path_end, cwd);
            let rooted_path = &text[rooted_start..rooted_end];
            let trimmed_end = rooted_path
                .trim_end_matches(|ch| {
                    matches!(
                        ch,
                        ',' | ';'
                            | ':'
                            | ')'
                            | ']'
                            | '}'
                            | '\''
                            | '"'
                            | '>'
                            | '`'
                            | '.'
                            | '!'
                            | '?'
                    )
                })
                .len();
            let path_end = rooted_start + trimmed_end;
            let relative = display_relative_path(cwd, Path::new(&text[rooted_start..path_end]));
            projected.push_str(&text[cursor..rooted_start]);
            projected.push_str(&relative.to_string_lossy());
            cursor = path_end;
        } else {
            projected.push_str(token);
            cursor = token_end;
        }
    }
    projected
}

fn non_file_url_start(token: &str) -> Option<usize> {
    let lowercase = token.to_ascii_lowercase();
    let mut search_from = 0;
    while let Some(relative_end) = lowercase[search_from..].find("://") {
        let scheme_end = search_from + relative_end;
        let scheme_start = token[..scheme_end]
            .char_indices()
            .rev()
            .find_map(|(index, ch)| {
                (!(ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.')))
                    .then_some(index + ch.len_utf8())
            })
            .unwrap_or(0);
        if !token[scheme_start..scheme_end].eq_ignore_ascii_case("file") {
            return Some(scheme_start);
        }
        search_from = scheme_end + "://".len();
    }
    None
}

fn extend_root_path_through_space(text: &str, mut path_end: usize, cwd: &Path) -> usize {
    while text[path_end..].starts_with(' ') {
        if !space_continues_path(&text[path_end..], cwd) {
            break;
        }
        let next_start = path_end
            + text[path_end..]
                .find(|ch: char| !ch.is_whitespace())
                .unwrap_or(text.len() - path_end);
        let next_end = text[next_start..]
            .find(char::is_whitespace)
            .map_or(text.len(), |offset| next_start + offset);
        path_end = next_end;
    }
    path_end
}

fn display_relative_path(cwd: &Path, path: &Path) -> PathBuf {
    let relative = relative_path(cwd, path);
    if matches!(relative.components().next(), Some(Component::Normal(_))) {
        Path::new(".").join(relative)
    } else {
        relative
    }
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

    let original_suffix = &text[end..];
    let leading_sequence_length =
        original_suffix.len() - strip_leading_terminal_sequences(original_suffix).len();
    let suffix = strip_trailing_terminal_sequences(&original_suffix[leading_sequence_length..]);
    let closing_length = suffix.chars().rev().take_while(|ch| *ch == '_').count();
    let closing_start = suffix.len() - closing_length;
    let path_extension = &suffix[..closing_start];
    let after_closing = strip_leading_terminal_sequences(
        &text[end + leading_sequence_length + closing_start + closing_length..],
    );
    closing_length == opening_length
        && (path_extension.is_empty() || path_extension.starts_with(std::path::MAIN_SEPARATOR))
        && is_path_end(after_closing)
}

fn replace_path_prefixes(text: &str, prefix: &str, replacement: &str, cwd: &Path) -> String {
    let mut projected = String::with_capacity(text.len());
    let mut copied_through = 0;
    for (start, _) in text.match_indices(prefix) {
        let end = start + prefix.len();
        let suffix = strip_leading_terminal_sequences(&text[end..]);
        let emphasized = is_underscore_emphasis_path(text, start, end);
        let boundary_before = is_path_start(text, start) || emphasized;
        let boundary_after = is_candidate_path_end(suffix, cwd) || emphasized;
        if boundary_before && boundary_after {
            let replacement_start = file_uri_scheme_start(text, start).unwrap_or(start);
            projected.push_str(&text[copied_through..replacement_start]);
            projected.push_str(replacement);
            copied_through = end;
        }
    }
    projected.push_str(&text[copied_through..]);
    projected
}

fn is_root_relative_url(text: &str, start: usize) -> bool {
    let prefix = strip_trailing_terminal_sequences(&text[..start]);
    let line_prefix = prefix.rsplit(['\n', '\r']).next().unwrap_or_default();
    if line_prefix.trim_start().starts_with('[') && line_prefix.contains("]:") {
        return true;
    }
    let lowercase = prefix.to_ascii_lowercase();
    if [
        "](", "url(", "url('", "url(\"", "href='", "href=\"", "src='", "src=\"",
    ]
    .iter()
    .any(|suffix| lowercase.ends_with(suffix))
    {
        return true;
    }
    prefix.rfind('<').is_some_and(|tag_start| {
        let tag = lowercase.get(tag_start..).unwrap_or_default();
        !tag.contains('>') && is_url_attribute_value_prefix(tag)
    })
}

fn is_url_attribute_value_prefix(tag: &str) -> bool {
    let tag = tag.trim_end_matches(['\'', '"']);
    let Some((before_value, _)) = tag.rsplit_once('=') else {
        return false;
    };
    matches!(
        before_value.split_whitespace().next_back(),
        Some(
            "action"
                | "background"
                | "cite"
                | "codebase"
                | "data"
                | "formaction"
                | "href"
                | "icon"
                | "longdesc"
                | "manifest"
                | "poster"
                | "profile"
                | "src"
                | "srcset"
                | "usemap"
                | "xlink:href"
        )
    )
}

fn file_uri_scheme_start(text: &str, start: usize) -> Option<usize> {
    let scheme_start = start.checked_sub("file://".len())?;
    text.get(scheme_start..start)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("file://"))
        .then_some(scheme_start)
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

fn is_candidate_path_end(suffix: &str, cwd: &Path) -> bool {
    is_path_end(suffix) && (!suffix.starts_with(' ') || !space_continues_path(suffix, cwd))
}

fn space_continues_path(suffix: &str, cwd: &Path) -> bool {
    let Some(after_space) = suffix.strip_prefix(' ') else {
        return false;
    };
    let after_space = after_space.trim_start_matches(' ');
    let Some(component) = after_space.split_whitespace().next() else {
        return false;
    };
    let component = component.trim_start_matches(['(', '\'', '"', '`']);
    if component.starts_with(std::path::MAIN_SEPARATOR) {
        return false;
    }
    if !component.contains(std::path::MAIN_SEPARATOR) {
        return after_space
            .strip_prefix(component)
            .is_some_and(|tail| tail.trim().is_empty());
    }
    // ponytail: favor existing cwd-relative tokens; use structured output if this ambiguity grows.
    !cwd.join(component).exists()
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
