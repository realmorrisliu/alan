//! Runtime prompt loading with optional override support.

use std::path::PathBuf;

/// Prompt loader with support for runtime overrides.
///
/// The loader first checks for custom prompts in the configured directory,
/// then falls back to the compiled-in defaults.
pub struct PromptLoader {
    /// Optional directory for custom prompt overrides
    custom_dir: Option<PathBuf>,
}

impl PromptLoader {
    /// Create a new prompt loader without custom overrides
    pub fn new() -> Self {
        Self { custom_dir: None }
    }

    /// Create a prompt loader with a custom override directory
    pub fn with_custom_dir(dir: PathBuf) -> Self {
        Self {
            custom_dir: Some(dir),
        }
    }

    /// Load a prompt by name, checking custom directory first
    pub fn load(&self, name: &str) -> String {
        // Try custom directory first
        if let Some(ref dir) = self.custom_dir {
            let custom_path = dir.join(format!("{}.md", name));
            if let Ok(content) = std::fs::read_to_string(&custom_path) {
                tracing::debug!(?custom_path, "Loaded custom prompt");
                return content;
            }
        }

        // Fall back to compiled-in prompts
        match name {
            "system" => super::SYSTEM_PROMPT.to_string(),
            _ => {
                tracing::warn!(name, "Unknown prompt requested, returning empty");
                String::new()
            }
        }
    }

    /// Get the system prompt (convenience method)
    pub fn system_prompt(&self) -> String {
        self.load("system")
    }

    /// Get a template
    pub fn template(&self, name: &str) -> String {
        self.load(name)
    }
}

impl Default for PromptLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_load_system_prompt() {
        let loader = PromptLoader::new();
        let prompt = loader.system_prompt();
        assert!(prompt.contains("You are alan"));
    }

    #[test]
    fn test_unknown_prompt() {
        let loader = PromptLoader::new();
        let prompt = loader.load("nonexistent");
        assert!(prompt.is_empty());
    }

    #[test]
    fn test_prompt_loader_new() {
        let loader = PromptLoader::new();
        assert!(loader.custom_dir.is_none());
    }

    #[test]
    fn test_prompt_loader_default() {
        let loader: PromptLoader = Default::default();
        assert!(loader.custom_dir.is_none());
    }

    #[test]
    fn test_prompt_loader_with_custom_dir() {
        let temp_dir = TempDir::new().unwrap();
        let loader = PromptLoader::with_custom_dir(temp_dir.path().to_path_buf());
        assert!(loader.custom_dir.is_some());
    }

    #[test]
    fn test_load_with_custom_override() {
        let temp_dir = TempDir::new().unwrap();
        let custom_prompt_path = temp_dir.path().join("system.md");

        // Write a custom system prompt
        let mut file = std::fs::File::create(&custom_prompt_path).unwrap();
        file.write_all(b"Custom system prompt override").unwrap();
        drop(file);

        let loader = PromptLoader::with_custom_dir(temp_dir.path().to_path_buf());
        let prompt = loader.load("system");

        // Should load from custom directory
        assert_eq!(prompt, "Custom system prompt override");
    }

    #[test]
    fn test_load_custom_fallback_to_builtin() {
        let temp_dir = TempDir::new().unwrap();

        // Only create a custom prompt for one file
        let custom_prompt_path = temp_dir.path().join("custom.md");
        let mut file = std::fs::File::create(&custom_prompt_path).unwrap();
        file.write_all(b"Custom").unwrap();
        drop(file);

        let loader = PromptLoader::with_custom_dir(temp_dir.path().to_path_buf());

        // Should fall back to builtin for system prompt
        let system_prompt = loader.load("system");
        assert!(system_prompt.contains("You are alan"));
    }

    #[test]
    fn test_load_all_builtin_prompts() {
        let loader = PromptLoader::new();

        // Test all builtin prompts
        let system = loader.load("system");
        assert!(!system.is_empty());
    }

    #[test]
    fn test_template_method() {
        let loader = PromptLoader::new();

        // Domain-specific templates are loaded via custom_dir, not builtin
        let unknown = loader.template("some_template");
        assert!(unknown.is_empty());
    }

    #[test]
    fn test_empty_custom_dir_uses_builtin() {
        let temp_dir = TempDir::new().unwrap();
        let loader = PromptLoader::with_custom_dir(temp_dir.path().to_path_buf());

        // Should fall back to builtin since custom dir is empty
        let prompt = loader.load("system");
        assert!(prompt.contains("You are alan"));
    }

    #[test]
    fn test_load_nonexistent_file_in_custom_dir() {
        let temp_dir = TempDir::new().unwrap();
        let loader = PromptLoader::with_custom_dir(temp_dir.path().to_path_buf());

        // Should fall back to builtin (which is empty for unknown prompts)
        let prompt = loader.load("nonexistent");
        assert!(prompt.is_empty());
    }
}
