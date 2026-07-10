# Skills & Tools — Extending the Machine

> Status: current tool behavior plus accepted V2 governance direction.
> The authoritative current governance contract lives in
> [`governance_current_contract.md`](./governance_current_contract.md).
>
> The authoritative skill-system contract now lives in OpenSpec:
> [`skill-system-contract`](../openspec/specs/skill-system-contract/spec.md).
> This document focuses on current implementation details, runtime surfaces,
> and operator-facing behavior.

## Overview

In the AI Turing Machine model, the core runtime is a generic state-transition engine. **Tools** are the side-effect interface — how the agent operates on the external world. **Skills** are dynamic instruction extensions — Markdown documents that reshape the agent's behavior at runtime. Together they let alan remain a small, generic core while supporting arbitrarily rich capabilities.

| Concept     | TM Role               | Implementation                                        |
| ----------- | --------------------- | ----------------------------------------------------- |
| **Tools**   | Side effects          | `Tool` trait in `alan-runtime`, impls in `alan-tools` |
| **Skills**  | Instruction extension | Markdown + YAML, rendered inline or as delegated capability stubs |
| **Sandbox** | Boundary constraint   | Current execution backend implementation (filesystem/process/network limits)  |
| **Policy**  | Decision boundary     | `PolicyEngine` rules (`allow/deny/escalate`)           |

---

## Tool System

### Architecture

The runtime defines **what a tool looks like**; a separate crate provides **concrete implementations**. This keeps the core provider-agnostic and domain-agnostic.

```
alan-runtime (trait + registry)          alan-tools (implementations)
┌──────────────────────────────┐        ┌─────────────────────────────┐
│  Tool trait                  │◄───────│  ReadFileTool               │
│  ToolRegistry                │        │  WriteFileTool              │
│  ToolContext                 │        │  EditFileTool               │
│  Sandbox                     │        │  BashTool                   │
└──────────────────────────────┘        │  GrepTool                   │
                                        │  GlobTool                   │
                                        │  ListDirTool                │
                                        └─────────────────────────────┘
```

### Tool Trait

Every tool implements a single trait ([registry.rs](../crates/runtime/src/tools/registry.rs)):

```rust
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> Value;            // JSON Schema
    fn execute(&self, args: Value, ctx: &ToolContext) -> ToolResult;
    fn capability(&self, args: &Value) -> ToolCapability;  // read / write / network / unknown
    fn timeout_secs(&self) -> usize;                 // tool default timeout (seconds)
}
```

### ToolRegistry

`ToolRegistry` manages tool lifecycle: registration, lookup, schema validation, and execution. It generates `ToolDefinition` objects for LLM function-calling APIs and enforces JSON Schema validation before dispatch.

### ToolContext

Each tool invocation receives a `ToolContext` ([context.rs](../crates/runtime/src/tools/context.rs)) carrying:

| Field         | Purpose                                        |
| ------------- | ---------------------------------------------- |
| `cwd`         | Working directory for relative path resolution |
| `scratch_dir` | Temporary storage for tool intermediates       |
| `config`      | Shared runtime configuration (`Arc<Config>`)   |

### Built-in Tool Profiles

`alan-tools` ships 7 built-ins with layered profiles:

- **Core (default)**: `read_file`, `write_file`, `edit_file`, `bash`
- **Read-only exploration**: `read_file`, `grep`, `glob`, `list_dir`
- **All built-ins**: core + exploration tools (7 total)

### Virtual Tools

Runtime always injects three baseline virtual tools into the LLM toolset for
planning and human-in-the-loop control:

- `request_confirmation` — pause execution and emit `Event::Yield` with kind `confirmation`
- `request_user_input` — pause execution and emit `Event::Yield` with kind `structured_input`
- `update_plan` — update in-memory plan metadata before continuing in the current turn

Parent runtimes also expose `invoke_delegated_skill` when delegated skill
execution is supported. Launch-root runtimes intentionally keep nested
delegated execution off in V1, so they do not expose that tool by default.

When a delegated task targets a different local workspace than the current
runtime, `invoke_delegated_skill` may also carry an explicit `workspace_root`
and an optional nested `cwd` so the child runtime binds to the correct local
scope instead of inheriting the parent workspace. Those launch paths should be
absolute by the time they reach the child launch contract; relative inputs must
be resolved or rejected first, and `cwd` must stay nested under
`workspace_root` when both are provided.

