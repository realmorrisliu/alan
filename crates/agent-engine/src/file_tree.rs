use std::{collections::BTreeMap, path::Component, sync::Arc};

use anyhow::{Result, ensure};

const MAX_DESCRIPTOR_FILES: usize = 4_096;
const MAX_DESCRIPTOR_BYTES: usize = 16 * 1024 * 1024;

/// Immutable, bounded file content carried by a Process descriptor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessFileTree {
    files: Arc<BTreeMap<String, Arc<[u8]>>>,
}

impl ProcessFileTree {
    pub fn new(files: BTreeMap<String, Vec<u8>>) -> Result<Self> {
        ensure!(
            files.len() <= MAX_DESCRIPTOR_FILES,
            "descriptor file count exceeds {MAX_DESCRIPTOR_FILES}"
        );
        let total_bytes = files.values().try_fold(0usize, |total, bytes| {
            total.checked_add(bytes.len()).ok_or_else(|| {
                anyhow::anyhow!("descriptor byte count exceeds {MAX_DESCRIPTOR_BYTES}")
            })
        })?;
        ensure!(
            total_bytes <= MAX_DESCRIPTOR_BYTES,
            "descriptor byte count exceeds {MAX_DESCRIPTOR_BYTES}"
        );
        let mut validated = BTreeMap::new();
        for (path, bytes) in files {
            let parsed = std::path::Path::new(&path);
            ensure!(
                !path.is_empty()
                    && !parsed.is_absolute()
                    && parsed
                        .components()
                        .all(|component| matches!(component, Component::Normal(_))),
                "descriptor file path is not canonical"
            );
            let normalized = parsed
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            ensure!(normalized == path, "descriptor file path is not canonical");
            validated.insert(path, Arc::<[u8]>::from(bytes));
        }
        Ok(Self {
            files: Arc::new(validated),
        })
    }

    pub fn bytes(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(AsRef::as_ref)
    }

    pub fn text(&self, path: &str) -> Result<Option<&str>> {
        self.bytes(path)
            .map(|bytes| std::str::from_utf8(bytes).map_err(anyhow::Error::from))
            .transpose()
    }

    pub fn contains_file(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    pub fn contains_dir(&self, path: &str) -> bool {
        let prefix = format!("{}/", path.trim_end_matches('/'));
        self.files.keys().any(|file| file.starts_with(&prefix))
    }

    pub fn child_dirs(&self, path: &str) -> Vec<String> {
        let prefix = if path.is_empty() {
            String::new()
        } else {
            format!("{}/", path.trim_end_matches('/'))
        };
        let mut children = self
            .files
            .keys()
            .filter_map(|file| file.strip_prefix(&prefix))
            .filter_map(|suffix| suffix.split_once('/').map(|(child, _)| child.to_string()))
            .collect::<Vec<_>>();
        children.sort();
        children.dedup();
        children
    }

    pub fn subtree(&self, path: &str) -> Result<Self> {
        let prefix = format!("{}/", path.trim_matches('/'));
        let files = self
            .files
            .iter()
            .filter_map(|(file, bytes)| {
                file.strip_prefix(&prefix)
                    .map(|relative| (relative.to_string(), bytes.to_vec()))
            })
            .collect();
        Self::new(files)
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_tree_is_confined_and_supports_subtrees() {
        let tree = ProcessFileTree::new(BTreeMap::from([
            ("SKILL.md".to_string(), b"root".to_vec()),
            (
                "agents/reviewer/agent.toml".to_string(),
                b"model = 'x'".to_vec(),
            ),
        ]))
        .unwrap();

        assert_eq!(tree.text("SKILL.md").unwrap(), Some("root"));
        assert_eq!(tree.child_dirs("agents"), vec!["reviewer"]);
        assert!(
            tree.subtree("agents/reviewer")
                .unwrap()
                .contains_file("agent.toml")
        );
        assert!(
            ProcessFileTree::new(BTreeMap::from([("../escape".to_string(), Vec::new(),)])).is_err()
        );
    }
}
