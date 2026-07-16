use anyhow::{Result, anyhow};
use std::path::{Component, Path};

pub(crate) const PROTECTED_SUBPATHS: [&str; 3] = [".git", ".alan", ".agents"];

pub(crate) fn protected_path_component(path: &Path) -> Option<&'static str> {
    path.components().find_map(protected_component)
}

fn protected_component(component: Component<'_>) -> Option<&'static str> {
    let Component::Normal(name) = component else {
        return None;
    };
    let candidate = name.to_str()?;
    PROTECTED_SUBPATHS
        .iter()
        .copied()
        .find(|protected| *protected == candidate)
}

pub(super) fn is_path_guard_reason(reason: &str) -> bool {
    reason.contains("outside host_mount")
}

#[cfg(unix)]
pub(super) fn existing_regular_file_has_multiple_links(path: &Path) -> Result<bool> {
    use std::io::ErrorKind;
    use std::os::unix::fs::MetadataExt;

    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(anyhow!(
                "Failed to inspect path link count for {}: {}",
                path.display(),
                error
            ));
        }
    };

    Ok(metadata.is_file() && metadata.nlink() > 1)
}

#[cfg(not(unix))]
pub(super) fn existing_regular_file_has_multiple_links(_path: &Path) -> Result<bool> {
    Ok(false)
}
