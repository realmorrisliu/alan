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

#### Scenario: Legacy skill contract is opened
- **WHEN** `docs/spec/skill_system_contract.md` is reached during migration
- **THEN** the page points to this OpenSpec capability and does not restate the
  full legacy contract

### Requirement: Skill vocabulary is stable
alan SHALL use stable skill-system vocabulary consistently across runtime,
daemon, CLI, documentation, and authoring tooling.

Stable terms:

- **Skill package**: one directory-backed portable skill plus optional
  alan-native sidecars, resources, and package-local launch targets.
- **First-party skill package**: a skill package shipped by alan from a built-in
  source. It follows the same package contract as externally discovered
  packages.
- **Portable skill**: the shared public contract centered on `SKILL.md`.
- **alan sidecar**: optional `skill.yaml` / `package.yaml` metadata that extends
  runtime behavior without changing `SKILL.md`.
- **Host tool**: a runtime capability registered through alan's tool system and
  exposed uniformly to the model.
- **Package-local executable tool**: a deterministic executable shipped under
  `bin/` inside one skill package and bound relative to that package instead of
  registered as a host-global tool.
- **Package-local helper**: deterministic executable or support logic shipped
  inside a skill package, usually under `scripts/` as glue, wrappers, or
  compatibility launchers.
- **Package-local launch target**: an alan-native export under `agents/<name>/`
  that may be launched for delegated execution.
- **Implicit invocation**: a skill is listed in the prompt catalog so the model
  may decide to use it on demand.
- **Host-level force-select**: a host/runtime UX such as direct skill-name
  mention or `$skill-id` that asks the host to activate a skill directly.
- **Active skill**: a skill force-selected for the current turn and rendered as
  active runtime context.
- **Parent runtime**: the runtime currently handling the user turn and, when
  supported, exposing `invoke_delegated_skill`.
- **Launch-root runtime**: a runtime started from a package-local launch target.
  Launch-root runtimes intentionally keep nested delegated execution disabled in
  V1.
- **Delegated skill**: a skill that resolves to a package-local launch target
  instead of parent-runtime inline instructions.

#### Scenario: Documentation names a skill-system concept
- **WHEN** docs, CLI output, daemon responses, or prompt assembly describe one
  of these concepts
- **THEN** they use the stable term and preserve the boundary defined by this
  capability

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
  `references/`, and `assets/`, under the channel-selected global public skill
  source, `<workspace>/.agents/skills/`, and alan `AgentRoot` `skills/`
  directories. The stable global public skill source is `~/.agents/skills/`;
  the dev global public skill source is `~/.agents-dev/skills/`.
- Tier 2 compatibility metadata may consume public metadata such as
  `agents/openai.yaml` for UI-facing metadata or policy hints like
  `allow_implicit_invocation`; unknown fields remain fail-open.
- Tier 3 authoring/eval companion assets preserve and ignore-by-default
  auxiliary assets such as `evals/evals.json`, fixtures, `agents/*.md`,
  package-local helper binaries, validator scripts, grader prompts, and review
  viewers.

#### Scenario: Public skill directory is installed
- **WHEN** a portable public skill package is installed under a supported skill
  source
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
alan SHALL discover skill packages from built-in first-party packages,
`AgentRoot` `skills/` directories, and public `.agents/skills/` directories
without making discovery itself imply runtime exposure.

Rules:

- Every discovered package enters the resolved capability view.
- Packages remain visible to catalog tooling even when their exported skills are
  disabled or unavailable.
- Built-in first-party packages are not a separate package kind; `builtin` is a
  discovery source and precedence tier, not a different runtime contract.

#### Scenario: Built-in package is discovered
- **WHEN** alan discovers a first-party built-in skill package
- **THEN** it follows the ordinary directory-backed skill package contract
- **AND** the built-in source does not by itself imply enablement or implicit
  prompt listing

### Requirement: Skill exposure is resolved before prompt rendering
alan SHALL resolve skill availability, overrides, built-in package sources, and
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
- Later `AgentRoot` overlays override earlier values field-by-field for the
  same skill without discarding unrelated overrides for other skills.
- Package-level overrides are not part of the stable contract.

#### Scenario: Overlay override is resolved
- **WHEN** multiple resolved `AgentRoot` layers define skill overrides
- **THEN** alan merges them by runtime `skill` id and field
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
- When delegated work targets a different local workspace, catalog guidance may
  include explicit launch-scope inputs such as `workspace_root` and optional
  nested `cwd`.
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
- Delegated launch may carry explicit workspace-binding inputs such as
  `workspace_root` and optional nested `cwd` when delegated work should run in a
  different local workspace than the parent runtime.
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

### Requirement: Management surfaces expose skill-level state
alan SHALL expose skill-system management through local-first CLI and daemon
surfaces while preserving skill-level `enabled` and
`allow_implicit_invocation`.

Management surfaces:

- `alan skills list`
- `alan skills packages`
- `alan skills init`
- `alan skills validate`
- `alan skills eval`
- `GET /api/v1/skills/catalog`
- `GET /api/v1/skills/changed?after=<cursor>`
- `POST /api/v1/skills/overrides`

Rules:

- Daemon skill APIs use the default workspace or a registered workspace alias /
  short id, not arbitrary filesystem paths.
- Override writes persist through the highest-precedence writable `AgentRoot`.
- Override writes reject unknown `skill_id` values; callers use a runtime skill
  id from the current catalog.
- Change detection is cursor-based so clients do not need full catalog reloads
  on every poll.
- Catalog and daemon responses expose skill-level `enabled` and
  `allow_implicit_invocation`.
- Package snapshots do not expose mount modes because package-level exposure is
  not part of the stable contract.

