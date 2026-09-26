use std::cmp::Reverse;
use std::path::{Path, PathBuf};

use super::{NativeToolExecutionAdapter, longest_namespace_mount};

// ponytail: project known roots at text boundaries; this is presentation, never path authority.
pub(super) fn project_text(adapter: &NativeToolExecutionAdapter, text: &str) -> String {
    project_native_text(
        adapter,
        &project_file_urls(adapter, &project_json_strings(adapter, text)),
        true,
        true,
    )
}

// Decode JSON string tokens so Unicode, surrogate pairs and optional solidus escapes
// share the native path matcher instead of enumerating encoder-specific spellings.
fn project_json_strings(adapter: &NativeToolExecutionAdapter, text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut copied = 0;
    let mut chars = text.char_indices();
    let mut markup = markup_tag_ranges(text).into_iter().peekable();
    while let Some((start, ch)) = chars.next() {
        if ch != '"' {
            continue;
        }
        while markup.peek().is_some_and(|range| range.end <= start) {
            markup.next();
        }
        let in_markup = markup.peek().is_some_and(|range| range.contains(&start));
        let mut escaped = false;
        for (end, ch) in chars.by_ref() {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                let token = &text[start..=end];
                let complete_token = is_complete_quoted_path(&text[end + 1..])
                    || text[end + 1..].trim_start().starts_with(':');
                if complete_token && let Ok(decoded) = serde_json::from_str::<String>(token) {
                    // Retain the quoted boundary when projecting URI punctuation.
                    let urls = project_file_urls(adapter, &format!("\"{decoded}\""));
                    let quoted = project_native_text(adapter, &urls, false, !in_markup);
                    let projected = &quoted[1..quoted.len() - 1];
                    if projected != decoded {
                        result.push_str(&text[copied..start]);
                        result.push_str(
                            &serde_json::to_string(&projected).expect("string serializes"),
                        );
                        copied = end + 1;
                    }
                }
                break;
            }
        }
    }
    result.push_str(&text[copied..]);
    result
}

fn ends_uri_scheme(text: &str) -> bool {
    text.strip_suffix(':').is_some_and(|prefix| {
        let scheme = prefix
            .rsplit(|ch: char| {
                ch.is_whitespace() || matches!(ch, ':' | '=' | '\'' | '"' | '(' | '[' | '<')
            })
            .next()
            .unwrap_or_default();
        scheme.starts_with(|ch: char| ch.is_ascii_alphabetic() || matches!(ch, '?' | '!'))
            && scheme
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
    })
}

fn markup_tag_ranges(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = text[cursor..].find('<') {
        let start = cursor + relative;
        cursor = start + 1;
        let tail = &text[start..];
        if !tail[1..]
            .trim_start_matches('/')
            .starts_with(|ch: char| ch.is_ascii_alphabetic() || matches!(ch, '?' | '!'))
        {
            continue;
        }
        let terminator = if tail.starts_with("<!--") {
            Some("-->")
        } else if tail.starts_with("<![CDATA[") {
            Some("]]>")
        } else if tail.starts_with("<?") {
            Some("?>")
        } else {
            None
        };
        let mut end = text.len();
        if let Some(terminator) = terminator {
            end = tail
                .find(terminator)
                .map_or(text.len(), |offset| start + offset + terminator.len());
        } else {
            let declaration = tail.starts_with("<!");
            let mut brackets = 0usize;
            let mut quote = None;
            let mut nested_end = 0;
            for (offset, ch) in tail[1..].char_indices() {
                let index = start + 1 + offset;
                if index < nested_end {
                    continue;
                }
                if let Some(delimiter) = quote {
                    if ch == delimiter {
                        quote = None;
                    }
                    continue;
                }
                if declaration {
                    let tail = &text[index..];
                    let terminator = if tail.starts_with("<!--") {
                        Some("-->")
                    } else if tail.starts_with("<?") {
                        Some("?>")
                    } else {
                        None
                    };
                    if let Some(terminator) = terminator {
                        nested_end = tail
                            .find(terminator)
                            .map_or(text.len(), |offset| index + offset + terminator.len());
                        continue;
                    }
                }
                match ch {
                    '\'' | '"' => quote = Some(ch),
                    '[' if declaration => brackets += 1,
                    ']' if declaration => brackets = brackets.saturating_sub(1),
                    '>' if brackets == 0 => {
                        end = index + 1;
                        break;
                    }
                    _ => {}
                }
            }
        }
        ranges.push(start..end);
        cursor = end;
    }
    ranges
}