These are implemented in `runtime/virtual_tools.rs` and are handled by the
runtime itself, not `alan-tools`.

### Tool Catalog

| Tool         | Capability | Description                                                |
| ------------ | ---------- | ---------------------------------------------------------- |
| `read_file`  | Read       | Read file contents, supports offset/limit, image detection |
| `write_file` | Write      | Write file, auto-creates parent directories                |
| `edit_file`  | Write      | Search-and-replace editing                                 |
| `bash`       | Dynamic    | Shell command execution with command-based capability classification |
| `grep`       | Read       | Recursive regex search across files                        |
| `glob`       | Read       | File path pattern matching                                 |
| `list_dir`   | Read       | Directory listing, directories sorted first                |

All implementations live in [alan-tools/src/lib.rs](../crates/tools/src/lib.rs).

Tool catalog identity is separate from runtime execution binding. Built-in
definitions keep stable names/schema/locality across runtimes, while
workspace-specific facts such as `workspace_root` and `cwd` belong to runtime
binding/context. See the OpenSpec
[`governance-tooling-contract`](../openspec/specs/governance-tooling-contract/spec.md).

`bash` exposes a `timeout` argument in schema (1–300, default 60), and the tool-level default timeout is currently 300 seconds (`timeout_secs`).

### Tool Governance: HITE First, Execution Backend Second

alan V2 governance is:

1. **Policy gate**: per-call decision (`allow`, `deny`, `escalate`).
2. **Execution backend**: the current `workspace_path_guard` backend is a best-effort execution guard, not a strict OS sandbox. Daemon session APIs surface it as `execution_backend`.

When policy returns `escalate`, runtime emits `Event::Yield` and waits for `Op::Resume`. This path is explicit and does not depend on session-level approval toggles.

Current contract: [governance_current_contract.md](./governance_current_contract.md).  
Target V2 design and policy file format: [`governance-tooling-contract`](../openspec/specs/governance-tooling-contract/spec.md).

### Verification-First Response Guardrails

Runtime prompt guidance now pushes the model to probe before claiming that
tools or current/external data are unavailable. That prompt nudge is only the
first layer. alan also applies a runtime response guardrail before assistant
text is emitted:

- if a draft claims tools are unavailable while tools are registered, runtime
  retries once with a correction instruction
- if a draft claims current/external data access is unavailable while a
  network-capable tool exists, runtime retries once before any user-visible
  text is emitted
- drafts that already contain tool calls bypass this retry path so the tool
  orchestration loop remains the source of truth

The accepted-output rule is therefore: user-visible assistant text should be
the post-guardrail draft, not the first contradictory draft.

### Steering During Tool Execution

`Op::Input` is treated as steering input. During a tool batch, runtime checks in-band steering after each tool call. If steering exists, remaining calls are marked as skipped and the steering message is injected before the next LLM generation.

### Durable Tool Payloads

alan distinguishes between:

- **live tool payloads on tape**: full in-session tool results used for the
  current runtime's reasoning and replay inside the same process
- **durable rollout payloads**: redacted/truncated projections written to
  rollout `tool_call`, `message`, and `effect` records

Durable payload persistence now redacts common secret-bearing fields such as
`authorization`, `proxy-authorization`, `cookie`, `set-cookie`, and common API
key / token-shaped fields before writing rollout history. Long string bodies
and oversized collections are also truncated. Effect records keep digests over
the durable payload so replay/dedupe stays auditable without durably storing
every raw byte.

### Filesystem Sandbox

The current `Sandbox` ([sandbox.rs](../crates/runtime/src/tools/sandbox.rs)) enforces workspace-only filesystem access:

- **Path validation** — `is_in_workspace()` canonicalizes paths (via `dunce`) and checks containment
- **New file support** — walks parent directories to validate paths that don't exist yet
- **Read / Write / Exec / ListDir** — all operations check workspace containment before proceeding
- **Protected subpaths (current path-guard backend)** — file writes are blocked by default under `.git`, `.alan`, and `.agents`, and process path references into those subpaths are blocked conservatively because shell commands cannot be proven read-only
- **Plain shell commands only** — the workspace path guard rejects shell variable, command, brace, and glob expansion, rejects shell control flow, rejects common wrapper forms like `env`, `command`, `builtin`, `exec`, `time`, `nice`, `nohup`, `timeout`, `stdbuf`, and `setsid`, rejects direct nested evaluators like `eval`, `.`, and `sh/bash/python -c`, rejects direct opaque command dispatchers like `xargs` and `find -exec`, and rejects a curated set of common direct script interpreters like `python file.py`, `bash script.sh`, and `awk -f script.awk` because script bodies and dynamic child paths cannot be validated safely before execution. It validates explicit path-like argv references and redirection targets, but it does not infer utility-specific operand roles for arbitrary bare tokens. It also does not inspect arbitrary program-internal writes or dispatch, including commands that mutate private state without an explicit path operand such as `git init`, `git add`, or `git config --local`, utility actions like `find -delete`, or utility-specific script/DSL modes or opaque recipe/script execution inside build or task runners, such as `sed -f`.
- **No OS-level sandboxing (current state)** — no Landlock, Seatbelt, or container isolation; purely path-based

