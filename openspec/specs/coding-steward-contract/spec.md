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

- **Coding steward**: the parent alan runtime that owns goal intake, repository
  discovery, routing, approvals, and result integration.
- **Repo worker**: a child runtime launched into a specific repo, directory, or
  project to perform bounded coding work.
- **Coding launch**: a `SpawnSpec` used specifically for repo-scoped coding
  execution.
- **Bound handles**: explicit parent-side state the child receives, such as
  `approval_scope`, `plan`, `conversation_snapshot`, `tool_results`, or
  `memory`. Host file authority travels separately in the Process Launch
  Context as explicit Host Mount grants.
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
alan SHALL model coding work as a parent steward that delegates bounded
repo-local execution to fresh child Agent Processes instead of as one default
single-repo coding shell.

Required product sequence:

1. the parent steward accepts the user's coding-oriented goal
2. the parent discovers or selects the correct repository or directory
3. the parent launches one or more repo workers through explicit Process Launch
   Context contracts
4. each repo worker performs bounded repo-scoped coding work
5. the parent integrates results, handles approvals, and decides whether more
   routing or child launches are needed

Boundary rules:

- Runtime parent-child relations apply to Agent Process launches.
- Agent Definitions are explicit launch inputs, not Host-directory overlays.
- Coding product behavior must not blur Process lineage with Agent Definition
  resolution.
- External benchmark results measure coding quality but do not define product
  behavior, repo-specific heuristics, task-specific prompts, or benchmark-only
  special cases.
- Coding improvements target reusable behavior such as code understanding,
  change-boundary control, verification discipline, and honest delivery.

#### Scenario: Single-repo bug fix is delegated
- **WHEN** a user asks alan to perform a repo-local bug fix
- **THEN** the parent steward selects the repo and launches a repo worker with
  an explicit Host Mount and approval scope
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
2. repository discovery, comparison, and selection
3. task routing across repos, directories, or projects
4. launch-shape decisions for child runtimes
5. approval ownership for risky cross-repository or external actions
6. result integration, dedupe, and follow-up planning
7. deciding whether the task remains repo-local or has expanded into broader
   orchestration

The child repo worker owns:

1. inspect -> plan -> edit -> verify -> deliver inside the delegated repo or
   directory
2. repo-local side effects within the granted Host Mount scope
3. maintaining a bounded coding transcript for the delegated task
4. producing a delivery summary with verification and residual-risk status
5. returning control when complete, blocked, or attempting to expand beyond
   delegated scope

The parent steward is not the default place for every repo-local edit loop. The
child worker must not silently broaden namespace, Host Mount, approval,
credential, or external-action scope beyond what the launch contract granted.

#### Scenario: Worker attempts to leave delegated scope
- **WHEN** a repo worker needs to mutate outside its granted Host Mount or perform
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
- The Process Launch Context carries the explicit Host Mount that defines the
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
  launch inputs, mounts, descriptors, and bound handles

#### Scenario: Deprecated reasoning shortcut is supplied
- **WHEN** a coding launch attempts to use `launch.budget_tokens`
- **THEN** alan rejects the launch shape instead of treating token budget as
  reasoning effort

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

1. repository discovery and comparison
2. safe read-heavy repo selection
3. planning and routing decisions
4. spawn preparation and bounded result integration

Repo-worker fast path:

1. repo-local reads and searches
2. repo-local edits inside the granted Host Mount
3. targeted deterministic verification
4. bounded delivery summaries and residual-risk reporting

The parent steward must not silently mutate multiple repos or publish
externally under a generic coding-task interpretation. The repo-worker fast
path ends when the task crosses trust, Host Mount, credential, or publish
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

1. cross-repository mutation beyond delegated Host Mount scope
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
  as `path`, `paths`, `directory`, and `cwd`.
- Before matching, alan lexically normalizes `.` and `..` segments.
- Relative policy prefixes may match absolute tool paths on component
  boundaries.
- When the runtime has a current tool `cwd`, relative path arguments are also
  evaluated against that base so parent-traversal paths do not bypass policy.
- alan conservatively case-folds path-prefix comparisons so case variants do
  not bypass policy on case-insensitive hosts.

Known limits:

- Bash payloads are not fully path-classified.
- Cross-repository intent is inferred mainly from launch shape and Host Mount guard
  rather than a dedicated policy dimension.
- Trust-boundary metadata such as `owner_boundary` or `blast_radius` is not yet
  modeled as first-class policy fields.
- The current backend remains `host_mount_path_guard`, which is best-effort
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

