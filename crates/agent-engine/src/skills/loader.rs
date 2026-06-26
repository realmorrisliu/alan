//! Skill loading from filesystem.

use crate::skills::types::*;
use serde::de::DeserializeOwned;
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use tracing::{debug, error, warn};

/// Load skill metadata from a SKILL.md file.
pub fn load_skill_metadata(path: &Path, scope: SkillScope) -> Result<SkillMetadata, SkillsError> {
    let content = std::fs::read_to_string(path)?;
    parse_skill_metadata_with_source(
        &content,
        path,
        scope,
        SkillContentSource::File(path.to_path_buf()),
        None,
    )
}

/// Parse skill metadata from content.
pub fn parse_skill_metadata(
    content: &str,
    path: &Path,
    scope: SkillScope,
) -> Result<SkillMetadata, SkillsError> {
    parse_skill_metadata_with_source(
        content,
        path,
        scope,
        SkillContentSource::File(path.to_path_buf()),
        None,
    )
}

/// Parse skill metadata from content with an explicit content source.
pub fn parse_skill_metadata_with_source(
    content: &str,
    path: &Path,
    scope: SkillScope,
    source: SkillContentSource,
    package_id: Option<CapabilityPackageId>,
) -> Result<SkillMetadata, SkillsError> {
    let (frontmatter_str, _) =
        extract_frontmatter(content).ok_or(SkillsError::MissingFrontmatter)?;

    let frontmatter: SkillFrontmatter = serde_yaml::from_str(&frontmatter_str)?;

    // Validate metadata fields
    validate_skill_metadata(
        &frontmatter.name,
        &frontmatter.description,
        frontmatter.metadata.short_description.as_deref(),
    )?;

    // Validate capabilities
    validate_capabilities(&frontmatter.capabilities)?;
    validate_skill_compatibility(&frontmatter.compatibility)?;

    let id = infer_skill_id(path, &frontmatter.name);

    Ok(SkillMetadata {
        id,
        package_id,
        name: frontmatter.name,
        description: frontmatter.description,
        short_description: frontmatter.metadata.short_description,
        path: path.to_path_buf(),
        package_root: None,
        resource_root: None,
        scope,
        tags: frontmatter.metadata.tags,
        capabilities: Some(frontmatter.capabilities),
        compatibility: frontmatter.compatibility,
        source,
        enabled: true,
        allow_implicit_invocation: true,
        alan_metadata: AlanSkillRuntimeMetadata::default(),
        compatible_metadata: CompatibleSkillMetadata::default(),
        execution: ResolvedSkillExecution::default(),
    })
}

/// Load full skill content (metadata + body).
pub fn load_skill(path: &Path, scope: SkillScope) -> Result<Skill, SkillsError> {
    let content = std::fs::read_to_string(path)?;
    load_skill_from_content(
        &content,
        path,
        scope,
        SkillContentSource::File(path.to_path_buf()),
        None,
    )
}

/// Load full skill content from an explicit content source.
pub fn load_skill_from_content(
    content: &str,
    path: &Path,
    scope: SkillScope,
    source: SkillContentSource,
    package_id: Option<CapabilityPackageId>,
) -> Result<Skill, SkillsError> {
    let (frontmatter_str, body) =
        extract_frontmatter(content).ok_or(SkillsError::MissingFrontmatter)?;

    let frontmatter: SkillFrontmatter = serde_yaml::from_str(&frontmatter_str)?;

    // Validate metadata fields
    validate_skill_metadata(
        &frontmatter.name,
        &frontmatter.description,
        frontmatter.metadata.short_description.as_deref(),
    )?;

    // Validate capabilities
    validate_capabilities(&frontmatter.capabilities)?;
    validate_skill_compatibility(&frontmatter.compatibility)?;

    let id = infer_skill_id(path, &frontmatter.name);

    let metadata = SkillMetadata {
        id,
        package_id,
        name: frontmatter.name.clone(),
        description: frontmatter.description.clone(),
        short_description: frontmatter.metadata.short_description.clone(),
        path: path.to_path_buf(),
        package_root: None,
        resource_root: None,
        scope,
        tags: frontmatter.metadata.tags.clone(),
        capabilities: Some(frontmatter.capabilities.clone()),
        compatibility: frontmatter.compatibility.clone(),
        source,
        enabled: true,
        allow_implicit_invocation: true,
        alan_metadata: AlanSkillRuntimeMetadata::default(),
        compatible_metadata: CompatibleSkillMetadata::default(),
        execution: ResolvedSkillExecution::default(),
    };

    Ok(Skill {
        metadata,
        content: body,
        frontmatter,
    })
}

