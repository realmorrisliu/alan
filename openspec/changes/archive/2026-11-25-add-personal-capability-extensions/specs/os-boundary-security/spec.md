# Spec Delta

## ADDED Requirements

### Requirement: Confined tool packages receive Host Store secrets without ambient Keychain access
Enabled `tool` subprocesses SHALL run inside OS confinement with outbound network restricted to their manifest `network_domains` and named secrets injected exclusively by Host adapters from Host Store (`SecretStore`) as short-lived environment variables or ephemeral `0600` ADC files, without ambient OS Keychain access.

#### Scenario: Confined gemini-api and colab-compute run without OS Keychain access
- **WHEN** an operator enables `gemini-api` or `colab-compute` with `--allow-network` and invokes a command
- **THEN** Host adapters project only the declared `GEMINI_API_KEY` env var or scoped `GOOGLE_APPLICATION_CREDENTIALS` file from Host Store into the confined subprocess
- **AND** outbound network egress is limited to the manifest's declared `network_domains` while ambient OS Keychain reads remain blocked

#### Scenario: Missing credentials or high-cost remote actions fail safely or confirm first
- **WHEN** a required `credential_id` is absent in Host Store or a user requests paid GPU provisioning via `colab-compute`
- **THEN** missing credentials fail deterministically with an explicit setup hint without leaking secret values
- **AND** Companion Skills require `colab usage` inspection and explicit user confirmation before provisioning paid GPU runtimes