### Requirement: Coding eval ladder separates product invariants from benchmark adapters
Alan SHALL validate coding steward orchestration, repo-worker execution, package-local evals, and
external benchmark adapters as separate layers.

Validation ladder:

1. **Coding steward harness** validates parent-side orchestration behavior: delegated launch
   contracts, Host Mount root versus nested namespace-cwd binding, default non-inheritance, explicit handle
   handoff, bounded result integration, and fail-safe behavior when delegated execution or artifact
   routing is unavailable.
2. **Repo-worker harness** validates bounded child behavior: minimum
   inspect -> plan -> edit -> verify -> deliver loop, control-mode stability, restart recovery,
   irreversible-effect dedupe continuity, and governance boundary coverage.
3. **Package-local benchmark scaffold** lives under
   `crates/agent-engine/skills/repo-coding/evals/evals.json` and covers activation selection,
   bounded single-repo routing, multi-repo steward-owned cases, and owner-boundary escalation cases.
4. **External benchmark adapters** prepare benchmark workspaces and manifests, validate or set up
   the official harness, and score predictions produced by an independently selected Agent Process
   execution workflow. They do not define or host Agent Process execution.

Recommended external benchmark bring-up order is Lite-first:

1. prepare and materialize one SWE-bench Lite case
2. produce `predictions.jsonl` through an independently selected Agent Process execution workflow
3. score the single case with the official harness
4. expand to curated Lite subsets and full Lite runs
5. expand to curated SWE-bench Pro only after the Lite measurement path is stable

#### Scenario: Benchmark fixture suggests a prompt shortcut
- **WHEN** an external benchmark reveals a failure
- **THEN** the fix is generalized into reusable contract, prompt, policy, or harness behavior
  rather than encoded as a benchmark-only heuristic

#### Scenario: Benchmark execution is requested before a file-native launcher exists
- **WHEN** an operator needs predictions and Alan has no accepted file-native benchmark Process
  launcher
- **THEN** the benchmark package exposes preparation, manifest, harness, and scoring surfaces only
- **AND** it does not recreate a removed host-control runner or compatibility wrapper

### Requirement: Coding eval surfaces and KPI fields are stable
Alan SHALL keep executable coding eval entrypoints and KPI output fields stable enough for steward
and worker regression tracking.

Minimum executable surfaces:

1. `bash scripts/harness/run_coding_steward_suite.sh`
2. `bash scripts/harness/run_repo_worker_suite.sh`
3. `cargo run -p alan -- skills eval crates/agent-engine/skills/repo-coding`

Current external benchmark operator surfaces:

1. `crates/agent-engine/skills/swebench/bin/swebench-lite-prepare-workspaces`
2. `crates/agent-engine/skills/swebench/bin/swebench-lite-materialize-subset`
3. `bash crates/agent-engine/skills/swebench/scripts/check_swebench_harness_env.sh`
4. `bash crates/agent-engine/skills/swebench/scripts/setup_swebench_harness_env.sh`
5. `bash crates/agent-engine/skills/swebench/scripts/score_swebench_predictions.sh <predictions-jsonl>`

The benchmark package SHALL NOT expose a package-local Agent Process execution runner until a
separate accepted change defines a file-native launcher. Prediction production remains an explicit
external input to the retained scoring surface.

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
- **THEN** it includes the shared fields needed for later aggregation
- **AND** it may add suite-specific fields without redefining the shared contract

#### Scenario: Operator scores independently produced predictions
- **WHEN** an operator has a `predictions.jsonl` file from an independently selected execution
  workflow
- **THEN** the retained scoring entrypoint evaluates it with the official harness
- **AND** no deleted package-local execution runner is required

### Requirement: Coding handles identify parent and worker Processes
Coding steward handles SHALL identify the parent Agent Process, worker Agent Process or executable,
bounded repository descriptors, namespace/policy inputs, and rollout/checkpoint evidence. They
SHALL use those concrete owners as their complete identity and authority boundary.

#### Scenario: A steward delegates repository work
- **WHEN** the steward spawns a bounded repo worker
- **THEN** the handle links the worker to the parent Agent Process and delegated repo resources
- **AND** Process, namespace, policy, and evidence files are sufficient to inspect and govern it

### Requirement: Coding recovery is file and Process based
Coding execution SHALL recover from authoritative Process state, steward/worker files, durable
checkpoints, rollouts, and repository evidence. It SHALL fail closed when those owners cannot prove
continuity.

#### Scenario: A worker disappears during coding work
- **WHEN** the worker Process exits or becomes unavailable before handoff
- **THEN** the steward reconstructs status from Process and durable evidence
- **AND** it resumes work only when those owners prove a safe continuation point
