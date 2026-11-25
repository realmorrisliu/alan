# Gemini API Tool Package (`gemini-api`)

Thin Quartermaster (`q`) `tool` package wrapping `gemini-api-cli` (`gemini-api`)
over injection-free `execve` `argv` forwarding (`arguments = "argv"`), plus a
bundled Companion Skill under `skills/gemini-api/SKILL.md`.

## Zero Ambient Authority & Credential Isolation

Installing `gemini-api` via `alan q install` records the immutable package
revision in Package Service and attaches its read-only Companion Skill
(`skills_only`) unless `--catalog-only` is passed:

- **No OS process** is started at install time.
- **No tool execution or network egress** is enabled until explicit `alan q enable gemini-api --allow-network` or `--package gemini-api` activation.
- **No OS Keychain access or raw credentials in manifests**: the tool declares `required_secrets = [{ env = "GEMINI_API_KEY", credential_id = "gemini-api-key" }]`, which Host Store (`SecretStore`) injects exclusively into the confined subprocess environment.
- **Allowlisted egress only**: `network_domains = ["generativelanguage.googleapis.com"]`.

## Host Setup & Activation

```bash
# 1. Install gemini-api-cli on the host machine if not already present
curl -fsSL https://raw.githubusercontent.com/google-gemini/gemini-api-cli/main/scripts/install.sh | bash

# 2. Store your Gemini API key in Alan Host Store (never echoed to stdout/logs)
alan q set-secret gemini-api-key

# 3. Install package (attaches read-only Companion Skill only)
alan q install crates/service-manager/packages/gemini-api

# 4. Activate runtime tool or run for a single ephemeral invocation
alan q enable gemini-api --allow-network
alan --ephemeral --package gemini-api -p "Preview a dry-run gemini-api generate call"

# 5. Revoke runtime activation while optionally keeping Companion Skill
alan q disable gemini-api --keep-skills
```

## Non-Goals

- Does **not** replace Alan's first-party Vertex / Gemini LLM provider (`crates/llm` / `alan connection add google_gemini_generate_content`).
- Does **not** integrate `gemini-api environments connect/poll/serve` self-hosted workers.