V2 direction: keep path-based checks as a lightweight local guard. Stronger
containment may exist as an optional deployment feature, but HITE governance
must not depend on it.

---

## Skill System

### Design

alan's definition layer works with filesystem-based capability packages. The
current stable directory-backed shape is a single-skill package with optional
alan-native extensions:

```
my-skill/
├── SKILL.md              # Required: YAML frontmatter + Markdown instructions
├── skill.yaml            # Optional: alan-native machine metadata for this skill
├── package.yaml          # Optional: package-level defaults for alan-native metadata
├── bin/                  # Optional: package-local executable tools
├── scripts/              # Optional: executable code the agent can invoke via bash
├── references/           # Optional: reference documentation
├── assets/               # Optional: templates, resources
├── evals/                # Optional: explicit authoring/eval manifests and fixtures
├── eval-viewer/          # Optional: static review/viewer assets
└── agents/               # Optional: alan-native package-local launch targets
```

Public `.agents/skills/<skill-id>/` installs are adapted automatically as
single-skill packages. alan-native extensions currently live inside that same
directory, most importantly sidecars, launch targets under `agents/`, and
explicit authoring/eval assets under `evals/` and `eval-viewer/`.
The stable package contract also reserves `bin/` for package-local executable
tools that travel with the skill package. Shipping an entry there does not make
it a host-global tool by itself.
The runtime skill id is a normalized lower-case hyphenated slug derived from
the package directory name (`<skill-id>/`) rather than from frontmatter `name`.

### SKILL.md Format

```yaml
---
name: skill-name
description: What this skill does and when to use it
metadata:
  short-description: Brief one-liner
  tags: ["tag1", "tag2"]
---

# Instructions

Step-by-step guidance for the agent...
```

Portable skill selection is driven by `name` and `description`. Hosts may still
offer their own force-select controls at runtime, but portable skills do not
define their own alias, keyword, or regex trigger metadata.

### alan Sidecar Metadata

alan also supports optional machine-readable sidecars that do not change the
`SKILL.md` portability contract:

- `skill.yaml`: skill-specific alan-native runtime metadata
- `package.yaml`: package-level runtime defaults applied before the skill sidecar

Stable sidecar keys are intentionally narrow:

- `runtime.execution.mode`
- `runtime.execution.target`
- `runtime.allow_implicit_invocation`
- `runtime.permission_hints`

`SKILL.md` remains the canonical source for identity, selection behavior,
availability requirements, and instructions. Sidecar precedence applies only to
runtime metadata:

1. `package.yaml` `skill_defaults.runtime`
2. `skill.yaml` `runtime`

This is fail-open: when sidecar files are absent, discovery and activation still
work from `SKILL.md` alone. Invalid sidecars are recorded as non-fatal load
errors and alan skips only the broken overlay while keeping any other valid
sidecar layers.

alan currently uses sidecar runtime metadata for two product behaviors:

- `runtime.permission_hints` can be attached to active-skill confirmation and
  approval surfaces as advisory context before privileged actions
- `runtime.allow_implicit_invocation` can hide an enabled skill from the prompt
  catalog without disabling explicit activation
- `runtime.execution.*` resolves whether a skill stays inline or delegates to a
  package-local launch target

Unavailable-skill remediation for missing tools, typed dependencies, or minimum
alan version still comes from `SKILL.md` frontmatter rather than from sidecars.

### Current Status And Partial Areas

The following pieces are implemented and should be treated as the current
contract:

- capability-package discovery from built-ins, agent roots, and public
  `.agents/skills/` directories
- directory-backed built-in first-party packages, including a shipped
  `skill-creator` package with ordinary package assets
