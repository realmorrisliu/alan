# alan Architecture — The AI Turing Machine

> Status: this document tracks the current architecture plus the accepted V2
> governance direction.
>
> Current governance semantics are defined in
> [`governance_current_contract.md`](./governance_current_contract.md). When this
> document discusses target HITE governance or optional stronger containment,
> treat that as target-state design rather than a statement about today's
> implementation.

## Philosophy

alan models AI agents as **Turing machines**: LLM generation is the transition
function, the tape holds bounded conversational state, and tools are the side
effects. That computation model is intentionally separate from alan's hosting
model, which distinguishes on-disk agent definitions, persistent workspaces,
running agent instances, and bounded sessions.

Companion OpenSpec contracts and migration owners:

- [`runtime-core-contract`](../openspec/specs/runtime-core-contract/spec.md)
- [`runtime-memory-contract`](../openspec/specs/runtime-memory-contract/spec.md)
- [`governance-tooling-contract`](../openspec/specs/governance-tooling-contract/spec.md)
- [`coding-steward-contract`](../openspec/specs/coding-steward-contract/spec.md)
- [`skill-system-contract`](../openspec/specs/skill-system-contract/spec.md)
- [`daemon-api-contract`](../openspec/specs/daemon-api-contract/spec.md)
- [`agent-root-layout`](../openspec/specs/agent-root-layout/spec.md)
- [`workspace-runtime-state-hygiene`](../openspec/specs/workspace-runtime-state-hygiene/spec.md)

---

## Hosting + Computation Model

```
┌─────────────────────────────────────────────────────────┐
│  HostConfig                                             │
│  Machine-local host settings (`~/.alan/host.toml`)      │
├─────────────────────────────────────────────────────────┤
│  AgentRoot                                              │
│  On-disk definition: agent.toml, persona, skills, policy│
├─────────────────────────────────────────────────────────┤
│  Workspace                                              │
│  Persistent identity, memory, sessions, workspace state │
├─────────────────────────────────────────────────────────┤
│  AgentInstance                                          │
│  Running process bound to a resolved agent definition   │
├─────────────────────────────────────────────────────────┤
│  Session                                                │
│  Bounded tape + rollout for the current task            │
└─────────────────────────────────────────────────────────┘
```

`SpawnSpec` is the explicit child-agent launch contract that will connect
agent-instance supervision with future multi-agent execution. Runtime-internal
types such as `AgentConfig` still exist, but they are derived from resolved
agent roots rather than serving as alan's primary user-facing hosting model.

### AgentRoot — The On-Disk Definition

An **AgentRoot** is the filesystem form of an agent definition. alan resolves one
effective agent by overlaying multiple roots.

```text
~/.alan/agents/default/                   # global default agent definition root
~/.alan/agents/<name>/           # global named agent definition root
<workspace>/.alan/agents/default/         # workspace default agent definition root
<workspace>/.alan/agents/<name>/ # workspace named agent definition root
```

All default and named agent definitions live under `agents/`. The reserved
`default/` child is selected when `agent_name` is omitted or set to `default`.
Other child directories are named definition roots selected by `agent_name`. A
named agent extends the default roots rather than replacing them.

Each root may contain:

- `agent.toml`
- `persona/`
- `skills/`
- `policy.yaml`

Workspace `.alan` contains both authored agent definition files and generated
runtime state. The repository ignores workspace `.alan/*` by default, then
explicitly allows authored roots under `.alan/agents/` and
workspace model overlays like `.alan/models.toml` to remain source-controlled.
Generated runtime files live under a channel namespace such as
`.alan/runtime/stable/` or `.alan/runtime/dev/`; they are local
continuation/debugging state and stay ignored by default. Stable Alan can still
read legacy generated workspace state from `.alan/sessions/` and `.alan/memory/`
for compatibility, but Alan Dev must not write those legacy stable paths.

Overlay order is:

- Default workspace agent: `~/.alan/agents/default -> <workspace>/.alan/agents/default`
- Named agent: `~/.alan/agents/default -> <workspace>/.alan/agents/default -> ~/.alan/agents/<name> -> <workspace>/.alan/agents/<name>`

