# Colab Compute Tool Package (`colab-compute`)

Thin Quartermaster (`q`) `tool` package wrapping `google-colab-cli` (`colab`)
forced to `--auth=adc` over injection-free `execve` `argv` forwarding
(`arguments = "argv"`), plus a bundled Companion Skill under
`skills/colab-compute/SKILL.md`.

## Zero Ambient Authority & Credential Isolation

Installing `colab-compute` via `alan q install` records the immutable package
revision in Package Service and attaches its read-only Companion Skill
(`skills_only`) unless `--catalog-only` is passed:

- **No OS process** is started at install time.
- **No tool execution or network egress** is enabled until explicit `alan q enable colab-compute --allow-network` or `--package colab-compute` activation.
- **No OS Keychain / `keyring` access**: `bin/colab-compute` forces `--auth=adc`, and Host Store projects `credential_id = "colab-adc"` into a short-lived `0600` `GOOGLE_APPLICATION_CREDENTIALS` JSON file inside instance scratch space (unlinked immediately after execution).
- **Allowlisted egress only**: `network_domains = ["colab.research.google.com", "colaboratory.googleapis.com", "colab.pa.googleapis.com", "oauth2.googleapis.com", "www.googleapis.com"]`.

## Host Setup & Scoped ADC

Initialize Host-owned ADC with the verified Colab scopes (`openid`,
`https://www.googleapis.com/auth/cloud-platform`,
`https://www.googleapis.com/auth/userinfo.email`,
`https://www.googleapis.com/auth/colaboratory`, and optional
`https://www.googleapis.com/auth/drive.file`), then store the ADC JSON in Alan
Host Store:

```bash
# 1. Install google-colab-cli on the host machine
pipx install google-colab-cli

# 2. Store scoped ADC JSON in Alan Host Store (never echoed to stdout/logs)
cat ~/.config/gcloud/application_default_credentials.json | alan q set-secret colab-adc

# 3. Install package (attaches read-only Companion Skill only)
alan q install crates/service-manager/packages/colab-compute

# 4. Activate runtime tool or run for a single ephemeral invocation
alan q enable colab-compute --allow-network
alan --ephemeral --package colab-compute -p "Check my Colab compute unit usage"

# 5. Revoke runtime activation while optionally keeping Companion Skill
alan q disable colab-compute --keep-skills
```