fn project_file_urls(adapter: &NativeToolExecutionAdapter, text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut copied = 0;
    for (start, _) in text
        .as_bytes()
        .windows(5)
        .enumerate()
        .filter(|(_, bytes)| bytes.eq_ignore_ascii_case(b"file:"))
    {
        if start < copied
            || ends_uri_scheme(&text[..start])
            || !text[start + 5..].starts_with('/')
            || text[..start]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '-' | '/' | '+' | '.'))
        {
            continue;
        }
        let mut brackets = Vec::new();
        let mut end = text[start..]
            .char_indices()
            .find_map(|(offset, ch)| {
                let closing = match ch {
                    '(' => Some(')'),
                    '[' => Some(']'),
                    '{' => Some('}'),
                    _ => None,
                };
                if let Some(closing) = closing {
                    brackets.push(closing);
                    return None;
                }
                if brackets.last() == Some(&ch) {
                    brackets.pop();
                    return None;
                }
                (is_field_separator(ch)
                    || matches!(
                        ch,
                        '\x1b' | '\x07' | '\'' | '"' | '<' | '>' | '`' | ')' | ']' | '}'
                    ))
                .then_some(start + offset)
            })
            .unwrap_or(text.len());
        // ponytail: unwrapped URLs use prose punctuation; quote/encode literal endings.
        if !text[start..end].contains(['?', '#'])
            && text[end..].chars().next().is_none_or(is_field_separator)
        {
            end = start
                + text[start..end]
                    .trim_end_matches([',', ';', '.', '!'])
                    .len();
        }
        let Ok(url) = url::Url::parse(&text[start..end]) else {
            continue;
        };
        let Ok(path) = url.to_file_path() else {
            continue;
        };
        let Some(relative) = relative_native_path(adapter, &path) else {
            continue;
        };
        result.push_str(&text[copied..start]);
        // Encode each URI path component; preserve relative ../ and / separators.
        let relative_uri = relative
            .components()
            .map(|component| {
                url::form_urlencoded::byte_serialize(component.as_os_str().as_encoded_bytes())
                    .collect::<String>()
                    .replace('+', "%20")
            })
            .collect::<Vec<_>>()
            .join("/");
        result.push_str(&relative_uri);
        if url.path().ends_with('/') {
            result.push('/');
        }
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

fn relative_native_path(adapter: &NativeToolExecutionAdapter, path: &Path) -> Option<PathBuf> {
    let active = longest_namespace_mount(&adapter.mounts, &adapter.namespace_cwd)?;
    let (cwd, namespace_cwd) = physical_cwd(adapter);
    if let Ok(suffix) = path
        .strip_prefix(&adapter.cwd)
        .or_else(|_| path.strip_prefix(&cwd))
    {
        return Some(Path::new(".").join(suffix));
    }
    if path.starts_with(&active.host_path) {
        return Some(relative_path(&cwd, path));
    }
    let Some(mount) = adapter
        .mounts
        .iter()
        .filter(|mount| path.starts_with(&mount.host_path))
        .max_by_key(|mount| mount.host_path.components().count())
    else {
        return (!cwd.starts_with(&active.host_path) && path.is_absolute())
            .then(|| relative_path(&cwd, path));
    };
    let suffix = path.strip_prefix(&mount.host_path).ok()?;
    Some(relative_path(
        &namespace_cwd,
        &mount.namespace_path.join(suffix),
    ))
}