The former singular default root `.alan/agent/` is removed from the runtime
contract. It is not read as a fallback and is not merged with
`.alan/agents/default/`; move authored files to the `default/` root.

Rust code treats this layout as a runtime-owned contract. The canonical API is
`alan_runtime::AgentRootLayout`, with semantic helpers for default roots, named
roots, `agent.toml`, `persona/`, `skills/`, and `policy.yaml`. Host crates such
as `alan` should call that API for reads and writes instead of joining
`agents/default` path segments locally. TypeScript setup code may keep a small
offline mirror for first-run setup, but online flows should prefer paths returned
by the daemon.

This overlay chain defines an agent. It is not runtime process ancestry, and it
is distinct from delegated child-agent runs created during a session.

### Capability Packages In The Definition Layer

For the authoritative skill-system contract, see
[`skill-system-contract`](../openspec/specs/skill-system-contract/spec.md).
For the current implementation guide, see [`skills_and_tools.md`](./skills_and_tools.md).
This section keeps only the architecture-level summary so the detailed behavior
does not drift in multiple places.

Each resolved `AgentRoot` contributes its `skills/` directory as a capability
package source. alan also adapts `~/.agents/skills/` and
`<workspace>/.agents/skills/` as public single-skill package sources for the
global and workspace default layers. alan combines those root-backed and public
sources with built-in first-party packages into one `ResolvedCapabilityView`,
which is then consumed by runtime instead of the older mixed
`repo/user/builtin` skill-loading paths.

A standards-compatible skill directory with `SKILL.md` and optional supporting
resources is adapted automatically as a single-skill package. Directory-backed
packages currently expose one portable skill plus optional alan-native
launch targets from `agents/` and resource directories such as `scripts/`,
`references/`, and `assets/`. Package hosting therefore stays in
the definition layer without requiring an alan-specific manifest for every
public skill directory.

Each root can then expose skills through explicit `skill_overrides` in
`agent.toml`. The stable runtime exposure fields are:

- `enabled`
- `allow_implicit_invocation`

Runtime consumes the resolved skill-level exposure state instead of inferring
activation from legacy scope-specific loading paths or package-level mount
modes.

Built-in first-party packages are no longer always active by default. Any
baseline behavior alan requires unconditionally must live in the base prompt,
tool descriptions, or dedicated runtime policy.

At runtime, those resolved skills may execute inline or delegate to
package-local launch targets, but the execution contract itself lives in
OpenSpec rather than this architecture summary.

Skill frontmatter/runtime requirement data is enforced at the runtime boundary. When
`required_tools` or `min_version` constraints are not met, the package remains
in the resolved definition view, but its skills are reported as unavailable in
both prompt assembly and `alan skills` inspection surfaces.

### Workspace — The Persistent Context

A **Workspace** is the persistent, stateful context in which an agent operates.
It gives the resolved agent definition its identity, memory, and working
environment.

```rust
pub struct WorkspaceRuntimeConfig {
    pub agent_config: AgentConfig,           // resolved runtime config from AgentRoot overlays
    pub workspace_id: String,                // identity
    pub workspace_root_dir: Option<PathBuf>, // workspace root used for tool cwd
    pub workspace_alan_dir: Option<PathBuf>, // `.alan` state directory
    pub resume_rollout_path: Option<PathBuf>, // session restore point
}
```

**Workspace directory layout:**

