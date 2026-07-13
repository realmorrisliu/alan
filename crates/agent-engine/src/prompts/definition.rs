//! Descriptor-local Agent Definition persona files for prompt assembly.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tracing::debug;

pub const DEFAULT_AGENTS_FILENAME: &str = "AGENTS.md";
pub const DEFAULT_SOUL_FILENAME: &str = "SOUL.md";
pub const DEFAULT_ROLE_FILENAME: &str = "ROLE.md";
pub const DEFAULT_USER_FILENAME: &str = "USER.md";
pub const DEFAULT_TOOLS_FILENAME: &str = "TOOLS.md";
pub const DEFAULT_HEARTBEAT_FILENAME: &str = "HEARTBEAT.md";
pub const DEFAULT_BOOTSTRAP_FILENAME: &str = "BOOTSTRAP.md";
pub(crate) const DEFINITION_PERSONA_MAX_CHARS: usize = 6_000;

const BOOTSTRAP_HEAD_RATIO: f32 = 0.7;
const BOOTSTRAP_TAIL_RATIO: f32 = 0.2;

#[derive(Debug, Clone)]
struct DefinitionTemplate {
    name: &'static str,
    content: &'static str,
}

const REQUIRED_DEFINITION_TEMPLATES: [DefinitionTemplate; 6] = [
    DefinitionTemplate {
        name: DEFAULT_AGENTS_FILENAME,
        content: include_str!("../../prompts/persona/AGENTS.md"),
    },
    DefinitionTemplate {
        name: DEFAULT_SOUL_FILENAME,
        content: include_str!("../../prompts/persona/SOUL.md"),
    },
    DefinitionTemplate {
        name: DEFAULT_ROLE_FILENAME,
        content: include_str!("../../prompts/persona/ROLE.md"),
    },
    DefinitionTemplate {
        name: DEFAULT_USER_FILENAME,
        content: include_str!("../../prompts/persona/USER.md"),
    },
    DefinitionTemplate {
        name: DEFAULT_TOOLS_FILENAME,
        content: include_str!("../../prompts/persona/TOOLS.md"),
    },
    DefinitionTemplate {
        name: DEFAULT_HEARTBEAT_FILENAME,
        content: include_str!("../../prompts/persona/HEARTBEAT.md"),
    },
];

const OPTIONAL_BOOTSTRAP_TEMPLATE: DefinitionTemplate = DefinitionTemplate {
    name: DEFAULT_BOOTSTRAP_FILENAME,
    content: include_str!("../../prompts/persona/BOOTSTRAP.md"),
};

#[derive(Debug, Clone)]
pub struct DefinitionFile {
    pub name: &'static str,
    pub content: Option<String>,
    pub missing: bool,
}

pub fn ensure_definition_bootstrap_files_at(definition_dir: &Path) -> io::Result<()> {
    fs::create_dir_all(definition_dir)?;

    let required_paths: Vec<PathBuf> = REQUIRED_DEFINITION_TEMPLATES
        .iter()
        .map(|template| definition_dir.join(template.name))
        .collect();
    let is_brand_new_definition = required_paths.iter().all(|path| !path.exists());

    for template in REQUIRED_DEFINITION_TEMPLATES {
        let path = definition_dir.join(template.name);
        write_file_if_missing(&path, template.content)?;
    }

    if is_brand_new_definition {
        let bootstrap_path = definition_dir.join(OPTIONAL_BOOTSTRAP_TEMPLATE.name);
        write_file_if_missing(&bootstrap_path, OPTIONAL_BOOTSTRAP_TEMPLATE.content)?;
    }

    Ok(())
}

#[allow(dead_code)]
pub fn load_definition_files(definition_dir: &Path) -> Vec<DefinitionFile> {
    load_definition_files_from_dirs(&[definition_dir.to_path_buf()])
}

pub fn load_definition_files_from_dirs(definition_dirs: &[PathBuf]) -> Vec<DefinitionFile> {
    let mut files = Vec::new();
    for template in REQUIRED_DEFINITION_TEMPLATES {
        files.push(read_definition_file_from_dirs(
            definition_dirs,
            template.name,
            /* optional */ false,
        ));
    }

    if overlay_definition_file_path(definition_dirs, OPTIONAL_BOOTSTRAP_TEMPLATE.name).is_some() {
        files.push(read_definition_file_from_dirs(
            definition_dirs,
            OPTIONAL_BOOTSTRAP_TEMPLATE.name,
            /* optional */ true,
        ));
    }

    files
}

#[allow(dead_code)]
pub(crate) fn definition_persona_tracked_paths(definition_dir: &Path) -> Vec<PathBuf> {
    definition_persona_tracked_paths_from_dirs(&[definition_dir.to_path_buf()])
}