const MAX_SCAN_DEPTH: usize = 6;
const MAX_SKILLS_DIRS_PER_ROOT: usize = 2000;
const CHILD_AGENT_EXPORT_DIR: &str = "agents";

#[derive(Debug, Default, serde::Deserialize)]
struct OpenAiMetadataFile {
    #[serde(default)]
    interface: CompatibleSkillInterface,
    #[serde(default)]
    dependencies: CompatibleSkillDependencies,
    #[serde(default)]
    policy: CompatibleSkillPolicy,
}

pub fn skill_sidecar_path(skill_path: &Path) -> Option<PathBuf> {
    skill_path.parent().map(|dir| dir.join(SKILL_SIDECAR_FILE))
}

pub fn package_sidecar_path(package_root: &Path) -> PathBuf {
    package_root.join(PACKAGE_SIDECAR_FILE)
}

pub fn compatibility_metadata_path(package_root: &Path) -> PathBuf {
    package_root
        .join(COMPATIBILITY_METADATA_DIR)
        .join(COMPATIBILITY_METADATA_FILE)
}

pub fn load_skill_sidecar(skill_path: &Path) -> Result<Option<AlanSkillSidecar>, SkillsError> {
    let Some(path) = skill_sidecar_path(skill_path) else {
        return Ok(None);
    };
    load_optional_yaml(&path)
}

pub fn load_package_sidecar(
    package_root: &Path,
) -> Result<Option<AlanPackageSidecar>, SkillsError> {
    load_optional_yaml(&package_sidecar_path(package_root))
}

pub fn load_compatibility_metadata(
    package_root: &Path,
) -> Result<Option<CompatibleSkillMetadata>, SkillsError> {
    let Some(mut parsed) =
        load_optional_yaml::<OpenAiMetadataFile>(&compatibility_metadata_path(package_root))?
    else {
        return Ok(None);
    };

    normalize_compatibility_interface_paths(package_root, &mut parsed.interface);

    let metadata = CompatibleSkillMetadata {
        interface: parsed.interface,
        dependencies: parsed.dependencies,
        policy: parsed.policy,
    };

    if metadata.is_empty() {
        Ok(None)
    } else {
        Ok(Some(metadata))
    }
}

fn load_optional_yaml<T>(path: &Path) -> Result<Option<T>, SkillsError>
where
    T: DeserializeOwned,
{
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(SkillsError::Io(err)),
    };
    Ok(Some(serde_yaml::from_str(&content)?))
}

fn normalize_compatibility_interface_paths(
    package_root: &Path,
    interface: &mut CompatibleSkillInterface,
) {
    interface.icon_small = interface
        .icon_small
        .take()
        .map(|path| normalize_compatibility_asset_path(package_root, path));
    interface.icon_large = interface
        .icon_large
        .take()
        .map(|path| normalize_compatibility_asset_path(package_root, path));
}

fn normalize_compatibility_asset_path(package_root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        return path;
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    package_root.join(normalized)
}

fn infer_skill_id(path: &Path, declared_name: &str) -> SkillId {
    path.parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .map(name_to_id)
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| name_to_id(declared_name))
}

