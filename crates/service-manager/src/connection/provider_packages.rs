//! Extension `provider` package resolution and availability gating for Connection Service.

use std::collections::BTreeMap;

use alan_agent_engine::LlmProvider;
use anyhow::{Context, Result, bail, ensure};

use crate::connection_profile::{
    ConnectionProfile, ConnectionsFile, first_party_provider_catalog, normalize_profile_settings,
    validate_profile_settings,
};
use crate::{PackageExportKind, PackageService, PackageState, ProviderPackageExport};

/// Prefix stored in [`ConnectionProfile::source`] for package-backed provider connections.
pub const PROVIDER_PACKAGE_SOURCE_PREFIX: &str = "pkg:";

/// Returns the durable `ConnectionProfile.source` marker binding a profile to `package_id`.
pub fn provider_package_source(package_id: &str) -> String {
    format!("{PROVIDER_PACKAGE_SOURCE_PREFIX}{package_id}")
}

/// Extracts the Quartermaster package ID when `source` references a provider package (`pkg:<id>`).
pub fn package_id_from_profile_source(source: &str) -> Option<&str> {
    source
        .strip_prefix(PROVIDER_PACKAGE_SOURCE_PREFIX)
        .filter(|id| !id.is_empty())
}

/// Maps an extension package protocol name onto Alan's built-in `crates/llm` protocol adapter.
pub fn protocol_to_llm_provider(protocol: &str) -> Result<LlmProvider> {
    match protocol.trim() {
        "anthropic_messages" => Ok(LlmProvider::AnthropicMessages),
        "openai_chat_completions" => Ok(LlmProvider::OpenAiChatCompletions),
        "openai_chat_completions_compatible" => Ok(LlmProvider::OpenAiChatCompletionsCompatible),
        "openai_responses" => Ok(LlmProvider::OpenAiResponses),
        other => bail!("unsupported extension provider protocol `{other}`"),
    }
}

/// Verifies whether `profile`'s backing provider package (if package-bound) is currently installed.
pub fn validate_provider_package_for_profile(
    packages: Option<&PackageService>,
    profile: &ConnectionProfile,
) -> Result<Option<ProviderPackageExport>> {
    let Some(package_id) = package_id_from_profile_source(&profile.source) else {
        return Ok(None);
    };
    let packages = packages.with_context(|| {
        format!("provider package `{package_id}` requires an active Package Service")
    })?;
    let record = packages.resolve(package_id).with_context(|| {
        format!("provider package `{package_id}` is not installed or is retiring")
    })?;
    ensure!(
        record.state == PackageState::Installed
            && record.export_kind == PackageExportKind::Provider,
        "package `{package_id}` is not an active provider package"
    );
    let export = record
        .provider_export
        .with_context(|| format!("package `{package_id}` lacks a provider adapter export"))?;
    let expected_provider = protocol_to_llm_provider(&export.protocol)?;
    ensure!(
        expected_provider == profile.provider,
        "provider package `{package_id}` protocol `{}` does not match profile provider `{}`",
        export.protocol,
        profile.provider.as_str()
    );
    Ok(Some(export))
}

/// Returns a copy of `connections` with omitted settings on `profile_id` filled from its installed
/// provider package descriptor (such as `default_base_url` and `default_model`).
pub(crate) fn hydrate_connections_with_package_defaults(
    packages: Option<&PackageService>,
    connections: &ConnectionsFile,
    profile_id: &str,
) -> ConnectionsFile {
    let mut hydrated = connections.clone();
    let Some(profile) = hydrated.profiles.get_mut(profile_id) else {
        return hydrated;
    };
    let Ok(Some(export)) = validate_provider_package_for_profile(packages, profile) else {
        return hydrated;
    };
    if profile
        .settings
        .get("base_url")
        .map(String::as_str)
        .map(str::trim)
        .is_none_or(str::is_empty)
    {
        profile
            .settings
            .insert("base_url".to_string(), export.default_base_url);
    }
    if profile
        .settings
        .get("model")
        .map(String::as_str)
        .map(str::trim)
        .is_none_or(str::is_empty)
        && let Some(default_model) = export.default_model
    {
        profile.settings.insert("model".to_string(), default_model);
    }
    hydrated
}

