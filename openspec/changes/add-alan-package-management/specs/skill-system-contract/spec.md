## MODIFIED Requirements

### Requirement: Discovery is separate from exposure
Alan SHALL resolve discovered skill packages through Quartermaster — the sole
skill-resolution authority — rather than by independently enumerating skill
source directories. Providers supply packages to Q (pre-installed built-in,
local-source `AgentRoot`/workspace/`.agents/skills`, and distribution), and Q
resolution feeds the capability view without making discovery itself imply
runtime exposure.

Rules:

- Every resolved package enters the resolved capability view.
- Packages remain visible to catalog tooling even when their exported skills are
  disabled or unavailable.
- Built-in first-party packages are not a separate package kind; `builtin` is a
  provider kind and precedence tier, not a different runtime contract.
- The engine does not scan built-in, `AgentRoot`, or `.agents/skills/`
  directories as independent sources; those become Q providers.

#### Scenario: Built-in package is resolved
- **WHEN** Quartermaster resolves a first-party built-in skill package
- **THEN** it follows the ordinary directory-backed skill package contract
- **AND** the built-in provider does not by itself imply enablement or implicit
  prompt listing

#### Scenario: No bypass source
- **WHEN** the engine assembles an agent's capability view
- **THEN** every package in it was resolved through Quartermaster
- **AND** no skill enters the view from a directory enumerated outside Q

### Requirement: First-party packages are ordinary skill packages
Alan SHALL ship first-party packages as ordinary directory-backed skill
packages, reseeded into the Quartermaster store as pre-installed packages, not
as privileged always-active instruction blobs.

Rules:

- Built-in distribution is a packaging detail (a pre-installed provider), not a
  different contract.
- First-party packages may carry the same `scripts/`, `references/`, `assets/`,
  `bin/`, `evals/`, `eval-viewer/`, `agents/`, and compatibility metadata as an
  external package.
- First-party package source does not imply implicit listing or explicit
  enablement overrides.
- Any behavior that Alan needs unconditionally lives in the base prompt, tool
  descriptions, or dedicated runtime policy rather than in always-active skills.

#### Scenario: First-party package is rendered in prompt context
- **WHEN** a built-in skill package is enabled, available, and implicitly
  invokable
- **THEN** it appears through the same prompt catalog contract as external
  packages
- **AND** unconditional runtime behavior is not hidden in first-party skill
  instructions

#### Scenario: Built-ins reseed as pre-installed packages
- **WHEN** Alan starts with an empty store
- **THEN** its first-party packages are present as Q pre-installed packages
- **AND** they are resolved through Q identically to installed packages

### Requirement: Global public skill sources are channel-scoped
Alan SHALL resolve global public skills through Quartermaster local-source
providers scoped to the active install channel. The stable channel SHALL keep
`~/.agents/skills/`; the dev channel SHALL use a separate global public skill
source. Channel scoping is preserved by Q, not by independent enumeration.

#### Scenario: Stable global public skills are resolved
- **WHEN** stable-channel Alan resolves global public skill packages
- **THEN** Q resolves packages registered from `~/.agents/skills/`
- **AND** existing stable public skill compatibility remains unchanged

#### Scenario: Dev global public skills are resolved
- **WHEN** dev-channel Alan resolves global public skill packages
- **THEN** Q resolves packages registered from `~/.agents-dev/skills/`
- **AND** it does not resolve `~/.agents/skills/` as an implicit fallback

#### Scenario: Dev installs a global skill
- **WHEN** a dev-channel command installs or updates a global public skill package
- **THEN** it writes under `~/.agents-dev/skills/`
- **AND** it does not create, modify, or remove packages under `~/.agents/skills/`

### Requirement: Workspace skill sources remain workspace-authored
Install-channel isolation SHALL NOT change the portable workspace public skill
source path. Workspace skills are resolved as Quartermaster local-source
providers registered at the workspace source, not copied into the global store.

#### Scenario: Workspace public skills are resolved
- **WHEN** either channel resolves portable public skill packages in a workspace
- **THEN** `<workspace>/.agents/skills/` remains the workspace public skill source
- **AND** packages resolved there are treated as workspace-authored content rather than channel-private global data

#### Scenario: Workspace skill writes generated output
- **WHEN** a workspace skill run writes generated runtime output, evaluation cache, or logs through Alan-managed paths
- **THEN** those generated outputs are channel-scoped
- **AND** the source skill package under `<workspace>/.agents/skills/` remains unchanged unless the user explicitly edits or installs into that workspace source

## ADDED Requirements

### Requirement: Package provenance is a stable sidecar block
Alan SHALL treat `provenance` as a stable, optional `package.yaml` sidecar
block identifying where the skill package's content came from and, when the
package was materialized by a distribution package, which one owns it. Field
semantics are owned by `package-management-contract`. Ownership resolution does
not depend on this block — Quartermaster's provider registry is authoritative —
so it is optional metadata. Provenance is management metadata: Alan SHALL
exclude it from runtime behavior resolution, exposure decisions, and prompt
rendering, and its absence SHALL NOT affect discovery.

#### Scenario: Provenance block is present
- **WHEN** a skill package's `package.yaml` contains a `provenance` block
- **THEN** discovery, exposure, and prompt rendering behave exactly as they
  would without it
- **AND** management surfaces may display the provenance information

#### Scenario: Provenance block is absent
- **WHEN** a skill package has no `provenance` block
- **THEN** the package remains fully valid under this contract