/// Scan a directory for skills (recursive).
pub fn scan_skills_dir(dir: &Path, scope: SkillScope) -> SkillLoadOutcome {
    let mut outcome = SkillLoadOutcome::default();
    outcome.tracked_paths.push(dir.to_path_buf());

    if !dir.exists() {
        debug!("Skills directory does not exist: {}", dir.display());
        return outcome;
    }

    if !dir.is_dir() {
        warn!("Skills path is not a directory: {}", dir.display());
        return outcome;
    }

    let follow_symlinks = matches!(scope, SkillScope::Repo | SkillScope::User);
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::from([(dir.to_path_buf(), 0)]);
    let mut visited_dirs: HashSet<PathBuf> = HashSet::new();
    let mut seen_skills: HashSet<PathBuf> = HashSet::new();
    visited_dirs.insert(dir.to_path_buf());
    let mut truncated = false;

    while let Some((current, depth)) = queue.pop_front() {
        outcome.tracked_paths.push(current.clone());
        if depth > MAX_SCAN_DEPTH {
            continue;
        }
        let skill_path = current.join("SKILL.md");
        let current_is_package_root = skill_path.is_file();
        if current_is_package_root {
            let resolved =
                std::fs::canonicalize(&skill_path).unwrap_or_else(|_| skill_path.clone());
            outcome.tracked_paths.push(resolved.clone());
            if seen_skills.insert(resolved.clone()) {
                match load_skill_metadata(&resolved, scope) {
                    Ok(metadata) => {
                        debug!("Loaded skill: {} from {}", metadata.id, resolved.display());
                        outcome.skills.push(metadata);
                    }
                    Err(e) => {
                        error!("Failed to load skill from {}: {}", resolved.display(), e);
                        outcome.errors.push(SkillError {
                            path: resolved,
                            message: e.to_string(),
                        });
                    }
                }
            }
            let child_agents_dir = current.join(CHILD_AGENT_EXPORT_DIR);
            outcome.tracked_paths.push(child_agents_dir.clone());
            if let Ok(resolved) = std::fs::canonicalize(&child_agents_dir) {
                outcome.tracked_paths.push(resolved);
            }
            continue;
        }
        if visited_dirs.len() >= MAX_SKILLS_DIRS_PER_ROOT {
            truncated = true;
            break;
        }

        let entries = match std::fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(e) => {
                error!(
                    "Failed to read skills directory {}: {}",
                    current.display(),
                    e
                );
                continue;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name,
                None => continue,
            };

            if file_name.starts_with('.') {
                continue;
            }
            if current_is_package_root && file_name == CHILD_AGENT_EXPORT_DIR {
                continue;
            }

            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(_) => continue,
            };

            if file_type.is_symlink() {
                if !follow_symlinks {
                    continue;
                }

                outcome.tracked_paths.push(path.clone());
                let metadata = match std::fs::metadata(&path) {
                    Ok(metadata) => metadata,
                    Err(e) => {
                        error!(
                            "Failed to stat skills entry {} (symlink): {}",
                            path.display(),
                            e
                        );
                        continue;
                    }
                };

                if metadata.is_dir() {
                    let resolved = match std::fs::canonicalize(&path) {
                        Ok(resolved) => resolved,
                        Err(_) => continue,
                    };
                    if visited_dirs.insert(resolved.clone()) {
                        outcome.tracked_paths.push(resolved.clone());
                        queue.push_back((resolved, depth + 1));
                    }
                }
                continue;
            }

            if file_type.is_dir() {
                let resolved = match std::fs::canonicalize(&path) {
                    Ok(resolved) => resolved,
                    Err(_) => continue,
                };
                if visited_dirs.insert(resolved.clone()) {
                    outcome.tracked_paths.push(resolved.clone());
                    queue.push_back((resolved, depth + 1));
                }
                continue;
            }
        }
    }

    if truncated {
        warn!(
            "Skills scan truncated after {} directories (root: {})",
            MAX_SKILLS_DIRS_PER_ROOT,
            dir.display()
        );
    }

    outcome.tracked_paths.sort();
    outcome.tracked_paths.dedup();

    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_load_skill_metadata() {
        let content = r#"---
name: Test Skill
description: A test skill for testing
metadata:
  short-description: Short desc
  tags:
    - test
    - demo
---

# Test Skill Body

This is the body content.
"#;

        let metadata = parse_skill_metadata(
            content,
            Path::new("/tmp/test-skill/SKILL.md"),
            SkillScope::User,
        )
        .unwrap();

        assert_eq!(metadata.id, "test-skill");
        assert_eq!(metadata.name, "Test Skill");
        assert_eq!(metadata.description, "A test skill for testing");
        assert_eq!(metadata.short_description, Some("Short desc".to_string()));
        assert_eq!(metadata.tags, vec!["test", "demo"]);
        assert_eq!(metadata.scope, SkillScope::User);
    }

    #[test]
    fn test_load_skill_metadata_uses_package_directory_for_runtime_id() {
        let content = r#"---
name: Human Friendly Title
description: A test skill for testing
---

Body
"#;

        let metadata = parse_skill_metadata(
            content,
            Path::new("/tmp/release-check/SKILL.md"),
            SkillScope::User,
        )
        .unwrap();

        assert_eq!(metadata.id, "release-check");
        assert_eq!(metadata.name, "Human Friendly Title");
    }

    #[test]
    fn test_load_skill_metadata_normalizes_package_directory_for_runtime_id() {
        let content = r#"---
name: Human Friendly Title
description: A test skill for testing
---

Body
"#;

        let metadata = parse_skill_metadata(
            content,
            Path::new("/tmp/repo.review/SKILL.md"),
            SkillScope::User,
        )
        .unwrap();

        assert_eq!(metadata.id, "repo-review");
        assert_eq!(metadata.name, "Human Friendly Title");
    }

    #[test]
    fn test_load_skill_metadata_accepts_long_portable_fields() {
        let name = "N".repeat(96);
        let description = "D".repeat(2048);
        let short_description = "S".repeat(2048);
        let content = format!(
            r#"---
name: "{name}"
description: "{description}"
metadata:
  short-description: "{short_description}"
---

Body
"#
        );

        let metadata = parse_skill_metadata(
            &content,
            Path::new("/tmp/release-check/SKILL.md"),
            SkillScope::User,
        )
        .unwrap();

        assert_eq!(metadata.name, name);
        assert_eq!(metadata.description, description);
        assert_eq!(metadata.short_description, Some(short_description));
    }

    #[test]
    fn test_scan_skills_dir() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");

        // Create a valid skill
        let skill_dir = skills_dir.join("test-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        let mut file = std::fs::File::create(skill_dir.join("SKILL.md")).unwrap();
        writeln!(
            file,
            r#"---
name: Test Skill
description: A test skill
---

Body
"#
        )
        .unwrap();

        // Create a directory without SKILL.md (should be ignored)
        std::fs::create_dir(skills_dir.join("invalid")).unwrap();

        // Create a hidden directory (should be ignored)
        std::fs::create_dir(skills_dir.join(".hidden")).unwrap();

        let outcome = scan_skills_dir(&skills_dir, SkillScope::User);
        assert_eq!(outcome.skills.len(), 1);
        assert_eq!(outcome.skills[0].id, "test-skill");
    }

    #[cfg(unix)]
    #[test]
    fn test_scan_skills_dir_tracks_symlink_path_and_target() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        let pack_v1 = temp.path().join("pack-v1");
        let linked_pack = skills_dir.join("linked-pack");

        std::fs::create_dir_all(pack_v1.join("demo-skill")).unwrap();
        std::fs::create_dir_all(&skills_dir).unwrap();
        std::fs::write(
            pack_v1.join("demo-skill/SKILL.md"),
            r#"---
name: Demo Skill
description: Linked skill
---

Body
"#,
        )
        .unwrap();
        symlink(&pack_v1, &linked_pack).unwrap();

        let outcome = scan_skills_dir(&skills_dir, SkillScope::Repo);
        let resolved = std::fs::canonicalize(&linked_pack).unwrap();

        assert_eq!(outcome.skills.len(), 1);
        assert!(outcome.tracked_paths.contains(&linked_pack));
        assert!(outcome.tracked_paths.contains(&resolved));
    }

    #[test]
    fn test_scan_skills_dir_skips_child_agent_skill_subtrees() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");

        std::fs::create_dir_all(skills_dir.join("parent-skill")).unwrap();
        std::fs::write(
            skills_dir.join("parent-skill/SKILL.md"),
            r#"---
name: Parent Skill
description: Parent-visible skill
---

Body
"#,
        )
        .unwrap();
        std::fs::create_dir_all(skills_dir.join("parent-skill/agents/reviewer/skills/child-only"))
            .unwrap();
        std::fs::write(
            skills_dir.join("parent-skill/agents/reviewer/skills/child-only/SKILL.md"),
            r#"---
name: Child Only
description: Child-agent-only skill
---

Body
"#,
        )
        .unwrap();

        let outcome = scan_skills_dir(&skills_dir, SkillScope::Repo);
        let skill_ids: Vec<_> = outcome
            .skills
            .iter()
            .map(|skill| skill.id.as_str())
            .collect();

        assert_eq!(skill_ids, vec!["parent-skill"]);
    }

    #[test]
    fn test_scan_skills_dir_allows_top_level_agents_package() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");

        std::fs::create_dir_all(skills_dir.join("agents")).unwrap();
        std::fs::write(
            skills_dir.join("agents/SKILL.md"),
            r#"---
name: Agents
description: A valid top-level package named agents
---

Body
"#,
        )
        .unwrap();
        std::fs::create_dir_all(skills_dir.join("agents/agents/reviewer/skills/child-only"))
            .unwrap();
        std::fs::write(
            skills_dir.join("agents/agents/reviewer/skills/child-only/SKILL.md"),
            r#"---
name: Child Only
description: Child-agent-only skill
---

Body
"#,
        )
        .unwrap();

        let outcome = scan_skills_dir(&skills_dir, SkillScope::Repo);
        let skill_ids: Vec<_> = outcome
            .skills
            .iter()
            .map(|skill| skill.id.as_str())
            .collect();

        assert_eq!(skill_ids, vec!["agents"]);
    }

    #[test]
    fn test_scan_skills_dir_tracks_child_agent_export_dir_even_when_missing() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");

        std::fs::create_dir_all(skills_dir.join("parent-skill")).unwrap();
        std::fs::write(
            skills_dir.join("parent-skill/SKILL.md"),
            r#"---
name: Parent Skill
description: Parent-visible skill
---

Body
"#,
        )
        .unwrap();

        let outcome = scan_skills_dir(&skills_dir, SkillScope::Repo);
        let expected_agents_dir = std::fs::canonicalize(skills_dir.join("parent-skill"))
            .unwrap()
            .join("agents");

        assert!(outcome.tracked_paths.contains(&expected_agents_dir));
    }

    #[test]
    fn test_scan_skills_dir_ignores_nested_skill_markdown_inside_package_assets() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");

        std::fs::create_dir_all(skills_dir.join("parent-skill/references/examples/nested"))
            .unwrap();
        std::fs::write(
            skills_dir.join("parent-skill/SKILL.md"),
            r#"---
name: Parent Skill
description: Parent-visible skill
---

Body
"#,
        )
        .unwrap();
        std::fs::write(
            skills_dir.join("parent-skill/references/examples/nested/SKILL.md"),
            r#"---
name: Nested Example
description: Should stay an ignored package asset
---

Body
"#,
        )
        .unwrap();

        let outcome = scan_skills_dir(&skills_dir, SkillScope::Repo);
        let skill_ids: Vec<_> = outcome
            .skills
            .iter()
            .map(|skill| skill.id.as_str())
            .collect();

        assert_eq!(skill_ids, vec!["parent-skill"]);
    }

    #[test]
    fn test_load_full_skill() {
        let temp = TempDir::new().unwrap();
        let skill_dir = temp.path().join("full-test");
        std::fs::create_dir_all(&skill_dir).unwrap();
        let skill_md = skill_dir.join("SKILL.md");

        let content = r#"---
name: Full Test
description: Testing full load
---

# Body

Content here.
"#;

        std::fs::write(&skill_md, content).unwrap();

        let skill = load_skill(&skill_md, SkillScope::Repo).unwrap();
        assert_eq!(skill.metadata.id, "full-test");
        assert!(skill.content.contains("# Body"));
    }
}