fn physical_cwd(adapter: &NativeToolExecutionAdapter) -> (PathBuf, PathBuf) {
    adapter.projection_cwd.clone()
}

fn relative_path(cwd: &Path, target: &Path) -> PathBuf {
    let common = cwd
        .components()
        .zip(target.components())
        .take_while(|(a, b)| a == b)
        .count();
    let mut relative = PathBuf::new();
    for _ in common..cwd.components().count() {
        relative.push("..");
    }
    for component in target.components().skip(common) {
        relative.push(component.as_os_str());
    }
    if relative.as_os_str().is_empty() {
        relative.push(".");
    }
    relative
}

fn is_field_separator(ch: char) -> bool {
    ch.is_whitespace() || ch == '\0'
}

fn shell_escaped_path(path: &str) -> String {
    let mut escaped = String::new();
    for ch in path.chars() {
        if ch.is_whitespace()
            || matches!(
                ch,
                '!' | '"'
                    | '$'
                    | '&'
                    | '\''
                    | '('
                    | ')'
                    | '*'
                    | ','
                    | ';'
                    | '<'
                    | '>'
                    | '?'
                    | '['
                    | '\\'
                    | ']'
                    | '^'
                    | '`'
                    | '{'
                    | '|'
                    | '}'
            )
        {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

fn ansi_escaped_path(native: &Path, escape_non_ascii: bool) -> String {
    let mut quoted = String::new();
    for chunk in native.as_os_str().as_encoded_bytes().utf8_chunks() {
        for ch in chunk.valid().chars() {
            match ch {
                '\x07' => quoted.push_str("\\a"),
                '\x08' => quoted.push_str("\\b"),
                '\x1b' => quoted.push_str("\\E"),
                '\x0c' => quoted.push_str("\\f"),
                '\n' => quoted.push_str("\\n"),
                '\r' => quoted.push_str("\\r"),
                '\t' => quoted.push_str("\\t"),
                '\x0b' => quoted.push_str("\\v"),
                '\\' => quoted.push_str("\\\\"),
                '\'' => quoted.push_str("\\'"),
                ch if ch.is_control() || (escape_non_ascii && !ch.is_ascii()) => {
                    for byte in ch.encode_utf8(&mut [0; 4]).as_bytes() {
                        quoted.push_str(&format!("\\{byte:03o}"));
                    }
                }
                ch => quoted.push(ch),
            }
        }
        for byte in chunk.invalid() {
            quoted.push_str(&format!("\\{byte:03o}"));
        }
    }
    quoted
}

fn replace_native_prefix(
    text: &str,
    native: &Path,
    replacement: &str,
    shell_quoting: bool,
) -> String {
    let path = native.to_string_lossy();
    let mut projected =
        replace_path_prefixes(text, &shell_escaped_path(&path), replacement, shell_quoting);
    if shell_quoting && path.contains('\'') {
        projected =
            replace_path_prefixes(&projected, &path.replace('\'', "'\\''"), replacement, true);
    }
    projected = replace_path_prefixes(&projected, &path, replacement, shell_quoting);
    for escape_non_ascii in [false, true] {
        if native.to_str().is_some()
            && !path
                .chars()
                .any(|ch| ch.is_control() || (escape_non_ascii && !ch.is_ascii()))
        {
            continue;
        }
        let quoted = ansi_escaped_path(native, escape_non_ascii);
        projected = replace_path_prefixes(&projected, &quoted, replacement, shell_quoting);
    }
    projected
}

fn project_native_text(
    adapter: &NativeToolExecutionAdapter,
    text: &str,
    shell_quoting: bool,
    project_root_paths: bool,
) -> String {
    let Some(active) = longest_namespace_mount(&adapter.mounts, &adapter.namespace_cwd) else {
        return text.to_string();
    };
    let (physical, namespace_cwd) = physical_cwd(adapter);
    let mut projected = text.to_string();
    for cwd in [&adapter.cwd, &physical] {
        if cwd == Path::new("/") && !project_root_paths {
            continue;
        }
        projected = if cwd == Path::new("/") {
            replace_rooted_path_starts(&projected, "./", shell_quoting)
        } else {
            replace_native_prefix(&projected, cwd, ".", shell_quoting)
        };
    }

    let mut mounts = adapter.mounts.iter().rev().collect::<Vec<_>>();
    mounts.sort_by_key(|mount| {
        (
            mount.namespace_path != active.namespace_path,
            Reverse(mount.host_path.components().count()),
        )
    });
    for mount in mounts {
        if mount.host_path == Path::new("/") && !project_root_paths {
            continue;
        }
        let mount_from_cwd = relative_path(&namespace_cwd, &mount.namespace_path);
        if mount.host_path == Path::new("/") {
            let replacement = mount_from_cwd.to_string_lossy();
            let replacement = if replacement == "." {
                "./".to_string()
            } else {
                format!("{replacement}/")
            };
            projected = replace_rooted_path_starts(&projected, &replacement, shell_quoting);
        } else {
            let replacement = mount_from_cwd.to_string_lossy();
            projected =
                replace_native_prefix(&projected, &mount.host_path, &replacement, shell_quoting);
        }
    }
    if !physical.starts_with(&active.host_path) {
        for ancestor in physical.ancestors().skip(1) {
            if ancestor == Path::new("/") && !project_root_paths {
                continue;
            }
            let relative = relative_path(&physical, ancestor);
            let replacement = relative.to_string_lossy();
            projected = if ancestor == Path::new("/") {
                replace_rooted_path_starts(&projected, &format!("{replacement}/"), shell_quoting)
            } else {
                replace_native_prefix(&projected, ancestor, &replacement, shell_quoting)
            };
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

fn replacement_at(text: &str, start: usize, replacement: &str, shell_quoting: bool) -> String {
    if !shell_quoting {
        return replacement.to_owned();
    }
    let before = strip_trailing_terminal_sequences(&text[..start]);
    match before.chars().next_back() {
        Some('\'') if before.ends_with("$'") => ansi_escaped_path(Path::new(replacement), false),
        Some('\'') => replacement.replace('\'', "'\\''"),
        Some('"') => replacement
            .chars()
            .flat_map(|ch| {
                let escape = matches!(ch, '"' | '\\' | '$' | '`');
                escape
                    .then_some('\\')
                    .into_iter()
                    .chain(std::iter::once(ch))
            })
            .collect(),
        _ if replacement.chars().any(char::is_control) => {
            format!("$'{}'", ansi_escaped_path(Path::new(replacement), false))
        }
        _ => shell_escaped_path(replacement),
    }
}

fn replace_path_prefixes(
    text: &str,
    prefix: &str,
    replacement: &str,
    shell_quoting: bool,
) -> String {
    let mut projected = String::with_capacity(text.len());
    let mut copied_through = 0;
    for (start, _) in text.match_indices(prefix) {
        let end = start + prefix.len();
        let suffix = strip_leading_terminal_sequences(&text[end..]);
        let emphasized = is_emphasized_path(text, start, end);
        let boundary_before = is_path_start(text, start) || emphasized;
        let boundary_after = quoted_path_end(text, start, suffix)
            .unwrap_or_else(|| is_path_end(suffix) || emphasized);
        if boundary_before && boundary_after && !ends_uri_scheme(&text[..start]) {
            projected.push_str(&text[copied_through..start]);
            projected.push_str(&replacement_at(text, start, replacement, shell_quoting));
            copied_through = end;
        }
    }
    projected.push_str(&text[copied_through..]);
    projected
}

fn quoted_path_end(text: &str, start: usize, suffix: &str) -> Option<bool> {
    let quote = strip_trailing_terminal_sequences(&text[..start])
        .chars()
        .next_back()
        .filter(|quote| matches!(quote, '\'' | '"' | '`'))?;
    if let Some(rest) = suffix.strip_prefix(quote) {
        Some(is_complete_quoted_path(rest))
    } else if suffix.starts_with('/') {
        None // A descendant continues inside the quoted path.
    } else {
        Some(false) // Punctuation inside quotes is part of a sibling filename.
    }
}

fn is_complete_quoted_path(rest: &str) -> bool {
    !rest.starts_with(['\'', '"', '`', '/'])
        && (is_path_end(rest) || rest.starts_with([',', ';', ']', '}']))
}

fn is_path_end(suffix: &str) -> bool {
    let Some(first) = suffix.chars().next() else {
        return true;
    };
    if is_field_separator(first) || first == std::path::MAIN_SEPARATOR {
        return true;
    }
    // A diagnostic location or closing prose delimiter may end a path, but a
    // punctuation-prefixed sibling filename (such as project#backup) does not.
    if first == ':' {
        if suffix.trim_start_matches(':').starts_with('/') {
            return true; // A following absolute entry in a native PATH-style list.
        }
        let location = suffix[1..].trim_start_matches(|ch: char| ch.is_ascii_digit() || ch == ':');
        return location.is_empty() || location.starts_with(is_field_separator);
    }
    let rest = suffix.trim_start_matches([',', ';', ')', ']', '}', '\'', '"', '>', '`', '.', '!']);
    rest.len() < suffix.len() && (rest.is_empty() || rest.starts_with(is_field_separator))
}

fn replace_rooted_path_starts(text: &str, replacement: &str, shell_quoting: bool) -> String {
    let mut projected = String::with_capacity(text.len());
    let mut copied_through = 0;
    let mut markup = markup_tag_ranges(text).into_iter().peekable();
    for (slash, _) in text.match_indices('/') {
        while markup.peek().is_some_and(|range| range.end <= slash) {
            markup.next();
        }
        if markup.peek().is_some_and(|range| range.contains(&slash)) {
            continue;
        }
        let start = if text[..slash].ends_with('\\') {
            slash - 1
        } else {
            slash
        };
        let suffix = strip_leading_terminal_sequences(&text[slash + 1..]);
        // A closing markup tag is not a path, even for a root-backed grant.
        if text[..start].ends_with('<')
            && suffix.find('>').is_some_and(|end| {
                suffix[..end]
                    .trim_end()
                    .chars()
                    .all(|ch| ch.is_alphanumeric() || matches!(ch, ':' | '_' | '-' | '.'))
            })
        {
            continue;
        }
        let method = text[..start]
            .split_whitespace()
            .next_back()
            .unwrap_or_default()
            .trim_start_matches(['"', '\'']);
        // HTTP method = token (RFC 9110), including extension methods.
        let http_target = !method.is_empty()
            && method
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
            && suffix
                .split_whitespace()
                .nth(1)
                .is_some_and(|version| version.starts_with("HTTP/"));
        if http_target {
            continue;
        }
        let emphasized = is_emphasized_path(text, start, slash + 1);
        let bare_root = emphasized
            || quoted_path_end(text, start, suffix) == Some(true)
            || if suffix.starts_with(is_field_separator) {
                suffix
                    .split(['\n', '\r', '\0'])
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .is_empty()
            } else {
                is_path_end(suffix) && !suffix.starts_with('/')
            };
        let after = suffix.chars().next();
        let uri_scheme = ends_uri_scheme(&text[..start]);
        if (is_path_start(text, start) || emphasized)
            && !uri_scheme
            && (bare_root || after.is_some_and(|ch| !is_field_separator(ch) && ch != '/'))
        {
            projected.push_str(&text[copied_through..start]);
            let replacement = if bare_root {
                replacement.trim_end_matches('/')
            } else {
                replacement
            };
            projected.push_str(&replacement_at(text, start, replacement, shell_quoting));
            copied_through = slash + 1;
        }
    }
    projected.push_str(&text[copied_through..]);
    projected
}

fn is_path_start(text: &str, start: usize) -> bool {
    let prefix = strip_trailing_terminal_sequences(&text[..start]);
    let before = prefix.chars().next_back();
    before.is_none_or(|ch| {
        is_field_separator(ch)
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
