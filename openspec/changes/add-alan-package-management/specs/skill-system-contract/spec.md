# skill-system-contract Delta

## MODIFIED Requirements

### Requirement: Discovery is separate from exposure

alan SHALL discover skill packages only from explicitly referenced installed
Alan OS packages and explicitly supplied Skill or Agent Definition descriptors,
without making discovery itself imply runtime exposure.

Rules:

- Every discovered package enters the resolved capability view.
- Packages remain visible to catalog tooling even when their exported Skills
  are disabled or unavailable.
- First-party packages are ordinary preinstalled Package Service packages;
  `builtin` is a provenance and precedence tier, not a second discovery path.
- Agent Execution Engine does not append compiled-in packages or scan ambient
  Host directories.

#### Scenario: First-party package is discovered

- **WHEN** the Root Agent Process receives an explicit reference to a
  first-party preinstalled package
- **THEN** it follows the ordinary directory-backed Skill package contract
- **AND** first-party provenance does not by itself imply enablement or prompt
  listing

#### Scenario: No discovery bypass exists

- **WHEN** alan assembles an Agent Process capability view
- **THEN** every Skill root came from an explicit installed-package reference
  or an explicit descriptor
- **AND** no workspace, AgentRoot, `.agents`, Alan home, or compiled-in append
  contributes another package

### Requirement: First-party packages are ordinary skill packages

alan SHALL ship first-party packages as ordinary directory-backed Skill
packages seeded into Package Service and explicitly referenced by the Root
Agent Process boot context, not as privileged always-active instruction blobs
or a compiled-in resolution append.

Rules:

- Preinstalled distribution is a packaging detail, not a different runtime
  contract.
- First-party packages may carry the same `scripts/`, `references/`, `assets/`,
  `bin/`, `evals/`, `eval-viewer/`, `agents/`, and compatibility metadata as an
  external package.
- First-party provenance does not imply implicit listing or explicit enablement
  overrides.
- Any behavior alan needs unconditionally lives in the base prompt, Tool
  descriptions, or dedicated runtime policy.

#### Scenario: First-party package is rendered in prompt context

- **WHEN** a referenced first-party Skill is enabled, available, and implicitly
  invokable
- **THEN** it appears through the same prompt catalog contract as an installed
  third-party Skill
- **AND** unconditional runtime behavior is not hidden in first-party Skill
  instructions

#### Scenario: First-party package is not referenced

- **WHEN** an Agent Process is created without a first-party package reference
- **THEN** that package is absent from its capability view
- **AND** Agent Execution Engine does not restore it through a built-in append

### Requirement: Skills enter through installed packages or descriptors

Alan SHALL resolve Skills only from explicit installed Alan OS package
references and explicit Skill/Agent Definition descriptors. It MUST NOT scan
AgentRoot, workspace, `.agents`, Alan home, System Store backing, or other Host
directories as implicit providers. Installing a package SHALL NOT expose it to
a Process that lacks a reference.

#### Scenario: Host directory contains a Skill

- **WHEN** a mounted Host directory contains `SKILL.md`
- **THEN** the Skill remains ordinary file content until explicitly installed
  or passed by descriptor

#### Scenario: Installed package is not referenced

- **WHEN** Package Service has an installed Skill package but an Agent Process
  launch omits its package reference
- **THEN** the Skill is absent from that Agent's resolved capability view

#### Scenario: Explicit descriptor contains a Skill

- **WHEN** a Process receives a valid Skill or Agent Definition descriptor
- **THEN** alan may resolve the confined Skill roots from that descriptor
- **AND** descriptor resolution does not register the Host directory as a
  package source

### Requirement: Non-goals remain outside the stable contract

alan SHALL keep explicitly removed or deferred skill-system concepts outside
the stable contract unless a later OpenSpec change adds them.

Explicit non-goals:

- `package.toml` manifests
- a single directory-backed Skill Package containing multiple Skills; an Alan
  OS distribution package MAY export multiple ordinary single-Skill packages
- structured trigger metadata
- runtime mount policies
- `viewers/` as a capability export or runtime contract
- `runtime.ui` as stable behavior
- nested delegated execution in V1

#### Scenario: Distribution package exports several Skills

- **WHEN** Package Service materializes several Skill roots from one explicitly
  referenced distribution package
- **THEN** each root remains an ordinary single-Skill directory-backed package
- **AND** the distribution package does not become a multi-Skill Skill Package

#### Scenario: Deferred skill-system concept is proposed

- **WHEN** a change proposes one of the explicit non-goals as stable behavior
- **THEN** the change updates this capability through OpenSpec before relying
  on the behavior in implementation or documentation