- directory-derived runtime skill ids for single-skill packages
- per-skill runtime exposure with `enabled` and `allow_implicit_invocation`
- description-driven portable selection plus host-level force-selection without
  skill-authored aliases
- path-aware prompt injection and progressive disclosure
- delegated skill execution with package-local launch targets
- package-local `bin/` directories recognized as part of the stable package
  resource shape
- explicit eval entrypoints over `evals/evals.json` plus legacy `scripts/eval.*`
- reusable structured eval artifact regeneration through
  `alan skills aggregate-benchmark` and `alan skills generate-review`
- a separate first-party `swebench` package with package-local `bin/`
  entrypoints and colocated workspace tooling under
  `crates/runtime/skills/swebench/tooling/`
- daemon skill catalog, changed-cursor polling, and skill-override writes

The following inputs are not part of the stable runtime contract:

- `viewers/` directories are tolerated in package trees, but alan does not
  export them through the capability view, CLI, or daemon skill catalog
- package-local `bin/` entries are part of the stable package shape, but direct
  runtime tool binding for those executables remains a future surface; packages
  currently invoke them through existing host tools such as `bash`
- `compatibility.requirements` is advisory remediation text, not a typed
  dependency gate
- `runtime.ui` is tolerated sidecar input without a stable runtime consumer and
  is not preserved in resolved runtime metadata
- `agents/openai.yaml` compatibility metadata is ingested for catalog/UI-facing
  interface fields and dependency hints, but those hints remain
  compatibility-only metadata in the current runtime. Unknown hints, including
  MCP-oriented hints in the current alan runtime, are ignored for availability
  gating. The file does not replace `SKILL.md` or alan sidecars as the
  canonical runtime contract
- authoring assets such as `agents/*.md` are tolerated, but alan does not load
  them as runtime capabilities by default

### Delegated Skill Execution

alan now resolves a package-local execution contract for each discovered skill.

The current resolved states are:

- `inline`
- `delegate(target=...)`
- `unresolved(...)`

Execution metadata lives in alan sidecars rather than `SKILL.md`, for example:

```yaml
runtime:
  execution:
    mode: delegate
    target: reviewer
```

Default inference is package-local and deterministic:

- if a package exports no launch targets, the skill resolves to `inline`
- if a skill id matches a launch target export name, it resolves to `delegate`
- if a package exports exactly one skill and exactly one launch target, that
  skill resolves to `delegate`
- ambiguous package shapes do not guess; they resolve to `unresolved(...)`
  until explicit sidecar metadata is present

This keeps delegated execution strict and predictable.

When an active skill resolves to `delegate(target=...)`, parent alan runtimes
expose delegated invocation by default. alan no longer injects the full
`SKILL.md` body into the parent prompt. Instead, the parent runtime sees a
lightweight delegated-capability stub with:

- the resolved `skill_id`
- the resolved delegated `target`
- an explicit `invoke_delegated_skill` runtime tool contract

This keeps the parent-runtime context small and makes delegated execution an
explicit runtime-owned path instead of an inline prompt convention.

The delegated tool now launches the resolved package-local launch target
through `SpawnSpec` with a fresh launch-root runtime and explicit handles only.
V1 keeps that default launch narrow: the launch-root runtime gets `Workspace` and
`ApprovalScope`, but it does not inherit parent tape, active skills, plan
state, or memory by default.

That delegated-launch default is intentionally conservative and runtime-wide. It
is not the only valid launch shape for higher-level products. Coding-oriented
steward flows may explicitly bind additional handles such as `plan`,
`conversation_snapshot`, `tool_results`, or `memory` when the parent needs a
stronger handoff into a repo-scoped child worker.

alan still preserves a compatibility fallback for runtimes that do not expose
delegated invocation, for example launch-root runtimes where nested delegated
execution is intentionally disabled in V1. In those runtimes, delegated skills
keep their inline `SKILL.md` instructions and surface a short runtime-fallback
note instead of a non-functional delegated tool path.

The delegated invocation path records a bounded invocation/result object in the
parent tape:

```json
{
  "skill_id": "repo-review",
  "target": "repo-review",
  "task": "Review the current diff for correctness and missing tests.",
  "result": {
    "status": "completed",
    "summary": "High-level delegated outcome",
    "child_run": {
      "session_id": "child-session-id",
      "child_run_id": "child-run-id",
      "rollout_path": ".alan/runtime/<channel>/sessions/child-rollout.jsonl"
    },
    "output_ref": {
      "session_id": "child-session-id",
      "rollout_path": ".alan/runtime/<channel>/sessions/child-rollout.jsonl",
      "field": "output_text"
    },
    "structured_output": {
      "optional": "capability-specific data"
    },
    "truncation": {
      "output_truncated": true
    }
  }
}
```

