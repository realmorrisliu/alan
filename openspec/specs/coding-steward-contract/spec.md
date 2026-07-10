# coding-steward-contract Specification

## Purpose
Defines the durable coding-steward contract for parent/worker orchestration,
repo-worker delegation, verification honesty, coding governance, and eval
boundaries.

## Requirements
### Requirement: Coding steward contracts live in OpenSpec
alan SHALL specify coding steward orchestration, repo-worker delegation,
verification honesty, behavior-preserving change policy, coding governance, and
coding eval ladders in OpenSpec.

#### Scenario: Coding workflow behavior changes
- **WHEN** a change modifies parent steward responsibilities, repo-scoped child
  worker responsibilities, minimum repo-worker loop behavior, verification
  reporting, delivery summaries, or coding governance boundaries
- **THEN** the requirement is updated in this capability,
  `delegation-capability-alignment`, `delegated-result-handoff`,
  `evidence-retention-and-projection`, or another active OpenSpec owner

#### Scenario: Repo-worker package layout is described
- **WHEN** docs describe the first-party repo-worker package path, child launch
  root, micro-skills, scripts, evals, or harness entrypoints
- **THEN** the docs point at current package implementation guides and OpenSpec
  capability owners instead of a historical `plans/` file

### Requirement: Coding verification remains evidence-based
alan SHALL distinguish actual verification from planned, skipped, mocked, or
environment-blocked verification in coding steward and repo-worker outputs.

#### Scenario: Worker reports completion
- **WHEN** a repo-worker or parent steward delivers a coding result
- **THEN** the response includes the commands or checks actually run, failures
  or environment blockers, and remaining risk
- **AND** it does not imply product behavior was proven by checks that did not
  execute or only exercised mocks

### Requirement: Coding evals validate steward and worker layers separately
alan SHALL keep repo-worker package validation, coding steward orchestration
validation, package-local evals, and external benchmark adapters separated by
what behavior each layer proves.

#### Scenario: Harness coverage is documented
- **WHEN** docs or fixtures describe repo-worker or coding-steward scenarios
- **THEN** they remain executable fixture documentation unless they define
  normative behavior, in which case the behavior is captured in OpenSpec

### Requirement: Coding steward vocabulary is stable
alan SHALL use stable coding-steward vocabulary across OpenSpec, package docs,
runtime launch contracts, harness fixtures, and delivery summaries.

Stable terms:

- **Coding steward**: the parent alan runtime that owns goal intake, workspace
  discovery, routing, approvals, and result integration.
- **Repo worker**: a child runtime launched into a specific repo, directory, or
  project to perform bounded coding work.
- **Coding launch**: a `SpawnSpec` used specifically for repo-scoped coding
  execution.
- **Bound handles**: explicit parent-side state the child receives, such as
  `workspace`, `approval_scope`, `plan`, `conversation_snapshot`,
  `tool_results`, or `memory`.
- **Bounded result integration**: parent consumption of child outcomes through
  terminal status, output summaries, runtime metadata, and explicit structured
  outputs rather than full child-transcript inheritance.

#### Scenario: Coding docs name a role or launch concept
- **WHEN** docs, prompts, specs, harnesses, or runtime metadata describe coding
  steward behavior
- **THEN** they use this vocabulary and preserve the distinction between parent
  steward, repo worker, coding launch, bound handles, and bounded result
  integration

### Requirement: Coding product model uses parent steward plus bounded repo workers
alan SHALL model coding work as a home-root steward that delegates bounded
repo-local execution to fresh child runtimes instead of as one default
single-repo coding shell.

Required product sequence:

1. the parent steward accepts the user's coding-oriented goal
2. the parent discovers or selects the correct workspace
3. the parent launches one or more repo workers through explicit `SpawnSpec`
   contracts
4. each repo worker performs bounded repo-scoped coding work
5. the parent integrates results, handles approvals, and decides whether more
   routing or child launches are needed

Boundary rules:

