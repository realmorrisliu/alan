## MODIFIED Requirements

### Requirement: Discovery is separate from exposure
alan SHALL discover skill packages only from Package Service Skill exports
explicitly selected for the current Process and explicitly supplied Skill or
Agent Definition descriptors, without making discovery itself imply runtime
exposure. It MUST NOT discover every installed package, enumerate Host
directories, or maintain an Agent Execution Engine package registry as an
installed-package source.

Rules:

- Every discovered package enters the resolved capability view.
- Packages remain visible to catalog tooling even when their exported skills are
  disabled or unavailable.
- Built-in first-party packages are not a separate package kind; `first-party`
  is package provenance and a precedence tier, not a different runtime contract.
- Package Service resolves only manifest-declared Skill exports selected for
  the current Process.
- Explicit Skill and Agent Definition descriptors remain valid without
  installing their source as a package.

#### Scenario: First-party package is discovered
- **WHEN** a first-party installed Skill export is selected for a Process
- **THEN** it follows the ordinary directory-backed skill package contract
- **AND** first-party provenance does not by itself imply enablement or implicit
  prompt listing

#### Scenario: Installed package is not selected
- **WHEN** Package Service has an installed Skill package that is not selected
  for the current Process
- **THEN** alan does not add its Skill exports to the resolved capability view

#### Scenario: Host directory contains a Skill
- **WHEN** a mounted Host directory contains `SKILL.md` but is neither installed
  nor passed by descriptor
- **THEN** alan does not discover the Skill
- **AND** no compatibility source scan occurs

### Requirement: First-party packages are ordinary skill packages
alan SHALL ship first-party Skill packages as ordinary Package Service-installed
directory-backed Skill packages, not privileged always-active instruction blobs
or an engine-local built-in source.

Rules:

- First-party distribution is package provenance, not a different contract.
- First-party packages may carry the same `scripts/`, `references/`, `assets/`,
  `bin/`, `evals/`, `eval-viewer/`, `agents/`, and compatibility metadata as an
  external package.
- First-party provenance does not imply implicit listing or explicit enablement
  overrides.
- Any behavior that alan needs unconditionally lives in the base prompt, Tool
  descriptions, or dedicated runtime policy rather than in always-active Skills.

#### Scenario: First-party package is rendered in prompt context
- **WHEN** a first-party installed Skill package is enabled, available, and
  implicitly invokable
- **THEN** it appears through the same prompt catalog contract as third-party
  packages
- **AND** unconditional runtime behavior is not hidden in first-party Skill
  instructions

#### Scenario: First-party package is missing from an empty store
- **WHEN** Package Service starts without a required first-party Skill package
- **THEN** it installs the canonical artifact through the ordinary transaction
  path before reporting ready
- **AND** Agent Execution Engine does not append a compiled-in substitute