The parent consumes that bounded record instead of replaying the child's full
transcript into parent context. If `output_text` is present, it is the complete
inline child output. If `output_ref` or truncation metadata is present, the
inline text is only a preview and the parent or operator should inspect the
namespace file at `output_ref.path` for full detail. Raw rollout and session
paths are optional debug metadata, not evidence access paths.

The parent rollout keeps a richer out-of-band reference for debugging and
auditing. Its delegated tool-call record stores the `child_run` object with the
child session id, child-run id, rollout path when durable, and terminal status.
Operators inspect live child Agent Processes through `/agent/<pid>/children`
and `/proc`. Parent Agent Processes request governed termination through
`terminate_child_run`; external operators may write `cancel` or `interrupt` to
`/proc/<child-pid>/ctl`. There is no daemon child-run control-plane API.

`alan skills list` and `alan skills packages` also surface each resolved
execution mode and flag unresolved delegated-package shapes with explicit
diagnostics.

If execution resolves to `unresolved(...)`, alan treats that skill as
unavailable and surfaces diagnostics in CLI, catalog, and remediation paths
rather than silently falling back to inline behavior.

### Progressive Disclosure

alan now consumes `capabilities.disclosure` during prompt assembly instead of
leaving it schema-only.

```yaml
capabilities:
  disclosure:
    level2: details.md
    level3:
      references: ["quickstart.md"]
      scripts: ["scripts/check.sh"]
      assets: ["assets/template.txt"]
```

Runtime behavior:

- `level2` chooses the primary instruction document injected for the active
  skill. When omitted, alan uses the `SKILL.md` body.
- `level3` lists package resources that may be expanded when the active
  instruction text actually references them.
- Relative resource references already mentioned in the active instruction text,
  such as `references/guide.md` or `scripts/build.sh`, are resolved
  deterministically against the canonical `resource_root`. Declared `level3`
  entries that are never referenced stay out of the active prompt.
- Prompt-cache invalidation tracks the concrete disclosed files, so edits to a
  referenced `details.md` or `references/*.md` file invalidate the active-skill
  render without requiring a full directory rescan.

### Capability-Package Sources

alan now resolves skills through one `ResolvedCapabilityView` instead of a
separate `repo/user/builtin` loading path. The current capability sources are:

| Source         | Location / Form                                         | Role                          |
| -------------- | ------------------------------------------------------- | ----------------------------- |
| **Built-in**   | Embedded first-party package assets                     | Core alan capabilities        |
| **User public skills** | `~/.agents/skills/`                              | Zero-conversion public installs |
| **User roots** | `~/.alan/agents/default/skills/` and `~/.alan/agents/<name>/skills/` | alan-native cross-project capability sources |
| **Workspace public skills** | `<workspace>/.agents/skills/`              | Zero-conversion workspace installs |
| **Workspace roots** | `.alan/agents/default/skills/` and `.alan/agents/<name>/skills/` | alan-native project/workspace capability sources |

Within the user and workspace sources, alan follows the resolved `AgentRoot`
overlay chain, and later roots override earlier ones when skill IDs collide.
Here `agent/` is the default definition root and `agents/<name>/` is one named
definition root selected by `agent_name`; named roots extend the default roots.

Overlay order is:

- Default workspace agent: `~/.alan/agents/default -> <workspace>/.alan/agents/default`
- Named agent: `~/.alan/agents/default -> <workspace>/.alan/agents/default -> ~/.alan/agents/<name> -> <workspace>/.alan/agents/<name>`

A standards-compatible skill directory with `SKILL.md` and optional
`scripts/`, `references/`, `assets/`, or package-local launch targets under
`agents/` is adapted automatically into a single-skill package. This keeps
public skill compatibility without requiring a custom `package.toml`.

Direct installation can therefore stay zero-conversion:

```text
~/.agents/skills/<skill-name>/SKILL.md
<workspace>/.agents/skills/<skill-name>/SKILL.md
```

`alan init` creates `<workspace>/.agents/skills/` for this workflow, and the
first-run setup wizard creates `~/.agents/skills/` alongside the canonical
global agent config.