- Runtime parent-child relations apply to `AgentInstance` launches.
- `AgentRoot` overlays remain definition layering only.
- Coding product behavior must not blur runtime parent-child relations with
  `AgentRoot` overlay semantics.
- External benchmark results measure coding quality but do not define product
  behavior, repo-specific heuristics, task-specific prompts, or benchmark-only
  special cases.
- Coding improvements target reusable behavior such as code understanding,
  change-boundary control, verification discipline, and honest delivery.

#### Scenario: Single-repo bug fix is delegated
- **WHEN** a user asks alan to perform a repo-local bug fix
- **THEN** the parent steward selects the repo and launches a repo worker with
  explicit workspace and approval scope
- **AND** the worker performs the inspect -> plan -> edit -> verify -> deliver
  loop inside that delegated scope

#### Scenario: Cross-repo work is requested
- **WHEN** a coding objective spans multiple repos or projects
- **THEN** the parent steward decomposes the objective into repo-scoped slices,
  launches separate workers where appropriate, and reconciles sequencing,
  approvals, and final delivery across the children

### Requirement: Parent steward responsibilities are distinct from repo-worker responsibilities
alan SHALL keep parent orchestration work separate from repo-local coding
execution.

The parent coding steward owns:

1. broad goal intake and clarification
2. workspace discovery, comparison, and selection
3. task routing across repos, directories, or projects
4. launch-shape decisions for child runtimes
5. approval ownership for risky cross-workspace or external actions
6. result integration, dedupe, and follow-up planning
7. deciding whether the task remains repo-local or has expanded into broader
   orchestration

The child repo worker owns:

1. inspect -> plan -> edit -> verify -> deliver inside the delegated repo or
   directory
2. repo-local side effects within the bound workspace scope
3. maintaining a bounded coding transcript for the delegated task
4. producing a delivery summary with verification and residual-risk status
5. returning control when complete, blocked, or attempting to expand beyond
   delegated scope

The parent steward is not the default place for every repo-local edit loop. The
child worker must not silently broaden workspace, approval, credential, or
external-action scope beyond what the launch contract granted.

#### Scenario: Worker attempts to leave delegated scope
- **WHEN** a repo worker needs to mutate outside the bound workspace or perform
  a credential, publish, deploy, or external action
- **THEN** it returns control or escalates according to governance rather than
  silently expanding its scope

### Requirement: Repo workers follow a minimum coding loop
alan SHALL require repo-scoped coding workers to execute and report a minimum
coding loop.

Minimum loop:

1. receive the delegated coding task
2. plan and decompose the work into actionable steps
3. apply code changes through tools
4. run verification commands when feasible
5. deliver a summary of what changed, what was verified, failures or blockers,
   and residual risk

This loop belongs to the repo worker. The parent steward remains responsible
for broader routing, approval ownership, and result integration.

#### Scenario: Worker cannot verify
- **WHEN** verification cannot run because tools, dependencies, environment, or
  permissions are unavailable
- **THEN** the worker reports the blocker and residual risk explicitly rather
  than implying the behavior passed

### Requirement: Coding workflow control modes preserve coding-loop causality
alan SHALL define `steer`, `follow_up`, and `next_turn` semantics for active
coding workflows.

Control semantics:

- `steer` re-plans the active coding loop quickly and may skip remaining safe
  steps when needed.
- `follow_up` queues additional coding intent for the immediate next cycle.
- `next_turn` queues future coding context without breaking the current turn's
  causality.

#### Scenario: User steers active coding work
- **WHEN** a user submits `steer` while a coding worker is in an active loop
- **THEN** alan treats the input as active-loop steering and may re-plan before
  continuing or stopping safe remaining steps

#### Scenario: User queues future context
- **WHEN** a user submits `next_turn` during a coding workflow
- **THEN** alan preserves it as future context rather than rewriting the current
  turn's causality

### Requirement: Coding launches are fresh and explicit
alan SHALL launch repo-scoped coding workers through fresh child runtimes with
explicit inputs and handle profiles.