```
{home}/.alan/
├── agents/
│   ├── default/
│   │   ├── agent.toml      # global default agent config
│   │   ├── persona/        # global default persona overlays
│   │   ├── skills/         # global default skills
│   │   └── policy.yaml     # optional global default policy override
│   └── <name>/
│       ├── agent.toml      # global named agent config
│       ├── persona/
│       ├── skills/
│       └── policy.yaml
├── host.toml               # daemon/client host config
├── models.toml             # optional global model overlay catalog
├── sessions/
│   └── <session-id>.json   # daemon session bindings (workspace + governance)

{workspace_root}/.alan/
├── state.json              # workspace state (status, config, current session), when persisted
├── agents/
│   ├── default/
│   │   ├── agent.toml      # workspace default agent config
│   │   ├── persona/        # workspace default persona overlays
│   │   ├── skills/         # workspace default skills
│   │   └── policy.yaml     # optional workspace default policy override
│   └── <name>/
│       ├── agent.toml      # workspace named agent config
│       ├── persona/
│       ├── skills/
│       └── policy.yaml
├── runtime/
│   ├── stable/
│   │   ├── memory/
│   │   │   └── MEMORY.md   # stable generated long-term knowledge
│   │   ├── sessions/
│   │   │   └── rollout-*.jsonl
│   │   ├── cache/
│   │   ├── shell-restore/
│   │   ├── metadata/
│   │   └── tmp/
│   └── dev/
│       ├── memory/
│       ├── sessions/
│       ├── cache/
│       ├── shell-restore/
│       ├── metadata/
│       └── tmp/

{workspace_root}/.alan/sessions/      # legacy stable rollout location, read-compatible
{workspace_root}/.alan/memory/        # legacy stable memory location, read-compatible
```

Public skill install targets live alongside the alan state roots:

```text
{home}/.agents/skills/            # user-wide public skills
{workspace_root}/.agents/skills/  # workspace-local public skills
```

**Key properties:**
- **Persistent** — survives restarts, maintains identity across sessions
- **Self-contained** — workspace state and tool state live under the workspace `.alan` directory; session bindings are tracked by daemon metadata
- **Composable** — different Agents can be mounted into the same Workspace

### AgentInstance — The Running Process

An **AgentInstance** is the running runtime process bound to one resolved agent
definition and one workspace at a time.

**Key properties:**
- **Fresh launch semantics** — startup is derived from the resolved definition, not from hidden parent prompt inheritance
- **Supervised by the host layer** — lifecycle is owned by the daemon/CLI layer, not by `alan-runtime` alone
- **Distinct from overlay resolution** — parent/child instance relations are runtime supervision, not definition ancestry
- **Spawned through `SpawnSpec`** — child instances start from an explicit
  launch contract with bounded handles, runtime overrides, and one-shot
  join/cancel/result semantics

### Session — The Computation

A **Session** is a single, bounded execution inside an `AgentInstance`. It
represents one conversation or task, limited by the LLM's context window.

**Key properties:**
- **Bounded** — constrained by the context window; when full, start a new session
- **Archivable** — daemon may detach the runtime while retaining session
  metadata and rollout bindings, so inactive sessions remain readable and
  resumable for replay or forking
- **One active runtime per workspace** at any time; daemon may still retain
  multiple inactive archived session bindings
- **Split live vs durable tool payloads** — the active tape may hold full tool
  results for current-turn reasoning, while persisted rollout records store a
  redacted/truncated durable projection

The compatibility session APIs surface this distinction through `active`.
`active=false` means the session still exists as retained daemon metadata, but
no live runtime is currently attached. TTL cleanup archives sessions by flipping
them into that inactive retained state; explicit delete is the destructive path.

---

## Policy Model (HITE Governance V2)

alan uses policy-as-code as the only decision layer for tool governance.

1. **Policy gate (`PolicyEngine`)**: per-call decision `allow | deny | escalate` based on tool name, capability, and command patterns.
2. **Execution backend**: the current `workspace_path_guard` backend is a best-effort execution guard for workspace paths and shell shape checks, not a strict OS sandbox. Daemon session APIs surface this as `execution_backend`.

Response guardrails sit after generation but before assistant text emission.
When runtime already knows a draft is contradictory to session capabilities
(for example, claiming that current/external data cannot be checked while a
network-capable tool is available), it may regenerate once before emitting any
user-visible assistant text.

`escalate` always maps to `Event::Yield` and waits for `Op::Resume`. There is no `approval_policy` downgrade branch.

Strong containment is optional defense in depth, not alan's primary HITE
control plane. Owner-local governance should remain coherent even when only the
lightweight built-in backend is available.

Policy file resolution is:

1. `governance.policy_path`, if provided
2. the highest-precedence existing `policy.yaml` in the resolved `AgentRoot` chain
3. builtin profile defaults

