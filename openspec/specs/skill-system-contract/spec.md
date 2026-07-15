# skill-system-contract Specification

## Purpose
Defines alan's durable skill-system contract: portable skill packages, alan-native
sidecars, discovery and exposure rules, prompt rendering, package-local helpers,
delegated execution, management surfaces, and the removed legacy mount-mode
model.
## Requirements
### Requirement: Skill system contracts live in OpenSpec
alan SHALL specify skill package layout, `SKILL.md` semantics, compatibility
metadata, discovery, exposure, override behavior, prompt rendering, helper
assets, delegated launch targets, and management surfaces in OpenSpec.

#### Scenario: Skill package behavior changes
- **WHEN** a change modifies package discovery, frontmatter parsing,
  compatibility metadata, resource directories, built-in package distribution,
  skill availability, skill prompt rendering, or skill execution
- **THEN** the OpenSpec delta updates this capability or another named skill
  capability
- **AND** `docs/skill_authoring.md` and `docs/skills_and_tools.md` remain
  implementation/operator guides instead of contract sources

### Requirement: Skill packages are directory-backed capabilities
alan SHALL treat a skill package as a directory with a root `SKILL.md` and
optional sidecars, resources, helper executables, evaluations, and package-local
agent launch targets.

The stable package layout is:

```text
skill-name/
|-- SKILL.md
|-- skill.yaml
|-- package.yaml
|-- bin/
|-- scripts/
|-- references/
|-- assets/
|-- evals/
|-- eval-viewer/
`-- agents/
```

Rules:

- A directory-backed package currently exports exactly one portable skill: the
  `SKILL.md` in the package root.
- `bin/`, `scripts/`, `references/`, and `assets/` are stable bundled resource
  directories.
- `evals/` and `eval-viewer/` are optional authoring/evaluation surfaces for
  explicit tooling. Runtime discovery ignores them by default.
- `agents/` is an alan-native extension directory for package-local launch
  targets and may also contain non-runtime authoring assets.
- Unknown additional files or directories are ignored by runtime discovery.
- The runtime `skill_id` is a normalized lower-case hyphenated slug derived
  from the package directory name. Separator variants such as `.`, `_`, and
  whitespace canonicalize to `-`.
- Multi-skill filesystem packages are not part of the stable public contract.

#### Scenario: Portable skill is discovered
- **WHEN** alan discovers a directory containing `SKILL.md`
- **THEN** it can adapt that directory as a skill package without requiring an
  alan-specific manifest for the portable baseline
- **AND** unknown extra files do not make discovery fail

#### Scenario: alan-native assets are present
- **WHEN** a package includes alan-native sidecars such as `skill.yaml`,
  `package.yaml`, `agents/`, `bin/`, `scripts/`, `references/`, `assets/`,
  `evals/`, or `eval-viewer/`
- **THEN** alan exposes only the supported runtime and authoring surfaces
  defined by OpenSpec
- **AND** shipping a helper file inside a package does not make it a host-global
  runtime tool

### Requirement: Compatibility tiers are explicit
alan SHALL preserve portable public skill compatibility while treating
alan-native extensions and authoring/evaluation assets as separate tiers.

Compatibility tiers:

- Tier 1 portable runtime compatibility discovers and runs public skill
  directories centered on `SKILL.md`, including optional `bin/`, `scripts/`,
  `references/`, and `assets/`, only when the package is installed in Alan OS
  or supplied by an explicit Skill or Agent Definition descriptor. Host
  directories are not implicit sources.
- Tier 2 compatibility metadata may consume public metadata such as
  `agents/openai.yaml` for UI-facing metadata or policy hints like
  `allow_implicit_invocation`; unknown fields remain fail-open.
- Tier 3 authoring/eval companion assets preserve and ignore-by-default
  auxiliary assets such as `evals/evals.json`, fixtures, `agents/*.md`,
  package-local helper binaries, validator scripts, grader prompts, and review
  viewers.

#### Scenario: Public skill directory is installed
- **WHEN** a portable public skill package is installed in Alan OS or supplied
  by descriptor
- **THEN** alan discovers and can run the package without alan-specific
  manifests

#### Scenario: Compatibility metadata contains unknown fields
- **WHEN** a package includes compatibility metadata or authoring/eval assets
  that alan does not understand
- **THEN** runtime discovery tolerates the fields or assets without treating
  them as required activation inputs

### Requirement: SKILL.md remains the portable selection contract
alan SHALL treat `SKILL.md` as the portable skill-authored selection and
instruction contract.

Required frontmatter:

- `name`
- `description`

Stable optional frontmatter:

- `metadata.short-description`
- `metadata.tags`
- `capabilities.required_tools`
- `capabilities.disclosure.level2`
- `capabilities.disclosure.level3.references`
- `capabilities.disclosure.level3.scripts`
- `capabilities.disclosure.level3.assets`
- `compatibility.min_version`
- `compatibility.dependencies`
- `compatibility.requirements`

Stable semantics:

- `name` and `description` are the only portable skill-authored fields that
  determine when a skill should be selected.
- `compatibility.min_version` is a hard availability gate.
- `compatibility.dependencies` is a typed availability gate. Stable dependency
  kinds are `env_var`, `tool`, and `runtime_capability`.
- `compatibility.requirements` is advisory remediation text only. It is not a
  typed availability gate.

#### Scenario: Skill frontmatter is parsed
- **WHEN** alan reads a package root `SKILL.md`
- **THEN** `name` and `description` remain sufficient for basic public skill
  interoperability
- **AND** optional compatibility gates are applied according to this contract

#### Scenario: Skill author adds trigger metadata
- **WHEN** `SKILL.md` contains aliases, keyword triggers, regex triggers,
  semantic triggers, negative keywords, or always-active activation hints
- **THEN** alan does not treat those fields as part of the stable portable
  selection contract

### Requirement: alan sidecars extend runtime behavior
alan SHALL use alan-native sidecars to extend runtime behavior without changing
the public `SKILL.md` portability contract.

Stable `skill.yaml` keys:

- `runtime.execution.mode = inline | delegate`
- `runtime.execution.target`
- `runtime.allow_implicit_invocation`
- `runtime.permission_hints`

`package.yaml` may provide `skill_defaults.runtime` with the same stable keys
as `skill.yaml`. Package defaults apply before the skill-local sidecar.

alan may also consume `agents/openai.yaml` `policy.allow_implicit_invocation`.

Implicit-invocation default precedence:

1. `skill.yaml` `runtime.allow_implicit_invocation`
2. `package.yaml` `skill_defaults.runtime.allow_implicit_invocation`
3. `agents/openai.yaml` `policy.allow_implicit_invocation`
4. default `true`

`runtime.ui` is tolerated input but is not part of the stable contract and is
not preserved in resolved runtime metadata.

#### Scenario: Sidecars define runtime behavior
- **WHEN** a skill package includes `skill.yaml`, `package.yaml`, or tolerated
  compatibility metadata
- **THEN** alan resolves runtime behavior using the precedence defined by this
  capability
- **AND** `SKILL.md` remains the portable selection and instruction source

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
- Installed package Skills are read through their Process namespace/aP
  descriptors; Package Store Host paths are not a discovery input.

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

### Requirement: Skill exposure is resolved before prompt rendering
alan SHALL resolve skill availability, overrides, installed package references, and
package-local launch targets before rendering the active prompt catalog.

Stable runtime exposure fields are:

- `enabled`
- `allow_implicit_invocation`

Semantics:

- `enabled = false`: the skill is disabled for this runtime. It does not appear
  in the prompt catalog and does not activate through explicit mention.
- `enabled = true && allow_implicit_invocation = true`: the skill may appear in
  the prompt catalog when it is also available.
- `enabled = true && allow_implicit_invocation = false`: the skill is hidden
  from the prompt catalog but explicit activation still works when the skill is
  available.

Rules:

- `enabled` defaults to `true`.
- `allow_implicit_invocation` defaults according to sidecar / compatibility
  metadata, then falls back to `true`.
- Package identity stays relevant for resources, package-local launch targets,
  and provenance, but package-level mount policy is removed from the stable
  runtime contract.
- Built-in source does not imply `enabled` or implicit listing by itself.

#### Scenario: Skill override is applied
- **WHEN** `enabled` or `allow_implicit_invocation` is set through an
  `agent.toml` skill override
- **THEN** alan applies the resolved skill-level exposure state consistently in
  prompt assembly, `alan skills` inspection, and runtime availability checks

#### Scenario: Disabled skill is requested
- **WHEN** a disabled skill is directly mentioned or otherwise force-selected
- **THEN** alan treats it like a not-found skill at runtime

### Requirement: Operator overrides are skill-level fields
alan SHALL resolve operator overrides through `agent.toml` `skill_overrides`
keyed by runtime `skill` id.

Example:

```toml
[[skill_overrides]]
skill = "repo-review"
enabled = false

[[skill_overrides]]
skill = "deployment"
allow_implicit_invocation = false
```

Rules:

- Overrides are keyed by runtime `skill` id.
- `enabled` and `allow_implicit_invocation` are independent override fields.
- The explicitly supplied Agent Definition owns its override set; Alan does not
  merge Host-directory overlay chains.
- Package-level overrides are not part of the stable contract.

#### Scenario: Definition-local override is resolved
- **WHEN** the explicit Agent Definition declares skill overrides
- **THEN** alan applies them by runtime `skill` id and field
- **AND** package-level mount or exposure policy is not used

### Requirement: Selection is description-driven
alan SHALL keep portable skill selection narrow and description-driven.

Portable selection rules:

- `name` and `description` are the only skill-authored fields that determine
  whether a portable skill should be selected.
- `SKILL.md` body content loads only after the host or model has selected the
  skill.
- There is no structured trigger schema, alias list, keyword list, regex list,
  semantic trigger list, or always-active contract in the stable skill format.

Host-level force-select rules:

- Only `enabled` skills may be force-selected.
- Force-select is keyed by the runtime `skill` id only; portable skills do not
  declare extra aliases.
- Force-select does not depend on `allow_implicit_invocation`.
- Unavailable skills stay unavailable even when force-selected; runtime surfaces
  the reason instead of silently injecting them.
- Disabled skills behave like not-found skills at runtime.

#### Scenario: Model selects an implicit skill
- **WHEN** a skill is available, enabled, and listed for implicit invocation
- **THEN** selection is based on the portable `name` and `description`
- **AND** the body of `SKILL.md` is loaded only after selection

#### Scenario: Host force-selects a skill
- **WHEN** a host-level direct skill reference or `$skill-id` asks alan to
  activate a skill
- **THEN** alan resolves the runtime `skill_id`
- **AND** availability and `enabled` state still gate activation

### Requirement: Prompt catalog uses progressive disclosure
alan SHALL build the system prompt skills catalog from skills that are
`enabled = true`, `allow_implicit_invocation = true`, and available in the
current runtime.

Rules:

- The catalog includes runtime `skill_id`, portable `name`, portable
  `description`, and the canonical `SKILL.md` path for inline skills.
- The catalog tells the model to open `SKILL.md` only when the task requires
  that skill.
- The catalog makes `name` and `description` the portable selection surface.
- The catalog uses progressive disclosure language: read the skill file first,
  then load only referenced resources as needed.
- Inline implicit skills are not injected into the prompt by default.
- Delegated implicit skills include enough metadata for direct tool use:
  `skill_id`, delegated `target`, and the instruction to call
  `invoke_delegated_skill`.
- When delegated work needs different authority, catalog guidance may include
  explicit descriptors, inherited mounts, and a namespace `cwd`.
- Core behavior that must always be present belongs in the base prompt or tool
  descriptions, not in always-active skills.

#### Scenario: Prompt catalog is rendered
- **WHEN** alan assembles runtime prompt context
- **THEN** only available, enabled, implicitly invokable skills appear in the
  implicit skills catalog
- **AND** inline implicit skills are represented as catalog entries rather than
  injected instruction bodies

### Requirement: Active skill rendering is a force-select surface
alan SHALL render active-skill prompt sections only as a runtime convenience
surface for force-selected skills, not as the primary implicit discovery
mechanism.

Rules:

- Inline force-selected skills render runtime context plus the disclosed
  instruction body and referenced resources.
- Delegated force-selected skills render runtime context plus a
  delegated-capability stub.
- Active-skill runtime context exposes `enabled`,
  `allow_implicit_invocation`, canonical path metadata, availability, and
  execution state.
- Active-skill runtime context does not mention removed mount-mode concepts.

#### Scenario: Inline skill is force-selected
- **WHEN** an enabled, available inline skill is force-selected
- **THEN** alan renders the active-skill context and disclosed instructions for
  that skill

#### Scenario: Delegated skill is force-selected
- **WHEN** an enabled, available delegated skill is force-selected
- **THEN** alan renders a delegated-capability stub rather than injecting an
  inline instruction body

### Requirement: Availability gates are explicit
alan SHALL determine skill availability through explicit hard gates and advisory
metadata.

Hard availability gates:

- `capabilities.required_tools`
- `compatibility.min_version`
- `compatibility.dependencies`
- resolved delegated execution state

Advisory only:

- `compatibility.requirements`
- unknown compatibility hints from tolerated metadata

Rules:

- If a skill resolves to delegated execution ambiguously, alan marks it
  unavailable rather than silently guessing or falling back inline.
- `capabilities.required_tools` canonicalizes into the same dependency gate as
  `compatibility.dependencies`.

#### Scenario: Required capability is missing
- **WHEN** a hard availability gate is not satisfied
- **THEN** the skill is unavailable
- **AND** direct activation surfaces the reason instead of injecting the skill

#### Scenario: Advisory metadata is present
- **WHEN** a package includes advisory compatibility requirements or unknown
  compatibility hints
- **THEN** alan may report them as remediation context
- **AND** it does not treat them as typed availability gates

### Requirement: Package-local helpers do not become host tools
alan SHALL separate host tools, package-local executable tools, package-local
helpers, and reusable skill tooling.

Rules:

- Runtime tools are host capabilities registered through alan's tool system and
  exposed uniformly to the model.
- Skill packages do not create new host-global runtime tool definitions merely
  by shipping files in the package tree.
- Skill packages may ship package-local executable tools under `bin/`.
- Package-local executable tools are package-scoped rather than host-global.
  When alan exposes them to the model, it binds them relative to the canonical
  package root and keeps them available only to the owning skill context or
  launch-root runtime.
- Source trees and packaged artifacts preserve package-relative executable
  layout. Packaged binaries remain under package-local `bin/` so skill
  instructions do not depend on machine-specific install paths.
- `bin/` is the preferred home for deterministic package-private executables
  that are part of the skill product and may be invoked repeatedly by that
  skill.
- `scripts/` remains the place for shell/Python glue, compatibility launchers,
  and thin wrappers around external ecosystems or around `bin/` entries.
- If a runtime does not yet expose package-local executable tools directly,
  packages may invoke `bin/` entries through existing host tools such as `bash`
  as a compatibility fallback.
- New first-party authoring and evaluation tooling should prefer typed Rust CLI
  surfaces or dedicated Rust binaries over shell, Python, or TypeScript scripts
  whenever feasible.
- If a skill depends on an external executable that is not shipped inside the
  package, authors declare it through `capabilities.required_tools` or
  `compatibility.dependencies` with dependency kind `tool`.
- Reusable skill tooling may be shared across multiple skill packages, but it
  remains operator-side tooling unless alan explicitly promotes it into the
  runtime tool surface.

#### Scenario: Package ships a helper executable
- **WHEN** a skill package includes files under `bin/` or `scripts/`
- **THEN** alan treats them as package-local resources unless a separate runtime
  tool registration promotes a capability into the host tool surface

#### Scenario: Runtime lacks direct bin exposure
- **WHEN** a skill package needs a package-local `bin/` executable and the
  runtime does not expose package-local executable tools directly
- **THEN** the package may invoke the executable through an existing host tool
  such as `bash` as a compatibility fallback

### Requirement: Resources use progressive disclosure levels
alan SHALL support progressive disclosure across metadata, primary instruction
body, and bundled resources.

Disclosure levels:

- Level 1 metadata: `name`, `description`, `short-description`, tags.
- Level 2 primary instruction body: `SKILL.md` body or
  `disclosure.level2`.
- Level 3 bundled resources: `references/`, `bin/`, `scripts/`, `assets/`.

Rules:

- `SKILL.md` stays concise and procedural.
- Detailed schemas, examples, and domain reference material move into
  `references/`.
- Package-local executable tools that travel with the skill live in `bin/`.
- Package-private deterministic helpers that remain script-based live in
  `scripts/`.
- Templates and output resources live in `assets/`.
- Relative resource paths resolve against the canonical package resource root.
- Authoring keeps references shallow.

#### Scenario: Skill references additional resources
- **WHEN** selected skill instructions point to package-local references,
  scripts, binaries, or assets
- **THEN** alan resolves those paths relative to the canonical package resource
  root
- **AND** the model loads only the resources needed for the current task

### Requirement: Skill execution resolves to inline or delegate
alan SHALL resolve each discovered skill to exactly one execution mode:
`inline` or `delegate(target=package-launch-target)`.

Default inference:

- no launch targets -> `inline`
- same-name skill and launch-target export -> `delegate`
- exactly one skill and one launch-target export -> `delegate`
- otherwise -> unresolved and unavailable

alan must not guess across ambiguous package shapes.

#### Scenario: Skill execution mode is inferred
- **WHEN** a package omits explicit execution sidecar configuration
- **THEN** alan applies deterministic package-local inference
- **AND** ambiguous package shapes make the skill unavailable rather than
  silently choosing an execution mode

### Requirement: Delegated execution is package-local and bounded
alan SHALL implement delegated execution as an alan-native runtime contract
that launches package-local targets from parent runtimes.

Rules:

- Parent runtimes expose `invoke_delegated_skill`.
- Delegated implicit skills may be invoked directly from the catalog without a
  prior active-skill injection step.
- Delegated launch uses a package-local `SpawnTarget` and a fresh launch-root
  runtime.
- Delegated launch may carry explicit Process Launch Context inputs such as
  descriptors, inherited mounts, and a namespace `cwd`.
- Parent runtime tape records a bounded delegated result rather than replaying
  the launch-root transcript.
- Launch-root rollout remains separately inspectable out of band.

Delegated execution does not implicitly inherit:

- parent tape
- active skills
- plan state
- memory handle
- nested delegated execution

#### Scenario: Delegated child is launched
- **WHEN** a parent runtime invokes a delegated skill
- **THEN** alan launches the package-local target as a fresh launch-root runtime
- **AND** parent tape records only the bounded delegated result
- **AND** the launch-root rollout remains inspectable separately

#### Scenario: Launch-root runtime tries to delegate again
- **WHEN** a V1 launch-root runtime would expose nested delegated execution
- **THEN** nested delegation remains disabled

### Requirement: Delegation fallback is runtime capability fallback only
alan SHALL NOT expose a third author-facing execution mode beyond `inline` and
`delegate`.

Rules:

- When a runtime does not expose delegated invocation support, a delegated skill
  may fall back to inline rendering for that runtime only.
- This is a runtime capability fallback, not stable author-facing execution
  mode.
- Fallback behavior is explicit in prompt/runtime surfaces when it materially
  changes how the skill is used.

#### Scenario: Runtime cannot invoke delegated skills
- **WHEN** a delegated skill is selected in a runtime without delegated
  invocation support
- **THEN** alan may render the skill inline for that runtime
- **AND** it does not record or expose this as a distinct skill-authored
  execution mode

### Requirement: Package-local launch targets stay inside the package
alan SHALL treat entries under `agents/` as package-local launch targets only
when they resolve inside the package tree.

Example layout:

```text
skill-name/
`-- agents/
    `-- reviewer/
        |-- agent.toml
        |-- persona/
        `-- policy.yaml
```

Rules:

- Entries under `agents/` are package-local launch targets.
- Exported roots remain inside the package tree after canonicalization.
- Symlinks that escape the package tree are ignored.
- Compatibility assets such as `agents/grader.md` are not runtime launch
  targets by themselves.

#### Scenario: Launch target root is resolved
- **WHEN** alan discovers a package-local launch target under `agents/`
- **THEN** the canonical target root remains inside the package tree
- **AND** escaped symlink targets are ignored

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

### Requirement: Legacy mount modes are removed
alan SHALL remove the previous mount-mode model from the stable runtime
contract rather than preserving legacy behavior.

Removed from the stable runtime contract:

- `PackageMount`
- `PackageMountMode`
- `package_mounts`
- `always_active`
- `discoverable`
- `explicit_only`
- `internal`
- structured trigger metadata in `SKILL.md`
- keyword / regex / negative-keyword activation
- always-active built-in skills
- package-level runtime exposure policy

Required cutover behavior:

- Config uses `skill_overrides`, not `package_mounts`.
- Runtime prompt assembly only force-selects active skills from host-level
  direct skill references; portable skills do not declare extra trigger
  metadata.
- The system prompt catalog is the only implicit-discovery surface.
- CLI and package-catalog surfaces expose `enabled` and
  `allow_implicit_invocation`, not mount modes.
- Tests asserting mount-mode behavior are deleted or rewritten.
- No legacy compatibility shim is required by this contract.

#### Scenario: Legacy mount-mode field is encountered
- **WHEN** old docs, fixtures, or code refer to mount modes or always-active
  activation as current behavior
- **THEN** the reference is removed, rewritten, or treated as outside this
  stable contract

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

### Requirement: Skill vocabulary is owned by packages and resolution
Alan SHALL use stable Skill, Skill Package, Registry, Resolver, Active Skill, Exposure Mode,
Invocation Mode, and Resource vocabulary across packages, prompt assembly, direct CLI, namespace
projection, documentation, and authoring tools.

#### Scenario: A Skill is described on a current surface
- **WHEN** a package, prompt, CLI response, namespace projection, or document describes a Skill
- **THEN** it uses the canonical skill-level vocabulary
- **AND** it does not depend on a background catalog API

### Requirement: Skill management remains local-first
Alan SHALL expose Skill discovery, validation, enablement, override, authoring, and evaluation
through direct CLI and package operations. Catalog snapshots MAY be derived locally but SHALL NOT
be a second authority or require a persistent product server.

#### Scenario: Operator inspects the Skill catalog
- **WHEN** an operator runs the direct Skill list or package command
- **THEN** Alan resolves installed packages and effective skill-level state locally
- **AND** the result comes from the package, registry, resolver, and override owners

### Requirement: Skill validation covers durable owners
Skill validation SHALL cover package structure, registry resolution, prompt rendering,
availability, delegation, CLI behavior, namespace projection, authoring, and current docs.

#### Scenario: A removed management surface returns
- **WHEN** a Skill test or current document adds a background API as a required management owner
- **THEN** validation rejects the change

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

#### Scenario: Installed package is referenced

- **WHEN** an Agent Process receives an immutable package handle and selected
  Skill descriptors
- **THEN** Agent Runtime Service loads them through namespace/aP
- **AND** capability assembly requires no Package Store Host Mount grant

#### Scenario: Explicit descriptor contains a Skill

- **WHEN** a Process receives a valid Skill or Agent Definition descriptor
- **THEN** alan may resolve the confined Skill roots from that descriptor
- **AND** descriptor resolution does not register the Host directory as a
  package source