pub(crate) fn definition_persona_tracked_paths_from_dirs(
    definition_dirs: &[PathBuf],
) -> Vec<PathBuf> {
    let mut tracked = Vec::new();
    for definition_dir in definition_dirs {
        tracked.push(definition_dir.join(DEFAULT_AGENTS_FILENAME));
        tracked.push(definition_dir.join(DEFAULT_SOUL_FILENAME));
        tracked.push(definition_dir.join(DEFAULT_ROLE_FILENAME));
        tracked.push(definition_dir.join(DEFAULT_USER_FILENAME));
        tracked.push(definition_dir.join(DEFAULT_TOOLS_FILENAME));
        tracked.push(definition_dir.join(DEFAULT_HEARTBEAT_FILENAME));
        tracked.push(definition_dir.join(DEFAULT_BOOTSTRAP_FILENAME));
    }
    tracked.sort();
    tracked.dedup();
    tracked
}

#[allow(dead_code)]
pub(crate) fn render_definition_persona_context(definition_dir: &Path) -> String {
    render_definition_persona_context_from_dirs(&[definition_dir.to_path_buf()])
}

pub(crate) fn render_definition_persona_context_from_dirs(definition_dirs: &[PathBuf]) -> String {
    let files = load_definition_files_from_dirs(definition_dirs);
    if files.is_empty() || files.iter().all(|file| file.missing) {
        return String::new();
    }
    let mut prompt = String::new();
    prompt.push_str("## Agent Definition Persona\n");
    prompt.push_str(
        "The following descriptor-local persona files are already injected and define the persona, role, and operating style.\n",
    );
    prompt.push_str(
        "Do not re-read them by default. Agent Definitions are authored or installed explicitly; this Process must not infer writable Host locations for them.\n",
    );
    prompt.push_str(
        "Only persist user-confirmed stable information. Do not store guesses, inferred traits, or transient machine focus in `USER.md`.\n",
    );

    for file in files {
        prompt.push_str(&format!("\n### {}\n", file.name));
        if file.missing {
            prompt.push_str("[MISSING FROM AGENT DEFINITION DESCRIPTOR]\n");
            continue;
        }
        prompt.push_str(&format!(
            "Descriptor path: /agent-definition/persona/{}\n",
            file.name
        ));
        let content = file.content.unwrap_or_default();
        let trimmed = trim_definition_content(&content, file.name, DEFINITION_PERSONA_MAX_CHARS);
        if trimmed.is_empty() {
            prompt.push_str("[EMPTY]\n");
        } else {
            prompt.push_str(trimmed.as_str());
            prompt.push('\n');
        }
    }

    prompt
}

fn write_file_if_missing(path: &Path, content: &str) -> io::Result<()> {
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(content.as_bytes())?;
            debug!(path = %path.display(), "Created Agent Definition bootstrap file");
            Ok(())
        }
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(err) => Err(err),
    }
}

fn read_definition_file_from_dirs(
    definition_dirs: &[PathBuf],
    name: &'static str,
    _optional: bool,
) -> DefinitionFile {
    if let Some(path) = overlay_definition_file_path(definition_dirs, name) {
        return match fs::read_to_string(&path) {
            Ok(content) => DefinitionFile {
                name,
                content: Some(content),
                missing: false,
            },
            Err(err) => DefinitionFile {
                name,
                content: Some(format!("[ERROR] Failed to read file: {}", err)),
                missing: false,
            },
        };
    }

    DefinitionFile {
        name,
        content: None,
        missing: true,
    }
}

fn overlay_definition_file_path(
    definition_dirs: &[PathBuf],
    name: &'static str,
) -> Option<PathBuf> {
    definition_dirs
        .iter()
        .rev()
        .map(|dir| dir.join(name))
        .find(|path| path.exists())
}

fn trim_definition_content(content: &str, file_name: &str, max_chars: usize) -> String {
    let trimmed = content.trim_end();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }

    let head_chars = ((max_chars as f32) * BOOTSTRAP_HEAD_RATIO).floor() as usize;
    let tail_chars = ((max_chars as f32) * BOOTSTRAP_TAIL_RATIO).floor() as usize;

    let head = take_chars(trimmed, head_chars);
    let tail = take_last_chars(trimmed, tail_chars);
    let marker = format!(
        "\n[...truncated, read {} for full content...]\n...(truncated {}: kept {}+{} chars)...\n",
        file_name, file_name, head_chars, tail_chars
    );

    format!("{}{}{}", head, marker, tail)
}

fn take_chars(input: &str, count: usize) -> String {
    input.chars().take(count).collect()
}