### Skill Overrides

Package discovery and runtime exposure are separate. Roots discover packages
from `skills/`, but runtime exposure is now resolved per skill:

```toml
[[skill_overrides]]
skill = "plan"
allow_implicit_invocation = false

[[skill_overrides]]
skill = "release-checklist"
enabled = false
```

Stable exposure fields are:

- `enabled`: disables the skill for the current runtime
- `allow_implicit_invocation`: controls whether the skill appears in the prompt
  catalog for model-side on-demand use
- overrides are keyed by the canonical runtime `skill` id only; legacy
  `skill_id` aliases and separator normalization are rejected

Defaults:

- `enabled = true`
- `allow_implicit_invocation = true` unless overridden by `skill.yaml`,
  `package.yaml`, or tolerated compatibility metadata such as
  `agents/openai.yaml`

Overrides are merged by runtime skill id across the resolved root chain. Later
roots override earlier values field-by-field without discarding unrelated
overrides.

### Built-In Packages

Seven first-party packages are embedded as built-in assets and included in the
resolved capability view:

| Skill      | Purpose                                                       |
| ---------- | ------------------------------------------------------------- |
| **memory** | Persistent pure-text memory across sessions (`{workspace_alan_dir}/runtime/<channel>/memory/`; stable can read legacy `{workspace_alan_dir}/memory/`) |
| **plan**   | Structured execution plans for complex tasks (`.alan/plans/`) |
| **repo-coding** | First-party bounded repo-local coding package for steward-owned child execution |
| **alan-shell-control** | Native alan terminal shell layout and pane control |
| **skill-creator** | First-party authoring and eval workflow guidance |
| **workspace-inspect** | First-party read-only workspace inspection delegation and child reader package |
| **workspace-manager** | Workspace lifecycle operations and recovery guidance |

Current built-in package ids are `builtin:alan-memory`,
`builtin:alan-plan`, `builtin:alan-repo-coding`,
`builtin:alan-shell-control`, `builtin:alan-skill-creator`,
`builtin:alan-workspace-inspect`, and `builtin:alan-workspace-manager`.

Source: [skills/memory/SKILL.md](../crates/runtime/skills/memory/SKILL.md), [skills/plan/SKILL.md](../crates/runtime/skills/plan/SKILL.md), [skills/repo-coding/SKILL.md](../crates/runtime/skills/repo-coding/SKILL.md), [skills/alan-shell-control/SKILL.md](../crates/runtime/skills/alan-shell-control/SKILL.md), [skills/skill-creator/SKILL.md](../crates/runtime/skills/skill-creator/SKILL.md), [skills/workspace-inspect/SKILL.md](../crates/runtime/skills/workspace-inspect/SKILL.md), [skills/workspace-manager/SKILL.md](../crates/runtime/skills/workspace-manager/SKILL.md)

Target memory contract: [`runtime-memory-contract`](../openspec/specs/runtime-memory-contract/spec.md)

These are exposed through the same package + skill-override model as every
other capability. Built-ins are a distribution source, not a separate runtime
skill kind.

Built-ins are no longer `always_active` by default. Any baseline behavior alan
needs unconditionally now belongs in the base prompt or runtime/tool
descriptions.

Built-ins are now materialized into a directory-backed packaged asset view
before capability discovery. That means resource roots, sidecars,
compatibility metadata, package-local launch targets, and prompt disclosure all flow
through the same code path as external filesystem packages.

You can inspect the resolved view directly from the CLI:

```bash
alan skills list
alan skills packages
alan skills init path/to/my-skill --template inline
alan skills validate path/to/my-skill
alan skills eval path/to/my-skill
```

`alan skills eval` is now manifest-first. When `evals/evals.json` exists, alan
runs the structured eval suite and writes `run.json`, `benchmark.json`, a
static review bundle, and per-case artifacts. When no manifest exists, alan
falls back to legacy `scripts/eval.sh` or `scripts/eval.py`.

For coding packages such as `repo-coding`, external benchmark ladders sit on
top of this local eval surface as adapter layers. They measure transfer
quality, but they do not define runtime behavior or justify benchmark-specific
prompt rules.

alan now exposes the same local-first management surface from the daemon:

- `GET /api/v1/skills/catalog` resolves the current packages, skills, skill
  exposure state, execution state, and availability snapshot for a workspace +
  optional named agent
