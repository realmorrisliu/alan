//! Rust terminal UI for alan.

pub mod completion;
pub mod composer;
mod file_backed;
pub mod form;
pub mod history;
mod reconcile;
pub mod terminal;
mod transcript_ui;

use crate::completion::CompletionCandidate;
pub use file_backed::{FileBackedRunConfig, run as run_file_backed};

/// Maximum number of composer history entries kept in memory.
const HISTORY_LIMIT: usize = 1000;
/// Maximum number of explicitly authorized Host files indexed for `@` completion.
const FILE_INDEX_LIMIT: usize = 5000;

/// Directory names skipped when indexing authorized Host files for `@` completion.
const SKIP_DIRS: [&str; 5] = [".git", "target", "node_modules", ".alan", "dist"];

/// Build a bounded list of Host-Mount-relative file paths for `@` completion.
fn build_file_index(root: &std::path::Path, limit: usize) -> Vec<CompletionCandidate> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if files.len() >= limit {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if name.starts_with('.') || SKIP_DIRS.contains(&name.as_ref()) {
                    continue;
                }
                stack.push(path);
            } else if file_type.is_file()
                && let Ok(relative) = path.strip_prefix(root)
            {
                files.push(CompletionCandidate::new(
                    relative.to_string_lossy().to_string(),
                    None,
                ));
                if files.len() >= limit {
                    break;
                }
            }
        }
    }
    files.sort_by(|a, b| a.value.cmp(&b.value));
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_file_index_lists_files_and_skips_hidden_dirs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "x").unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git").join("config"), "x").unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src").join("lib.rs"), "x").unwrap();

        let index = build_file_index(dir.path(), 100);
        let values: Vec<_> = index.iter().map(|c| c.value.as_str()).collect();
        assert!(values.contains(&"main.rs"));
        assert!(values.contains(&"src/lib.rs"));
        assert!(!values.iter().any(|value| value.contains(".git")));
    }
}