fn take_last_chars(input: &str, count: usize) -> String {
    let chars: Vec<char> = input.chars().collect();
    let start = chars.len().saturating_sub(count);
    chars[start..].iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_ensure_definition_bootstrap_files_creates_required_templates() {
        let temp_dir = TempDir::new().unwrap();
        ensure_definition_bootstrap_files_at(temp_dir.path()).unwrap();

        for name in [
            DEFAULT_AGENTS_FILENAME,
            DEFAULT_SOUL_FILENAME,
            DEFAULT_ROLE_FILENAME,
            DEFAULT_USER_FILENAME,
            DEFAULT_TOOLS_FILENAME,
            DEFAULT_HEARTBEAT_FILENAME,
            DEFAULT_BOOTSTRAP_FILENAME,
        ] {
            assert!(
                temp_dir.path().join(name).exists(),
                "expected {} to be created",
                name
            );
        }
    }

    #[test]
    fn test_ensure_definition_bootstrap_files_does_not_create_bootstrap_when_not_brand_new() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join(DEFAULT_AGENTS_FILENAME),
            "# Existing definition",
        )
        .unwrap();

        ensure_definition_bootstrap_files_at(temp_dir.path()).unwrap();

        assert!(!temp_dir.path().join(DEFAULT_BOOTSTRAP_FILENAME).exists());
        assert!(temp_dir.path().join(DEFAULT_SOUL_FILENAME).exists());
        assert!(temp_dir.path().join(DEFAULT_ROLE_FILENAME).exists());
    }

    #[test]
    fn test_ensure_definition_bootstrap_files_preserves_existing_content() {
        let temp_dir = TempDir::new().unwrap();
        let soul_path = temp_dir.path().join(DEFAULT_SOUL_FILENAME);
        fs::write(&soul_path, "custom soul").unwrap();

        ensure_definition_bootstrap_files_at(temp_dir.path()).unwrap();

        let content = fs::read_to_string(soul_path).unwrap();
        assert_eq!(content, "custom soul");
    }

    #[test]
    fn test_render_definition_persona_context_adds_runtime_guidance() {
        let temp_dir = TempDir::new().unwrap();
        ensure_definition_bootstrap_files_at(temp_dir.path()).unwrap();

        let prompt = render_definition_persona_context(temp_dir.path());

        assert!(prompt.contains("already injected and define the persona"));
        assert!(prompt.contains("Do not re-read them by default"));
        assert!(prompt.contains("Only persist user-confirmed stable information"));
        assert!(prompt.contains("Descriptor path: /agent-definition/persona/"));
        assert!(!prompt.contains(temp_dir.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn test_load_definition_files_uses_last_explicit_definition_directory() {
        let temp_dir = TempDir::new().unwrap();
        let global_dir = temp_dir.path().join("global");
        let definition_dir = temp_dir.path().join("definition");
        fs::create_dir_all(&global_dir).unwrap();
        fs::create_dir_all(&definition_dir).unwrap();
        fs::write(global_dir.join(DEFAULT_USER_FILENAME), "global user").unwrap();

        let files = load_definition_files_from_dirs(&[global_dir.clone(), definition_dir.clone()]);
        let user_file = files
            .iter()
            .find(|file| file.name == DEFAULT_USER_FILENAME)
            .expect("expected USER.md entry");

        assert_eq!(user_file.content.as_deref(), Some("global user"));
        assert!(!user_file.missing);
    }

    #[test]
    fn test_render_definition_persona_context_hides_host_paths() {
        let temp_dir = TempDir::new().unwrap();
        let global_dir = temp_dir.path().join("global");
        let definition_dir = temp_dir.path().join("definition");
        fs::create_dir_all(&global_dir).unwrap();
        fs::create_dir_all(&definition_dir).unwrap();
        fs::write(global_dir.join(DEFAULT_USER_FILENAME), "global user").unwrap();

        let prompt = render_definition_persona_context_from_dirs(&[
            global_dir.clone(),
            definition_dir.clone(),
        ]);

        assert!(prompt.contains("Descriptor path: /agent-definition/persona/USER.md"));
        assert!(!prompt.contains(global_dir.to_string_lossy().as_ref()));
        assert!(!prompt.contains(definition_dir.to_string_lossy().as_ref()));
    }

    #[test]
    fn test_render_definition_persona_context_never_infers_write_target() {
        let temp_dir = TempDir::new().unwrap();
        ensure_definition_bootstrap_files_at(temp_dir.path()).unwrap();

        let prompt = render_definition_persona_context(temp_dir.path());

        assert!(!prompt.contains("Write updates to:"));
        assert!(!prompt.contains(temp_dir.path().to_string_lossy().as_ref()));
    }
}