- `GET /api/v1/skills/changed?after=<cursor>` returns a lightweight change
  check so clients do not need to re-fetch the full catalog on every poll
- `POST /api/v1/skills/overrides` writes a skill override through the
  highest-precedence writable agent root and returns the refreshed snapshot

These routes are intentionally local-first: `workspace_dir` is the default
workspace or a registered workspace alias / short id, not an arbitrary
filesystem path.

The write path persists to the resolved writable `agent.toml` for the target
agent definition layer. For example, the workspace default agent writes
`<workspace>/.alan/agents/default/agent.toml`, while a workspace named agent writes
`<workspace>/.alan/agents/<name>/agent.toml`.

`enabled: null` / `allowImplicitInvocation: null` removes an existing explicit
override field instead of editing the file manually.

### Triggering

Runtime activation is intentionally narrow. The injector
([injector.rs](../crates/runtime/src/skills/injector.rs)):

1. Extracts canonical `$skill-id` patterns from input
2. Resolves a structured active-skill envelope for each directly selected
   skill
3. Loads full content on demand for inline skills or delegated-fallback
   runtimes
4. Injects inline instructions or delegated capability stubs together with
   stable path and package context

The active-skill envelope now carries:

- skill metadata (`id`, `package_id`, `enabled`, `allow_implicit_invocation`)
- canonical `SKILL.md` path
- canonical package root and resource root when available
- availability state
- activation reason
- resolved execution state

Prompt injection consumes that structured shape instead of pathless Markdown
fragments. Active skill sections therefore include an `alan Runtime Context`
block before the skill body so downstream resource resolution can be
deterministic.

There is no skill-authored alias/keyword/pattern auto-activation and no
always-active skill injection. Host force-select surfaces are separate from the
portable skill contract; otherwise, portable skill discovery comes from the
prompt catalog's `name` and `description`.

Skill availability is also filtered by declared runtime requirements:

- `required_tools`
- `compatibility.dependencies`
- `compatibility.min_version`

If a skill fails those checks, alan keeps the package in the resolved
definition layer but excludes the skill from runtime activation. Explicit
mentions then surface a concrete unavailable reason instead of silently
injecting an unusable skill.

A skills catalog is also rendered into the system prompt so the LLM can choose
available skills on demand. That catalog includes canonical `SKILL.md` paths
for inline skills and direct `invoke_delegated_skill` guidance for delegated
skills. Inline skills are still read progressively on demand rather than being
injected wholesale.

Resource listings are now emitted relative to the envelope's `resource_root`
instead of bare filenames, for example `scripts/check.sh` rather than only
`check.sh`.

### Module Structure

```
crates/runtime/src/skills/
├── mod.rs             # exports + built-in skill/package assets
├── types.rs           # SkillMetadata, PortableSkill, CapabilityPackage, SkillOverride, ...
├── loader.rs          # Filesystem scanning + SKILL.md parsing
├── capability_view.rs # build ResolvedCapabilityView from package sources
├── registry.rs        # SkillsRegistry — exposure-aware lookup, listing, matching
└── injector.rs        # $mention extraction, prompt injection, catalog rendering
```

---

## Design Principles

1. **Generic Core** — `alan-runtime` defines `Tool` trait and `ToolRegistry` but contains zero tool implementations. The same core powers any tool set.

2. **Skills over Plugins** — Capabilities are Markdown instructions that shape behavior, not compiled code that extends the runtime. Adding a skill requires no recompilation.

3. **Self-Sufficient Capability Packages** — Packages carry their own skills,
   package-local executables, scripts, references, assets, and optional
   package-local launch targets. They may ship private binaries under `bin/`,
   but those remain package-scoped rather than becoming host-global tools.

4. **Tooling Layers Stay Separate** — host/runtime tools, package-local
   executable tools, package-local helper scripts, and reusable authoring/eval
   tooling are distinct layers. Shared skill tooling should not automatically
   become `alan` top-level subcommands or runtime tools.

5. **No MCP** — No external protocol dependencies. Tools are direct Rust trait implementations; skills are local filesystem documents.

6. **HITE Governance** — humans define boundaries, the agent executes inside them, and the current execution backend provides only a best-effort local guard.

7. **Path-Based Filesystem Isolation** — simple, portable workspace containment without OS-specific mechanisms; trades maximum isolation for zero external dependencies.

For the stable contract and compatibility target, see
[`skill-system-contract`](../openspec/specs/skill-system-contract/spec.md).
