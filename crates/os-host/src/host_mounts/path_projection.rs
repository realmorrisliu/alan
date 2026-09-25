use std::cmp::Reverse;
use std::path::{Component, Path, PathBuf};

use super::{
    NativeToolExecutionAdapter, longest_namespace_mount, replace_path_prefixes,
    replace_rooted_path_starts,
};

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

pub(super) fn is_underscore_emphasis_path(text: &str, start: usize, end: usize) -> bool {
    let prefix = super::strip_trailing_terminal_sequences(&text[..start]);
    let opening_length = prefix.chars().rev().take_while(|ch| *ch == '_').count();
    if opening_length == 0 {
        return false;
    }
    let before_opening = &prefix[..prefix.len() - opening_length];
    if !super::is_path_start(before_opening, before_opening.len()) {
        return false;
    }

    let suffix = super::strip_leading_terminal_sequences(&text[end..]);
    let closing_length = suffix.chars().take_while(|ch| *ch == '_').count();
    let after_closing = super::strip_leading_terminal_sequences(&suffix[closing_length..]);
    closing_length == opening_length && super::is_path_end(after_closing)
}
