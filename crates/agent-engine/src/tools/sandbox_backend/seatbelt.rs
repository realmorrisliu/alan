//! macOS Seatbelt profile generation and path projection rules.

use std::path::{Component, Path, PathBuf};

/// Generate a macOS Seatbelt (SBPL) profile that confines filesystem writes to
/// writable roots (plus the temp dir) and denies outbound network.
///
/// Uses an allow-by-default base then denies the two effects we care about
/// (network and out-of-host_mount writes) and re-allows writes to the host_mount
/// and temp locations. This keeps process exec, dynamic linking, and reads
/// working while still blocking network and writes that escape writable roots —
/// which is what the auto-approve boundary relies on.
pub fn seatbelt_profile(
    writable_roots: &[PathBuf],
    read_denylist: &[PathBuf],
    allow_network: bool,
) -> String {
    // sandbox-exec evaluates real (symlink-resolved) paths, so the subpath
    // rules must use canonical paths (e.g. /var -> /private/var on macOS).
    let tmpdir = std::env::var("TMPDIR").ok();
    let mut writable = writable_roots
        .iter()
        .map(|root| sbpl_quote(&canonical_string(root)))
        .collect::<Vec<_>>();
    writable.extend([
        sbpl_quote("/tmp"),
        sbpl_quote("/private/tmp"),
        sbpl_quote("/private/var/folders"),
    ]);
    if let Some(tmpdir) = tmpdir.as_deref().filter(|value| !value.is_empty()) {
        writable.push(sbpl_quote(
            canonical_string(Path::new(tmpdir.trim_end_matches('/'))).as_str(),
        ));
    }
    let write_allows = writable
        .iter()
        .map(|path| format!("(allow file-write* (subpath {path}))"))
        .collect::<Vec<_>>()
        .join("\n");
    let read_denylist = read_denylist_excluding_writable_roots(read_denylist, writable_roots);
    let read_denies = read_denylist
        .iter()
        .map(|path| {
            let path = sbpl_quote(&canonical_string(path));
            format!("(deny file-read* (literal {path}) (subpath {path}))\n")
        })
        .collect::<String>();
    // NOTE: we do NOT kernel-deny the protected subpaths (`.git`/`.alan`/
    // `.agents`). The kernel cannot distinguish a tool's tampering from the
    // legitimate program-internal writes those dirs are designed for — denying
    // `.git` breaks `git` itself (init/add/commit all write `.git`), and denying
    // `.alan` breaks the agent's channel-scoped Memory Store. Protected-subpath
    // tampering is instead blocked by the path-guard parser (direct +
    // shell-wrapper-nested path writes), which leaves program-internal writes
    // (git porcelain, memory) intact.
    // The OS sandbox's role here is the host_mount + network boundary.
    let network_rule = if allow_network {
        ""
    } else {
        "(deny network*)\n"
    };
    format!(
        "(version 1)\n\
         (allow default)\n\
         {network_rule}\
         {read_denies}\
         (deny file-write*)\n\
         {write_allows}\n\
         (allow file-write-data (literal \"/dev/null\") (literal \"/dev/stdout\") (literal \"/dev/stderr\") (literal \"/dev/tty\") (literal \"/dev/dtracehelper\"))\n"
    )
}

pub(crate) fn read_denylist_excluding_writable_roots(
    read_denylist: &[PathBuf],
    writable_roots: &[PathBuf],
) -> Vec<PathBuf> {
    read_denylist
        .iter()
        .filter(|deny_path| !read_deny_matches_any_writable_root(deny_path, writable_roots))
        .cloned()
        .collect()
}

fn read_deny_matches_any_writable_root(deny_path: &Path, writable_roots: &[PathBuf]) -> bool {
    let deny_variants = comparable_path_variants(deny_path);
    writable_roots.iter().any(|writable_root| {
        let writable_variants = comparable_path_variants(writable_root);
        deny_variants
            .iter()
            .any(|deny| writable_variants.iter().any(|writable| writable == deny))
    })
}

/// Quote a path for inclusion in an SBPL literal/subpath form.
fn sbpl_quote(path: &str) -> String {
    format!("\"{}\"", path.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Resolve a path to its canonical (symlink-free) string, falling back to the
/// lexical path when it cannot be canonicalized (e.g. it does not exist yet).
fn canonical_string(path: &Path) -> String {
    std::fs::canonicalize(path)
        .map(|resolved| resolved.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

fn comparable_path_variants(path: &Path) -> Vec<PathBuf> {
    let mut variants = vec![lexically_normalize_path(path)];
    if let Ok(canonical) = std::fs::canonicalize(path)
        && !variants.contains(&canonical)
    {
        variants.push(canonical);
    }
    variants
}

fn lexically_normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

#[cfg(test)]
#[path = "seatbelt_tests.rs"]
mod tests;