/// Resolves a CLI/API provider selector (`<first-party>`, `pkg:<package-id>`, or installed extension
/// protocol/id) without keeping non-first-party adapters in the built-in first-party wizard catalog.
pub fn resolve_provider_selection(
    raw: &str,
    packages: Option<&PackageService>,
) -> Result<(LlmProvider, Option<(String, ProviderPackageExport)>)> {
    let trimmed = raw.trim();
    ensure!(!trimmed.is_empty(), "provider must not be blank");

    if trimmed.starts_with(PROVIDER_PACKAGE_SOURCE_PREFIX) {
        let packages =
            packages.context("Package Service is unavailable to resolve `pkg:` provider")?;
        let (record, export) = packages.resolve_provider_package(trimmed)?;
        let llm_provider = protocol_to_llm_provider(&export.protocol)?;
        return Ok((llm_provider, Some((record.id, export))));
    }

    if let Some(builtin) = parse_builtin_provider_name(trimmed) {
        let descriptor = ConnectionsFile::profile_descriptor(builtin);
        if descriptor.first_party {
            return Ok((builtin, None));
        }
        if let Some(packages) = packages
            && let Ok((record, export)) = packages.resolve_provider_package(builtin.as_str())
        {
            let llm_provider = protocol_to_llm_provider(&export.protocol)?;
            return Ok((llm_provider, Some((record.id, export))));
        }
        let first_party = first_party_provider_catalog()
            .iter()
            .map(|d| d.provider_id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "Provider `{}` is not a built-in first-party provider (offered choices: {}; install a Quartermaster `provider` package first via `alan q install <path>`)",
            builtin.as_str(),
            first_party
        );
    }

    if let Some(packages) = packages
        && let Ok((record, export)) = packages.resolve_provider_package(trimmed)
    {
        let llm_provider = protocol_to_llm_provider(&export.protocol)?;
        return Ok((llm_provider, Some((record.id, export))));
    }

    let first_party = first_party_provider_catalog()
        .iter()
        .map(|d| d.provider_id.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    bail!(
        "Unsupported provider `{trimmed}` (first-party built-ins: {first_party}; or install a Quartermaster `provider` package)"
    )
}

/// Applies installed `provider` package default settings before normalizing/validating profile settings.
pub fn normalize_settings_with_provider_package(
    provider: LlmProvider,
    export: Option<&ProviderPackageExport>,
    overrides: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>> {
    let mut merged = BTreeMap::new();
    if let Some(export) = export {
        merged.insert("base_url".to_string(), export.default_base_url.clone());
        if let Some(model) = export.default_model.as_deref() {
            merged.insert("model".to_string(), model.to_string());
        }
    }
    for (key, value) in overrides {
        merged.insert(key.clone(), value.clone());
    }
    let normalized = normalize_profile_settings(provider, &merged);
    validate_profile_settings(provider, &normalized)?;
    Ok(normalized)
}

fn parse_builtin_provider_name(raw: &str) -> Option<LlmProvider> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "magpie" => Some(LlmProvider::Magpie),
        "openai_responses" | "openai-responses" | "openai" => Some(LlmProvider::OpenAiResponses),
        "chatgpt" | "chatgpt_responses" | "codex" => Some(LlmProvider::Chatgpt),
        "google_gemini_generate_content" | "vertex" | "gemini" => {
            Some(LlmProvider::GoogleGeminiGenerateContent)
        }
        "openrouter" => Some(LlmProvider::OpenRouter),
        "openai_chat_completions" => Some(LlmProvider::OpenAiChatCompletions),
        "openai_chat_completions_compatible" | "openai_compatible" | "compatible" => {
            Some(LlmProvider::OpenAiChatCompletionsCompatible)
        }
        "anthropic_messages" | "anthropic" => Some(LlmProvider::AnthropicMessages),
        "typesafe_evaluation" | "typesafe" => Some(LlmProvider::TypesafeEvaluation),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PackageCommand, PackageSnapshot, PackageSnapshotEntry};

    #[test]
    fn hydrates_omitted_base_url_and_model_from_installed_provider_package() {
        let packages = PackageService::ephemeral().unwrap();
        let manifest = std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("packages/anthropic-provider/alan-package.toml"),
        )
        .unwrap();
        let adapter = std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("packages/anthropic-provider/adapter.toml"),
        )
        .unwrap();
        let install = packages
            .execute(PackageCommand::Install {
                request_id: "install-anth".to_string(),
                package_id: "anthropic-provider".to_string(),
                snapshot: PackageSnapshot {
                    source_name: "anthropic-provider".to_string(),
                    entries: vec![
                        PackageSnapshotEntry {
                            path: "alan-package.toml".to_string(),
                            bytes: manifest,
                            executable: false,
                        },
                        PackageSnapshotEntry {
                            path: "adapter.toml".to_string(),
                            bytes: adapter,
                            executable: false,
                        },
                    ],
                },
            })
            .unwrap();
        assert!(install.success, "{}", install.message);

        let mut connections = ConnectionsFile::default();
        connections.profiles.insert(
            "claude".to_string(),
            ConnectionProfile {
                provider: LlmProvider::AnthropicMessages,
                label: None,
                credential_id: Some("claude-cred".to_string()),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                source: provider_package_source("anthropic-provider"),
                settings: BTreeMap::new(),
            },
        );
        let hydrated =
            hydrate_connections_with_package_defaults(Some(&packages), &connections, "claude");
        let hydrated_settings = &hydrated.profiles["claude"].settings;
        assert_eq!(
            hydrated_settings.get("base_url").map(String::as_str),
            Some("https://api.anthropic.com/v1")
        );
        assert_eq!(
            hydrated_settings.get("model").map(String::as_str),
            Some("claude-sonnet-4-6")
        );
    }
}