#### Scenario: Skill catalog is queried
- **WHEN** a CLI or daemon client requests the skill catalog
- **THEN** alan returns skill-level exposure and availability state
- **AND** package snapshots omit removed mount-mode concepts

### Requirement: First-party packages are ordinary skill packages
alan SHALL ship first-party packages as ordinary directory-backed skill
packages, not privileged always-active instruction blobs.

Rules:

- Built-in distribution is a packaging detail, not a different contract.
- First-party packages may carry the same `scripts/`, `references/`, `assets/`,
  `bin/`, `evals/`, `eval-viewer/`, `agents/`, and compatibility metadata as an
  external package.
- First-party package source does not imply implicit listing or explicit
  enablement overrides.
- Any behavior that alan needs unconditionally lives in the base prompt, tool
  descriptions, or dedicated runtime policy rather than in always-active skills.

#### Scenario: First-party package is rendered in prompt context
- **WHEN** a built-in skill package is enabled, available, and implicitly
  invokable
- **THEN** it appears through the same prompt catalog contract as external
  packages
- **AND** unconditional runtime behavior is not hidden in first-party skill
  instructions

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
- CLI, daemon, and catalog surfaces expose `enabled` and
  `allow_implicit_invocation`, not mount modes.
- Tests asserting mount-mode behavior are deleted or rewritten.
- No legacy compatibility shim is required by this contract.

#### Scenario: Legacy mount-mode field is encountered
- **WHEN** old docs, fixtures, or code refer to mount modes or always-active
  activation as current behavior
- **THEN** the reference is removed, rewritten, or treated as outside this
  stable contract

### Requirement: Validation covers resolution, prompts, availability, and docs
alan SHALL validate the skill-system cutover with coverage for resolution,
prompt assembly, availability, CLI/daemon/catalog behavior, and documentation.

Validation matrix:

- Skill override merge resolves `enabled` and `allow_implicit_invocation`
  independently across overlays.
- Disabled skills are absent from explicit and implicit runtime surfaces.
- `allow_implicit_invocation = false` skills are catalog-hidden but still
  force-selectable by direct `skill_id`.
- Inline implicit skills appear in the catalog with canonical `SKILL.md` paths
  and are not auto-injected.
- Delegated implicit skills appear in the catalog with `skill_id`, `target`,
  and direct `invoke_delegated_skill` guidance.
- Direct `skill_id` mention still renders active skill context for inline
  skills.
- Direct `skill_id` mention still renders delegated stubs for delegated skills.
- No skill-authored alias / keyword / pattern / always-active activation
  remains.
- Disabled skills render as not found.
- Enabled but unavailable skills render unavailable diagnostics on direct
  `skill_id` mention.
- Unavailable skills do not appear in the implicit catalog.
- Catalog snapshots expose `enabled` and `allow_implicit_invocation`.
- Package snapshots no longer expose mount modes.
- Daemon override writes target `skill_overrides`.
- CLI output no longer prints mount-mode labels.
- Config examples use `skill_overrides`.
- Architecture and AGENTS summaries do not mention mount modes or always-active
  activation.
- Built-in fixtures and snapshots assume no always-active defaults.

#### Scenario: Skill-system cutover is validated
- **WHEN** alan changes skill resolution, prompt rendering, availability, CLI
  output, daemon APIs, catalog snapshots, or docs
- **THEN** validation covers the relevant rows of this matrix
- **AND** reports actual checks, skipped checks, and remaining risk separately

### Requirement: Non-goals remain outside the stable contract
alan SHALL keep explicitly removed or deferred skill-system concepts outside
the stable contract unless a later OpenSpec change adds them.

Explicit non-goals:

- `package.toml` manifests
- multi-skill filesystem packages
- structured trigger metadata
- runtime mount policies
- `viewers/` as a capability export or runtime contract
- `runtime.ui` as stable behavior
- nested delegated execution in V1

#### Scenario: Deferred skill-system concept is proposed
- **WHEN** a change proposes one of the explicit non-goals as stable behavior
- **THEN** the change updates this capability through OpenSpec before relying
  on the behavior in implementation or documentation

### Requirement: Global public skill sources are channel-scoped
Alan SHALL resolve and mutate global public skill install sources according to
the active install channel. The stable channel SHALL keep `~/.agents/skills/`;
the dev channel SHALL use a separate global public skill source.

#### Scenario: Stable global public skills are discovered
- **WHEN** stable-channel Alan discovers global public skill packages
- **THEN** it discovers packages under `~/.agents/skills/`
- **AND** existing stable public skill compatibility remains unchanged

#### Scenario: Dev global public skills are discovered
- **WHEN** dev-channel Alan discovers global public skill packages
- **THEN** it discovers packages under `~/.agents-dev/skills/`
- **AND** it does not discover `~/.agents/skills/` as an implicit fallback

#### Scenario: Dev installs a global skill
- **WHEN** a dev-channel command installs or updates a global public skill package
- **THEN** it writes under `~/.agents-dev/skills/`
- **AND** it does not create, modify, or remove packages under `~/.agents/skills/`

### Requirement: Workspace skill sources remain workspace-authored
Install-channel isolation SHALL NOT change the portable workspace public skill
source path.

#### Scenario: Workspace public skills are discovered
- **WHEN** either channel discovers portable public skill packages in a workspace
- **THEN** `<workspace>/.agents/skills/` remains the workspace public skill source
- **AND** packages discovered there are treated as workspace-authored content rather than channel-private global data

#### Scenario: Workspace skill writes generated output
- **WHEN** a workspace skill run writes generated runtime output, evaluation cache, or logs through Alan-managed paths
- **THEN** those generated outputs are channel-scoped
- **AND** the source skill package under `<workspace>/.agents/skills/` remains unchanged unless the user explicitly edits or installs into that workspace source