Coding launches may target:

1. a resolved on-disk agent root
2. a package-exported child-agent target

Recommended launch inputs:

- `launch.task` describes the delegated coding objective and hard constraints.
- `launch.cwd` points at the repo root or narrower task-local directory where
  commands should execute.
- `launch.workspace_root` points at the repo or project root that defines the
  worker's writable boundary.
- `launch.timeout_secs` defaults to a bounded value for productized coding
  paths unless intentionally omitted.
- `runtime_overrides.model_reasoning_effort` is the reasoning-control field
  for bounded worker reasoning level.
- Removed shortcuts such as `launch.budget_tokens` are rejected rather than
  interpreted as reasoning controls.

#### Scenario: Coding launch is prepared
- **WHEN** the parent steward launches a repo worker
- **THEN** the child starts with a fresh runtime and receives only explicit
  launch inputs and bound handles

#### Scenario: Deprecated reasoning shortcut is supplied
- **WHEN** a coding launch attempts to use `launch.budget_tokens`
- **THEN** alan rejects the launch shape instead of treating token budget as
  reasoning effort

### Requirement: Coding handle profiles are explicit
alan SHALL use named handle profiles for common coding handoff shapes and SHALL
avoid ambient parent-state inheritance.

Recommended profiles:

- **Minimal repo-local worker**: `workspace`, `approval_scope`.
- **Standard coding handoff**: `workspace`, `approval_scope`, `plan`,
  `conversation_snapshot`.
- **Verification or debugging follow-up**: standard handoff plus
  `tool_results` when recent tool evidence is expensive or lossy to recompute.
- **Durable project continuity**: standard handoff plus `memory` only when
  shared durable workspace memory materially improves the delegated task.

Coding launches do not implicitly inherit:

1. the full parent tape
2. the parent's full active skill set
3. the parent's dynamic-tool registry
4. the parent's session identity
5. artifact-routing behavior before artifact handles are implemented
6. ambient trust outside the bound workspace

#### Scenario: Standard handoff is selected
- **WHEN** the parent has already clarified task shape and current plan
- **THEN** the standard coding handoff includes `plan` and
  `conversation_snapshot` as explicit handles
- **AND** it still does not inherit the full parent tape

#### Scenario: Durable memory is requested
- **WHEN** the parent decides a repo worker needs durable project continuity
- **THEN** `memory` is bound explicitly and is not treated as the default coding
  handoff

### Requirement: Repo-coding package owns first-party worker implementation
alan SHALL keep the first-party repo worker under the package-native
`crates/agent-engine/skills/repo-coding/` path and SHALL NOT keep duplicate
top-level staging copies as product boundaries.

Target package layout:

```text
crates/agent-engine/skills/repo-coding/
|-- SKILL.md
|-- skill.yaml
|-- references/
|-- evals/
|-- scripts/
`-- agents/
    `-- repo-worker/
        |-- agent.toml
        |-- persona/
        |-- policy.yaml
        |-- skills/
        |   |-- decompose/SKILL.md
        |   |-- edit-verify/SKILL.md
        |   `-- deliver/SKILL.md
        `-- extensions/
            |-- code-index.yaml
            |-- test-analyzer.yaml
            `-- pr-helper.yaml
