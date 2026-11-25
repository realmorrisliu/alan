# Spec Delta

## ADDED Requirements

### Requirement: Quartermaster imports multi-skill community bundles cleanly
Quartermaster `q install` SHALL materialize explicit namespace snapshots of third-party multi-skill bundles by accepting `commands/*.md` alongside `skills/`, preserving shared read-only asset folders (`playbooks/`, `references/`), ignoring unknown frontmatter keys safely, and never scanning ambient Host directories or accepting remote URLs.

#### Scenario: Installing a multi-skill community repository snapshot
- **WHEN** an operator authorizes a namespace mount and runs `alan q install /mnt/import/pstack` on a bundle containing `skills/*/SKILL.md`, `commands/*.md`, shared `playbooks/`, and custom frontmatter fields
- **THEN** Package Service materializes all valid skills and shared read-only assets into `/lib/pkg/<pkg>/`
- **AND** unknown frontmatter keys do not fail installation or grant extra runtime authority

#### Scenario: Ambient host skill directories and remote URLs remain rejected
- **WHEN** `q install` receives a remote URL or an unmounted ambient Host directory contains `SKILL.md`
- **THEN** Package Service rejects the remote URL and ignores the unmounted Host directory

### Requirement: Tool packages support thin argv forwarding with network domains and secret references
Tool package manifests SHALL allow thin `argv` forwarding (`arguments = "argv"`) via direct process execution without a shell, reuse `network_domains` validation when `allow_network = true`, and declare named `required_secrets` or `required_adc` references while rejecting raw credentials in manifests.

#### Scenario: Installing a tool package with network domains and secret refs grants no authority
- **WHEN** an operator runs `alan q install` on `gemini-api` or `colab-compute`
- **THEN** only read-only Companion Skills attach in `SkillsOnly` mode
- **AND** tool execution and network egress remain disabled until explicit `alan q enable` or `--package` activation

#### Scenario: Tool manifest containing a raw secret or empty network domain allowlist is rejected
- **WHEN** a tool manifest sets `allow_network = true` with an empty `network_domains` list or embeds raw credential strings instead of a `credential_id` reference
- **THEN** manifest validation fails closed before materialization