When a policy file is found, it replaces the builtin profile rule set for that session. There is no implicit merge with builtin rules.

Detailed current behavior: [`governance_current_contract.md`](./governance_current_contract.md).  
Target V2 design: [`governance-tooling-contract`](../openspec/specs/governance-tooling-contract/spec.md).

---

## Turing Machine Mapping

| TM Concept              | alan Implementation                                          |
| ----------------------- | ------------------------------------------------------------ |
| **Program**             | Resolved `AgentRoot` definition consumed as runtime config   |
| **Tape**                | `Tape` — messages, context items, conversation summary       |
| **Head**                | Current turn — reads tape, produces output                   |
| **Transition Function** | LLM generation — maps (state, input) → (action, new state)   |
| **State**               | `Session` — holds tape, tools, skills, and runtime config    |
| **Machine**             | `AgentInstance` running against a `Workspace`                |
| **Alphabet**            | Messages (user/assistant/tool) and tool calls                |
| **Halt**                | No more tool calls, final text response emitted              |

---

## System Architecture

```
┌─────────────────────────────────────────────────────────┐
│                        Clients                          │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐              │
│  │   TUI    │  │  Native  │  │   API    │              │
│  │  (Bun)   │  │ (SwiftUI)│  │ (HTTP/WS)│              │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘              │
└───────┼─────────────┼─────────────┼─────────────────────┘
        └─────────────┴─────────────┘
                      │
              ┌───────▼─────────────────────────┐
              │         alan daemon             │  ← Workspace lifecycle & hosting
              │ runtime_manager/session_store   │
              └───────┬─────────────────────────┘
                      │ manages
        ┌─────────────┼─────────────┐
        │             │             │
   ┌────▼─────┐ ┌────▼─────┐ ┌────▼─────┐
   │  Agent   │ │  Agent   │ │  Agent   │  ← Running instances bound to workspaces
   │Instance 1│ │Instance 2│ │Instance N│
   └────┬─────┘ └────┬─────┘ └────┬─────┘
        │             │             │ each run
        └─────────────┴─────────────┘
                      │
              ┌───────▼───────┐
              │  alan-runtime │  ← Agent runtime (transition function + tape)
              └───────┬───────┘
                      │
        ┌─────────────┼──────────────────┐
        │             │            │     │
   ┌────▼────┐  ┌─────▼─────┐ ┌───▼──┐ ┌▼────────┐
   │  alan   │  │   alan-   │ │alan  │ │  Tools  │
   │  -llm   │  │ protocol  │ │-tools│ │ (trait) │
   └─────────┘  └───────────┘ └──────┘ └─────────┘
```

### Crate Responsibilities

| Crate           | Role                                                             |
| --------------- | ---------------------------------------------------------------- |
| `alan-protocol` | Wire format — Events (output) and Operations (input)             |
| `alan-llm`      | Pluggable LLM adapters — Google Gemini GenerateContent API, OpenAI Responses API, OpenAI Chat Completions API, OpenAI Chat Completions API-compatible, Anthropic Messages API, and OpenRouter SDK-backed chat |
| `alan-runtime`  | Core engine — session, tape, agent loop, tool registry, skills   |
| `alan-tools`    | Builtin tool implementations (`read_file`, `bash`, `grep`, etc.) |
| `alan`          | Unified CLI + daemon — workspace lifecycle, HTTP/WS API, session mgmt |

---

## Design Principles

1. **Stateless Agent, Stateful Workspace** — Clean separation between reusable computation logic and persistent identity/context.

2. **Checkpointed Reasoning** — Every thought, action, and observation is durably recorded in the session rollout.

3. **Generic Core** — `alan-runtime` is provider-agnostic, domain-agnostic, and hosting-agnostic. The same runtime powers different agents, workspaces, and deployment targets.

4. **Skills-First, Extension-Ready** — Workflow intelligence lives in skills; pluggable system capabilities live in extensions behind stable contracts.

5. **Bounded Sessions** — Context windows are finite. Instead of fighting this constraint, alan embraces it: sessions are discrete, archivable units that can be summarized, forked, and resumed.