```

Package roles:

- `repo-coding/SKILL.md` is the parent-facing capability entry for repo-scoped
  coding work.
- `repo-coding/skill.yaml` expresses alan-native delegated execution defaults.
- `repo-coding/agents/repo-worker/` is the package-local child launch target.
- `references/`, `evals/`, and `scripts/` remain package-local authoring,
  validation, and harness surfaces.

#### Scenario: Docs name the repo-worker implementation
- **WHEN** docs describe the first-party repo-worker package, child launch
  root, micro-skills, scripts, evals, or harness entrypoints
- **THEN** they point to `crates/agent-engine/skills/repo-coding/` and the relevant
  OpenSpec owners rather than a historical top-level staging copy

### Requirement: Coding governance separates steward and repo-worker fast paths
alan SHALL distinguish the parent steward's safe orchestration actions from the
repo worker's bounded repo-local coding loop.

Parent steward fast path:

1. workspace discovery and comparison
2. safe read-heavy repo selection
3. planning and routing decisions
4. spawn preparation and bounded result integration

Repo-worker fast path:

1. repo-local reads and searches
2. repo-local edits inside the bound workspace
3. targeted deterministic verification
4. bounded delivery summaries and residual-risk reporting

The parent steward must not silently mutate multiple repos or publish
externally under a generic coding-task interpretation. The repo-worker fast
path ends when the task crosses trust, workspace, credential, or publish
boundaries.

#### Scenario: Steward task becomes mutating cross-repo work
- **WHEN** a parent steward task moves from discovery or routing into mutation
  across multiple repos
- **THEN** alan routes through explicit worker launches and approvals instead
  of treating the parent fast path as sufficient

### Requirement: Coding governance enforces owner-boundary classes
alan SHALL escalate or deny owner-boundary classes that exceed ordinary
repo-local coding work.

Minimum boundary classes:

1. cross-workspace mutation beyond delegated repo scope
2. network or external publishing actions
3. credential exploration or modification
4. shared deploy or infrastructure changes
5. destructive or ambiguous high-blast-radius actions
6. unknown-capability tools whose real blast radius is unclear

First-party repo-worker child policies keep these defaults explicit:

- unknown capability -> escalate
- network capability -> escalate
- publish commands -> escalate
- deploy and infrastructure commands -> escalate
- credential reads or writes -> escalate
- dangerous destructive commands -> deny

Path-sensitive escalation is appropriate for files such as `.github/workflows/`,
`deploy/`, `infra/`, and `.env*`.

#### Scenario: Repo worker touches infrastructure
- **WHEN** a repo worker attempts to modify deploy, infrastructure, workflow, or
  environment-secret surfaces
- **THEN** policy escalates or denies according to the child policy rather than
  treating the edit as ordinary repo-local coding

### Requirement: Coding governance documents current policy hooks and limits
alan SHALL document the current policy matcher surface and its known limits for
coding workflows.

Current matcher surface:

1. `tool`
2. `capability`
3. `match_command`
4. `match_path_prefix`

Path-prefix rules:

- `match_path_prefix` is evaluated against common file-oriented arguments such
  as `path`, `paths`, `directory`, `cwd`, and `workspace_root`.
- Before matching, alan lexically normalizes `.` and `..` segments.
- Relative policy prefixes may match absolute tool paths on component
  boundaries.
- When the runtime has a current tool `cwd`, relative path arguments are also
  evaluated against that base so parent-traversal paths do not bypass policy.
- alan conservatively case-folds path-prefix comparisons so case variants do
  not bypass policy on case-insensitive hosts.

Known limits:

- Bash payloads are not fully path-classified.
- Cross-workspace intent is inferred mainly from launch shape and path guard
  rather than a dedicated policy dimension.
- Trust-boundary metadata such as `owner_boundary` or `blast_radius` is not yet
  modeled as first-class policy fields.
- The current backend remains `workspace_path_guard`, which is best-effort
  rather than strict containment.

#### Scenario: Shell command needs path-sensitive policy
- **WHEN** a shell command may cross a sensitive path boundary
- **THEN** alan uses current `match_command` and available path context
- **AND** docs and delivery summaries do not claim strict OS containment from
  the current backend

### Requirement: Coding changes preserve surrounding behavior by default
alan SHALL treat existing behavior and tests as presumptive constraints for
repo-scoped coding work unless the requested fix requires behavior change.

Rules:

1. Existing tests and nearby behavior guards are presumptive constraints.
2. The default repair shape is minimal product-code change plus focused
   regression coverage.
3. Workers must not weaken or rewrite existing tests merely to make a guessed
   patch pass.
4. Modifying an existing test requires an explicit behavior-level reason, not
   only local convenience.
5. When the issue statement and existing tests appear to conflict, the worker
   surfaces the discrepancy instead of silently normalizing one side away.

#### Scenario: Existing test conflicts with issue statement
- **WHEN** a worker finds that the issue statement and current tests imply
  different behavior
- **THEN** it reports the discrepancy and seeks a behavior-level resolution
  rather than rewriting tests for convenience

### Requirement: Coding execution is recoverable and fail-safe
alan SHALL keep repo-scoped coding execution recoverable through normal runtime
checkpoint and audit paths.

Execution constraints:

1. unfinished coding runs should resume after restart through the normal
   checkpoint and session-recovery path
2. irreversible side effects should remain dedupable through runtime effect
   record and audit surfaces
3. unknown side-effect status must fail safe and escalate rather than silently
   proceeding

#### Scenario: Side-effect status is unknown after recovery
- **WHEN** a coding run resumes and cannot determine whether an irreversible
  side effect already happened
- **THEN** alan escalates or stops safely rather than repeating the side effect
  silently

### Requirement: Coding eval ladder separates product invariants from benchmark adapters
alan SHALL validate coding steward orchestration, repo-worker execution,
package-local evals, and external benchmark adapters as separate layers.

Validation ladder:

1. **Coding steward harness** validates parent-side orchestration behavior:
   delegated launch contracts, workspace-root versus nested-cwd binding,
   default non-inheritance, explicit handle handoff, bounded result integration,
   and fail-safe behavior when delegated execution or artifact routing is
   unavailable.
2. **Repo-worker harness** validates bounded child behavior: minimum
   inspect -> plan -> edit -> verify -> deliver loop, control-mode stability,
   restart recovery, irreversible-effect dedupe continuity, and governance
   boundary coverage.
3. **Package-local benchmark scaffold** lives under
   `crates/agent-engine/skills/repo-coding/evals/evals.json` and covers activation
   selection, bounded single-repo routing, multi-repo steward-owned cases, and
   owner-boundary escalation cases.
4. **External benchmark adapters** transform outside task corpora into
   operator-side eval surfaces and measure general coding quality; they do not
   define task-specific runtime behavior.

Recommended external benchmark bring-up order is Lite-first:

1. single-case SWE-bench Lite bring-up through the steward entrypoint
2. curated Lite subset runs
3. full Lite runs
4. curated SWE-bench Pro expansion after the Lite path is stable

#### Scenario: Benchmark fixture suggests a prompt shortcut
- **WHEN** an external benchmark reveals a failure
- **THEN** the fix is generalized into reusable contract, prompt, policy, or
  harness behavior rather than encoded as a benchmark-only heuristic

### Requirement: Coding eval surfaces and KPI fields are stable
alan SHALL keep executable coding eval entrypoints and KPI output fields
stable enough for steward and worker regression tracking.

Minimum executable surfaces:

1. `bash scripts/harness/run_coding_steward_suite.sh`
2. `bash scripts/harness/run_repo_worker_suite.sh`
3. `cargo run -p alan -- skills eval crates/agent-engine/skills/repo-coding`

First external benchmark operator-run surfaces:

1. `bash crates/agent-engine/skills/swebench/scripts/run_swebench_full_steward_case.sh <case-json>`
2. `bash crates/agent-engine/skills/swebench/scripts/run_swebench_full_steward_subset.sh <suite-json>`
3. `bash crates/agent-engine/skills/swebench/scripts/score_swebench_predictions.sh <predictions-jsonl>`

Shared KPI fields:

1. `suite`
2. `mode`
3. `total`
4. `passed`
5. `failed`
6. `skipped`
7. `pass_rate_percent`
8. `duration_secs`
9. `executed_scenarios`
10. `kpi_tag_counts`

Suite-specific fields may extend this, such as `profile` for autonomy.

#### Scenario: Harness KPI is emitted
- **WHEN** a steward or repo-worker harness writes KPI output
- **THEN** it includes the shared fields needed for later aggregation and may
  add suite-specific fields without redefining the shared contract
