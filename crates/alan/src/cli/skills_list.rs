use std::collections::BTreeMap;
use std::path::Path;

use alan_agent_engine::skills::preinstalled_skill_package_sources;
use alan_os_host::SystemStorePaths;
use alan_service_manager::{PackageKind, PackageService, PackageState};
use anyhow::Result;
use serde::Serialize;

/// Serializable summary of an installed or preinstalled package and its exported skills.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ListedSkillPackage {
    /// Package identifier from catalog or preinstalled registry.
    pub package_id: String,
    /// Origin category (`"preinstalled"` or `"installed"`).
    pub kind: String,
    /// Package lifecycle state (`"available"`, `"installed"`, or `"retiring"`).
    pub state: String,
    /// Exported skill IDs provided by this package.
    pub skills: Vec<String>,
    /// Active installed revision, if tracked in the package catalog.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

#[derive(Debug, Serialize)]
struct SkillsListReport {
    packages: Vec<ListedSkillPackage>,
}

/// Print installed and preinstalled skill packages in tabular or JSON format.
pub fn run_list_skills(json: bool) -> Result<()> {
    let store = SystemStorePaths::detect()?;
    let packages_dir = store.packages()?;
    let packages = collect_listed_packages(Some(&packages_dir))?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&SkillsListReport { packages })?
        );
        return Ok(());
    }
    if packages.is_empty() {
        println!("No skill packages found.");
        return Ok(());
    }
    println!("{:<28} {:<14} {:<10} SKILLS", "PACKAGE", "KIND", "STATE");
    for pkg in packages {
        let skills = if pkg.skills.is_empty() {
            "-".to_string()
        } else {
            pkg.skills.join(", ")
        };
        println!(
            "{:<28} {:<14} {:<10} {}",
            pkg.package_id, pkg.kind, pkg.state, skills
        );
    }
    Ok(())
}

pub(crate) fn collect_listed_packages(
    packages_dir: Option<&Path>,
) -> Result<Vec<ListedSkillPackage>> {
    let mut entries: BTreeMap<String, ListedSkillPackage> = BTreeMap::new();

    for source in preinstalled_skill_package_sources() {
        let skill_name =
            extract_skill_md_name(&source.files).unwrap_or_else(|| source.source_name.to_string());
        entries.insert(
            source.package_id.to_string(),
            ListedSkillPackage {
                package_id: source.package_id.to_string(),
                kind: "preinstalled".to_string(),
                state: "available".to_string(),
                skills: vec![skill_name],
                revision: None,
            },
        );
    }

    if let Some(dir) = packages_dir
        && dir.is_dir()
        && (dir.join("catalog.json").exists() || dir.join("revisions").is_dir())
    {
        let service = PackageService::open(dir.to_path_buf())?;
        let catalog = service.catalog()?;
        for (id, record) in catalog.packages {
            let skills = record
                .exports
                .iter()
                .map(|export| export.skill_id.clone())
                .collect::<Vec<_>>();
            let kind = match record.kind {
                PackageKind::Preinstalled => "preinstalled",
                PackageKind::Installed => "installed",
            }
            .to_string();
            let state = match record.state {
                PackageState::Installed => "installed",
                PackageState::Retiring => "retiring",
            }
            .to_string();
            entries.insert(
                id.clone(),
                ListedSkillPackage {
                    package_id: id,
                    kind,
                    state,
                    skills,
                    revision: Some(record.revision),
                },
            );
        }
    }

    Ok(entries.into_values().collect())
}

fn extract_skill_md_name(
    files: &[alan_agent_engine::skills::PreinstalledSkillFile],
) -> Option<String> {
    let skill_md = files
        .iter()
        .find(|file| file.path == Path::new("SKILL.md"))?;
    let content = std::str::from_utf8(skill_md.bytes).ok()?;
    let mut lines = content.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            break;
        }
        if let Some(rest) = trimmed.strip_prefix("name:") {
            let name = rest.trim().trim_matches('"').trim_matches('\'');
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}
