# Alan

**Alan is a programmable personal computing environment built to be the next computer.**

Alan starts from the premise that the next computer is not just a device people
operate through apps, but an environment where humans and agents can act across
the digital world. Here, "computer" does not mean a bare-metal machine. It means the
complete end-to-end experience: interface, runtime, files, memory, tools,
permissions, sessions, local state, and native clients designed as one closed
loop. Alan starts local-first, so the agent can see context, take governed
action, remember durable facts, recover work, and collaborate with the human in
the same workspace.

At the runtime layer, `alan` is built around the **AI Turing Machine** metaphor:
a state machine where LLMs drive transitions while the runtime manages tape
(context), tooling, governance, and persistence.

> **⚠️ Project Status: Early Development**
>
> This project is actively being developed. APIs may change without notice.
>
> Governance model note: this README summarizes governance at a high level.
> The authoritative current implementation contract lives in
> `docs/governance_current_contract.md`; target-design material lives in
> OpenSpec.

---

## Component Names

Use these names consistently across specs, docs, crates, and UI copy:

| Name | Meaning |
| --- | --- |
| **Alan** | The product: a programmable personal computing environment. "Programmable environment" is the category, not a second product name. |
| **Alan OS** | The operating-system boundary for Alan: Alan Kernel, Host Service APIs, System Agent Supervisor, Agent Capability Service, app/capability registry, hosts, and app integration conventions. It is not the daemon, CLI, TUI, macOS app, or agent engine. |
| **Alan Kernel** / `alan-kernel` | The semantic substrate inside Alan: objects, commands, buffers, views, tasks, actors, permissions, ledgers, projections, and app/capability registry. |
| **Host Service APIs** | Abstract Alan OS service interfaces for storage, credentials, connection profiles, process/runtime supervision, event transport, scheduling/reconnect, policy, skill catalogs, and OS integration. |
| **Host Service Implementations** | Concrete providers of Host Service APIs. Today the Alan daemon implements or bridges many of them; future Alan for macOS may embed some; tests may use in-memory providers. |
| **Alan Daemon** / `alan daemon` | The current local process/gateway exposing HTTP/WS and implementing or bridging Host Service APIs. It is not Alan OS and not an app backend. |
| **Alan CLI** / `alan` | The operator/client entrypoint for configuration, daemon control, local commands, and developer workflows. |
| **Alan App** | A registered app module with manifest, object types, commands, task types, buffers/views, projections, required Host Service APIs, and exported capabilities. |
| **Capability** | A typed ability exported by Alan OS or an Alan App, invoked through command, permission, and audit paths. |
| **System Agent Supervisor** | The always-available Alan OS agent supervisor with long-lived identity, memory, system awareness, and cross-app continuity. It supervises scoped Agent Runs; it is not a root chat session and not the Alan Agent UI. |
| **Agent Capability** | A standard Alan OS ability that lets Alan Apps request scoped AI-mediated reading, planning, transformation, delegation, or action through typed Context Grants and Result Contracts without depending on the Alan Agent UI. |
| **Agent Capability Service** | The Host Service API that starts, schedules, streams, yields, resumes, cancels, and completes Agent Runs using provider, runtime, memory, and execution implementations outside Alan Kernel. |
| **Agent Run** | A bounded execution of an Agent Capability owned by the requesting app, object, or task by default, with System Agent Supervisor continuity when allowed. |
| **Alan Agent** | The built-in Agent Workspace Alan App for inspecting, steering, and organizing agent sessions, Agent Runs, supervisor-raised tasks, memory, evidence, and cross-app work. It is not the System Agent Supervisor or Agent Capability Service; it uses an internal Agent Execution Engine and projects work into Alan Kernel semantics. |
| **Agent Execution Engine** / `alan-runtime` | The current internal execution engine used by Alan Agent: tape, session loop, tools, skills, policy, memory, and persistence. It is not Alan OS or Alan Kernel. Future crate name: `alan-agent-engine`. |
| **Alan for macOS** | The native Apple host for Alan: renderer, input shell, windowing, and OS integration surface. |
| **Alan TUI** / `alan-terminal-ui` | The terminal host for Alan, currently implemented with Ratatui in `crates/tui`. |
| **Alan Agent App Module** / future `alan-agent` | The durable module boundary for the Alan Agent workspace projections, commands, buffers, views, tasks, artifacts, evidence, and compatibility adapters. |
| **Alan Terminal Renderer** / future `alan-terminal-renderer` | Optional extracted renderer/input adapter for the terminal host. Prefer folding terminal renderer work into `alan-terminal-ui` until an independent crate boundary is justified. |
| **Alan Apps** | Apps such as Alan Agent and Groove Master that run on Alan OS with app-owned domain cores and Alan adapters. |

---

## Architecture Premise: AI Turing Machine

At its core, `alan` models AI agents as **Turing machines**: LLM generation is the transition
function, the tape is the conversation/context state, and tools are the side
effects. That computation model sits inside a separate hosting model:

| Hosting Concept    | Role                               | Analogy                   |
| ------------------ | ---------------------------------- | ------------------------- |
| **AgentRoot**      | On-disk agent definition           | Executable + config root  |
| **Workspace**      | Persistent identity and context    | Filesystem + home         |
| **AgentInstance**  | Running agent process              | Process                   |
| **Session**        | Bounded execution within an agent  | A task/run inside a proc  |

`HostConfig` holds machine-local daemon/client settings under `~/.alan/host.toml`.
`SpawnSpec` is the future explicit child-agent launch contract. Runtime-internal
types such as `AgentConfig` are derived from resolved agent roots; they are not
the primary user-facing hosting abstraction.

> 📖 **[Full Architecture Documentation →](docs/architecture.md)**
>
> 📚 **[Docs Index →](docs/README.md)**

### Design Principles

1. **Alan-Owned OS** — Alan OS semantics live in Alan Kernel, Host Service APIs, and standard OS abilities such as Agent Capability; `alan-runtime` is the internal Agent Execution Engine for Alan Agent
2. **Checkpointed Reasoning** — Every thought, action, and observation is durably recorded
3. **Separation of Concerns** — Agent Execution Engine handles agent state transitions; the `alan` binary is the current CLI/daemon implementation and gateway
4. **Skills over Plugins** — Capabilities are Markdown-based instructions, not compiled code
5. **Human-in-the-End** — Humans own outcomes, not operations ([docs →](docs/README.md))

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Clients                              │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐                  │
│  │ Alan TUI │  │  macOS   │  │   API    │                   │
│  │  (Rust)  │  │ (SwiftUI)│  │ (HTTP/WS)│                   │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘                  │
└───────┼─────────────┼─────────────┼─────────────────────────┘
        │             │             │
        └─────────────┴─────────────┘
                      │
              ┌───────▼────────┐
              │      alan      │  ← current Host Service Implementation
              │ CLI + daemon   │     and HTTP/WS gateway
              └───────┬────────┘
                      │ manages
        ┌─────────────┼─────────────┐
        │             │             │
   ┌────▼─────┐ ┌────▼─────┐ ┌────▼─────┐
   │  Agent   │ │  Agent   │ │  Agent   │  ← Running instances bound to workspaces
   │Instance 1│ │Instance 2│ │Instance N│
   └────┬─────┘ └────┬─────┘ └────┬─────┘
        │             │             │ each runs
        └─────────────┴─────────────┘
                      │
              ┌───────▼───────┐
              │  alan-runtime │  ← Alan Agent execution engine
              └───────┬───────┘
                      │
        ┌─────────────┼──────────────────┐
        │             │            │     │
   ┌────▼────┐  ┌─────▼─────┐ ┌───▼──┐ ┌▼────────┐
   │  alan   │  │   alan-   │ │alan  │ │  Tools  │
   │  -llm   │  │ protocol  │ │-tools│ │ (trait) │
   └─────────┘  └───────────┘ └──────┘ └─────────┘
```

---

## Project Structure

```
alan/
├── crates/
│   ├── auth/         # Managed auth storage and ChatGPT/Codex login support
│   ├── protocol/     # Event/Op protocol definitions + ContentPart
│   ├── llm/          # LLM provider adapters (ChatGPT/Codex, OpenAI, Gemini, Anthropic, OpenRouter)
│   ├── runtime/      # Agent Execution Engine: tape, session, agent loop, skills, SWE-bench tooling
│   ├── tools/        # Builtin tool implementations
│   ├── tui/          # Rust inline terminal UI
│   ├── alan-kernel/   # Alan Kernel substrate primitives
│   └── alan/         # Alan CLI + daemon: current host-service implementation/gateway
├── clients/
│   └── apple/        # Native Apple client (SwiftUI, macOS/iOS)
└── docs/             # Architecture, contracts, maintainer notes, testing strategy
```

### Crates

| Crate                  | Role                                                                |
| ---------------------- | ------------------------------------------------------------------- |
| `alan-auth`            | Managed credential storage and ChatGPT/Codex login helpers          |
| `alan-protocol`        | Wire format — Events (output), Operations (input), ContentPart      |
| `alan-llm`             | Pluggable LLM adapters — ChatGPT/Codex managed Responses surface, OpenAI Responses API, OpenAI Chat Completions API, OpenAI Chat Completions API-compatible, Google Gemini GenerateContent API, Anthropic Messages API, and OpenRouter SDK-backed chat |
| `alan-runtime`         | Agent Execution Engine — session, tape, agent loop, tool registry, skills; future `alan-agent-engine` |
| `alan-swebench-tooling` | SWE-bench workspace and suite materialization helpers               |
| `alan-tools`           | Builtin tool implementations (`read_file`, `bash`, `grep`, etc.)    |
| `alan-kernel`          | Alan Kernel substrate primitives: objects, commands, buffers, views, tasks, actors, ledgers, projections, app/capability registry |
| `alan`                 | Public CLI + daemon; current Host Service API implementation/gateway — workspace lifecycle, HTTP/WS API, ask, chat |

---

## Features

- **Multi-Provider LLM**: ChatGPT/Codex managed Responses surface, OpenAI Responses API, OpenAI Chat Completions API, OpenAI Chat Completions API-compatible, Google Gemini GenerateContent API, Anthropic Messages API, OpenRouter
- **Streaming Responses**: Real-time token streaming with tool call support
- **Layered Tool Profiles**:
  - Core (default): `read_file`, `write_file`, `edit_file`, `bash`
  - Read-only exploration: `read_file`, `grep`, `glob`, `list_dir`
  - All built-ins: core + exploration tools (7 total)
- **Skill System**: Markdown-based capability packages with public Codex/Claude-compatible `SKILL.md` portability, explicit activation, implicit catalog listing, progressive disclosure, and delegated child-agent execution
- **Capability-Package Hosting**: Built-in first-party packages, agent-root `skills/` directories, and public `.agents/skills/` installs resolve into one `ResolvedCapabilityView`; packages can expose portable skills, child-agent roots, and resource directories without requiring `package.toml`
- **Skill Management Surface**: daemon APIs expose the local skill catalog, change polling, and skill override writes
- **Session Persistence**: Rollout recording with history reads, reconnect snapshots, resume, fork, rollback, and compaction hooks
- **HITE Governance**: Humans define boundaries, policy decides (`allow/deny/escalate`), and the current execution backend reports its best-effort local guard as `execution_backend`; see `docs/governance_current_contract.md` for exact guard behavior
- **Policy Profiles**: Builtin `autonomous`/`conservative` presets, overridable via `policy.yaml` in the resolved agent-root chain
- **Steering-First Execution**: In-turn `input` can interrupt tool batches and reprioritize the next step
- **WebSocket + HTTP API**: Real-time session communication plus connection, skill catalog, and relay control surfaces
- **Shell Control Surface**: `alan shell` IPC commands expose native shell state, spaces, tabs, panes, attention, routing, and events
- **Context Compaction**: Automatic summarization when context grows large
- **Thinking Support**: Optional reasoning/thinking display with canonical named effort control
- **Session Rollback**: Undo last N turns within a session

---

## Thinking / Reasoning Support

alan exposes `model_reasoning_effort` as the canonical runtime config control.
The old public `thinking_budget_tokens` field has been removed; provider-native
budgets are derived internally from named effort presets when a provider requires
budget-shaped wire fields. Current provider behavior:

- **Anthropic Messages API**: native thinking blocks, thinking signature, and redacted thinking blocks; named effort maps to provider budget presets
- **ChatGPT/Codex managed Responses surface**: preserves reasoning text, signatures, response metadata, and cached token usage when available; named effort maps to managed request controls
- **OpenAI Responses API**: preserves thinking metadata when available and maps named effort to `reasoning.effort`
- **OpenAI Chat Completions API**: preserves thinking metadata when available and maps named effort to `reasoning_effort`
- **OpenAI Chat Completions API-compatible**: chat-completions-compatible path with reasoning field support (for example `reasoning_content` and reasoning metadata)
- **OpenRouter**: SDK-backed chat adapter that preserves OpenRouter reasoning and reasoning-detail metadata when available and maps named effort to provider-native reasoning controls
- **Google Gemini GenerateContent API**: maps Gemini 3 effort to `thinkingLevel` and Gemini 2.5 effort to `thinkingBudget`

Notes:

- `model_reasoning_effort = "medium"` is the preferred config shape when the
  selected model supports named effort.
- Existing `thinking_budget_tokens` config is rejected; replace it with the
  closest supported `model_reasoning_effort` value.
- Alan TUI and daemon event APIs surface thinking deltas when the selected provider emits them.

---

## Quick Start

### Prerequisites

- Rust 1.85+ (2024 edition)
- [just](https://github.com/casey/just) (task runner, optional but recommended)

### Building

```bash
git clone <repo-url>
cd alan
cargo build --release

# Or use just
just build
```

### Installation

The supported macOS distribution is app-first. The signed `Alan.app` bundle
contains the `alan` command under `Contents/Resources/bin`.

```bash
# Normal user install
brew install --cask alan

# Local developer install from this checkout
ALAN_DEVELOPER_ID_APPLICATION="Developer ID Application: Example (TEAMID)" just install

# Public release artifact
just release
```

Local release/install scripts load allowlisted signing and notarization settings
from `ALAN_RELEASE_ENV_FILE` when set. Without that override they look for
repo-local env files such as `.env.release.local`, `.env.local`, and `.env`,
then fall back to `~/.alan/release.env`. This supports local variables such as
`ALAN_DEVELOPER_ID_APPLICATION`, `ALAN_NOTARY_KEYCHAIN_PROFILE`,
`APPLE_ID`, `APPLE_TEAM_ID`, and `APPLE_APP_SPECIFIC_PASSWORD` without
committing machine-specific secrets.

Start from the checked-in example:

```bash
cp .env.example .env
```

For fully automated notarization, keep `ALAN_NOTARY_KEYCHAIN_PROFILE` in `.env`
and add Apple ID app-specific password credentials:

```bash
ALAN_NOTARY_KEYCHAIN_PROFILE=alan-notary
APPLE_ID=your-apple-id@example.com
APPLE_TEAM_ID=TEAMID
APPLE_APP_SPECIFIC_PASSWORD=xxxx-xxxx-xxxx-xxxx
```

`just release-check` verifies the setup without building. `just release`
creates or refreshes the notary keychain profile automatically before
submitting the release artifact.

Homebrew links the embedded `alan` binary into its prefix.
When installing `Alan.app` directly, use **Tools > Install Command Line
Tools...** in the app to create PATH-visible symlinks. `~/.alan/bin` is not a
supported install location.

Directly installed release apps update through **Check for Updates...** using
Sparkle and `https://alanworks.app/appcast.xml`. Homebrew cask installs stay
under Homebrew ownership and should be updated with:

```bash
brew upgrade --cask alan
```

The release zip remains a GitHub Release asset. `alanworks.app` hosts the
website and appcast only; it must not host `alan-<version>-macos.zip`.
See [macOS Auto Update](docs/macos_auto_update.md) for the appcast and
Cloudflare Pages flow.

### Configuration

The recommended setup path is launching bare `alan` and using the
first-run wizard. The wizard starts with user-facing service presets such as
ChatGPT/Codex login, OpenAI API Platform, OpenRouter, Kimi Coding, DeepSeek,
Google Gemini via Vertex AI, and Anthropic API. Raw API-family selection is kept
behind `Advanced / custom setup`.

Connection/provider metadata lives in `~/.alan/connections.toml`; Agent Execution
Engine knobs live in `~/.alan/agents/default/agent.toml`. An agent config can pin a
profile with `connection_profile`, otherwise alan resolves a workspace pin or
the global `default_profile`.

```toml
# ~/.alan/connections.toml
version = 1
default_profile = "chatgpt-main"

[credentials.chatgpt]
kind = "managed_oauth"
provider_family = "chatgpt"
label = "ChatGPT login"
backend = "alan_home_auth_json"

[profiles.chatgpt-main]
provider = "chatgpt"
label = "ChatGPT/Codex"
credential_id = "chatgpt"
source = "managed"

[profiles.chatgpt-main.settings]
base_url = "https://chatgpt.com/backend-api/codex"
model = "gpt-5.3-codex"
account_id = ""
```

API-key profiles use the same file shape and are managed through
`alan connection`:

```bash
alan connection add openai_responses --profile openai-main --setting model=gpt-5.4
alan connection set-secret openai-main
alan connection add openrouter --profile openrouter-main --setting model=moonshotai/kimi-k2.6
alan connection set-secret openrouter-main
```

```toml
# ~/.alan/agents/default/agent.toml

# Optional explicit pin
# connection_profile = "chatgpt-main"

llm_request_timeout_secs = 180
tool_timeout_secs = 30

# Optional skill exposure overrides
[[skill_overrides]]
skill = "plan"
allow_implicit_invocation = false

[[skill_overrides]]
skill = "release-checklist"
enabled = false

# Optional explicit compaction budgeting override
# By default alan derives this from its model catalog.
# context_window_tokens = 128000
# Deprecated hard-threshold alias:
# compaction_trigger_ratio = 0.8
# Preferred dual-threshold form:
# compaction_soft_trigger_ratio = 0.72
# compaction_hard_trigger_ratio = 0.8

# Thinking / reasoning (optional)
model_reasoning_effort = "medium"
```

Host-facing daemon/client settings live in `~/.alan/host.toml`. You can also set
`ALAN_CONFIG_PATH` to use a custom agent config file location.

### AgentRoot Layout

alan resolves an agent definition from on-disk `AgentRoot`s:

```text
~/.alan/agents/default/         # global default agent root
~/.alan/agents/<name>/          # global named agent root

<workspace>/.alan/agents/default/ # workspace default agent root
<workspace>/.alan/agents/<name>/  # workspace named agent root
```

Each root may contain:

- `agent.toml`
- `persona/`
- `skills/`
- `policy.yaml`

Resolution order is:

- Default workspace agent: `~/.alan/agents/default -> <workspace>/.alan/agents/default`
- Named agent: `~/.alan/agents/default -> <workspace>/.alan/agents/default -> ~/.alan/agents/<name> -> <workspace>/.alan/agents/<name>`

The former singular default root `.alan/agent/` is not a supported compatibility
path. Move authored files from `.alan/agent/` to `.alan/agents/default/`.

Each resolved root contributes its `skills/` directory as a capability-package
source in the definition layer. alan combines those sources with built-in
first-party packages into one `ResolvedCapabilityView`, and a
standards-compatible skill directory is adapted automatically as a single-skill
package without an alan-specific manifest.

Skill-system contract material now lives in OpenSpec under
`openspec/specs/skill-system-contract/spec.md`.
`docs/skills_and_tools.md` is the current implementation guide, and historical
plan documents must not be treated as specification sources.

alan also supports optional alan-native sidecars inside a skill package:

- `skill.yaml` for skill-specific machine metadata
- `package.yaml` for package-level defaults applied before the skill sidecar

Precedence is `SKILL.md` frontmatter -> `package.yaml` `skill_defaults` ->
`skill.yaml`. Sidecars are fail-open: when absent, alan continues to load the
skill from `SKILL.md` alone, and an invalid sidecar only drops that overlay
instead of poisoning the whole skill package.

alan also recognizes zero-conversion public skill install directories:

- `~/.agents/skills/` for user-wide public skills
- `<workspace>/.agents/skills/` for workspace-local public skills

These directories are scanned into the same package host as single-skill
packages. A resolved package can also expose package-level resources such as
`scripts/`, `references/`, `assets/`, `viewers/`, and child-agent roots under
`agents/`.

At runtime, a resolved skill may execute inline or as a delegated
package-local child-agent run. Detailed execution, fallback, and availability
semantics live in the OpenSpec skill-system contract.

Each root can also override skill exposure explicitly in `agent.toml`:

```toml
[[skill_overrides]]
skill = "plan"
allow_implicit_invocation = false

[[skill_overrides]]
skill = "deploy-checklist"
enabled = false
```

Managed ChatGPT login is now scoped to a connection profile:

```bash
alan connection login chatgpt-main browser
alan connection current --workspace /path/to/workspace
alan connection default set chatgpt-main
```

Stable exposure fields are:

- `enabled`: whether the skill is usable in the current runtime
- `allow_implicit_invocation`: whether the skill appears in the system-prompt
  catalog for model-side on-demand use

Built-in first-party packages are discovered through the same package host as
external skills. They are not auto-injected by default. The first-run setup
wizard writes canonical provider config, and `alan init` creates
`<workspace>/.agents/skills/` as the default zero-conversion install target
for public skills.

Skill frontmatter can also declare runtime requirements such as
`required_tools` or `min_version`. alan now evaluates those constraints when
building the runtime skill catalog and in
`alan skills ...` output, so unavailable skills are surfaced with explicit
reasons instead of silently appearing activatable.

This is definition overlay, not runtime parent-child inheritance.

alan resolves model metadata in this order:

1. Bundled catalog
2. `~/.alan/models.toml`
3. `{workspace}/.alan/models.toml`

Overlay catalogs currently extend `openai_chat_completions_compatible` models only. Official
`openai_responses` and `openai_chat_completions` models stay pinned to alan's curated catalog.

Example overlay:

```toml
[openai_chat_completions_compatible]
[[openai_chat_completions_compatible.models]]
slug = "my-team-model"
family = "my-team"
context_window_tokens = 262144
supports_reasoning = true
```

### CLI Usage

```bash
# Initialize a workspace
alan init

# Start the daemon
alan daemon start              # background (default)
alan daemon start --foreground # foreground
alan daemon stop
alan daemon status

# Manage model/provider profiles and credentials
alan connection list
alan connection current --workspace ./my-project
alan connection add chatgpt --profile chatgpt-main
alan connection login chatgpt-main browser
alan connection add openai_responses --profile openai-main --setting model=gpt-5.4
alan connection set-secret openai-main
alan connection default set chatgpt-main
alan connection test chatgpt-main

# Interactive Alan TUI
alan

# Inspect resolved skills, packages, package exports, and availability
alan skills list
alan skills packages
alan skills init ./my-skill --name my-skill
alan skills validate ./my-skill --strict
alan skills eval ./my-skill

# Inspect or drive a local alan shell host
alan shell state
alan shell space list
alan shell tab list
alan shell pane list
alan shell events --follow

# Workspace management
alan workspace list
alan workspace add ./my-project --name myproj
alan workspace remove myproj
alan workspace info myproj
```

### API Usage

Route ownership lives in `crates/alan/src/daemon/api_contract.rs`; the examples
below show the stable public paths, while production clients should use the
contract helpers or generated TUI helpers.

```bash
# Create a session
# streaming_mode accepts auto | on | off
curl -X POST http://localhost:8090/api/v1/sessions \
  -H "Content-Type: application/json" \
  -d '{
    "workspace_dir": "/path/to/workspace",
    "agent_name": "default",
    "profile_id": "chatgpt-main",
    "reasoning_effort": "medium",
    "governance": {"profile": "autonomous", "policy_path": ".alan/agents/default/policy.yaml"},
    "streaming_mode": "on",
    "partial_stream_recovery_mode": "continue_once"
  }'

# Create a conservative session
curl -X POST http://localhost:8090/api/v1/sessions \
  -H "Content-Type: application/json" \
  -d '{"governance": {"profile": "conservative"}}'

# Create response sample fields
# {
#   "session_id": "...",
#   "websocket_url": "/api/v1/sessions/.../ws",
#   "events_url": "/api/v1/sessions/.../events",
#   "submit_url": "/api/v1/sessions/.../submit",
#   "agent_name": "default",
#   "governance": {...},
#   "execution_backend": "workspace_path_guard",
#   "streaming_mode": "on",
#   "partial_stream_recovery_mode": "continue_once",
#   "profile_id": "chatgpt-main",
#   "provider": "chatgpt",
#   "resolved_model": "gpt-5.3-codex",
#   "reasoning_effort": "medium",
#   "durability": {"durable": false, "required": false}
# }
# Note: 409 returned when the workspace already has an active runtime.

# Read session metadata + persisted messages
curl http://localhost:8090/api/v1/sessions/{id}/read

# Read persisted message history only
curl http://localhost:8090/api/v1/sessions/{id}/history

# Read reconnect handoff state for TUI/mobile recovery
curl http://localhost:8090/api/v1/sessions/{id}/reconnect_snapshot

# Inspect delegated child-agent runs
curl http://localhost:8090/api/v1/sessions/{id}/child_runs

# Poll events from rollout gap-aware API
curl "http://localhost:8090/api/v1/sessions/{id}/events/read?after_event_id=e-123&limit=50"

# Response includes:
# {
#   "session_id": "...",
#   "gap": false,
#   "oldest_event_id": "e-100",
#   "latest_event_id": "e-123",
#   "events": [...]
# }

# Submit user input
curl -X POST http://localhost:8090/api/v1/sessions/{id}/submit \
  -H "Content-Type: application/json" \
  -d '{"op": {"type": "turn", "parts": [{"type": "text", "text": "Hello!"}]}}'

# Runtime recovery and control
curl -X POST http://localhost:8090/api/v1/sessions/{id}/resume
curl -X POST http://localhost:8090/api/v1/sessions/{id}/fork
curl -X POST http://localhost:8090/api/v1/sessions/{id}/compact \
  -H "Content-Type: application/json" \
  -d '{"focus": "preserve open todos and file paths"}'
curl -X POST http://localhost:8090/api/v1/sessions/{id}/rollback \
  -H "Content-Type: application/json" \
  -d '{"turns": 1}'
curl -X POST http://localhost:8090/api/v1/sessions/{id}/schedule_at \
  -H "Content-Type: application/json" \
  -d '{"wake_at": "2026-06-24T09:00:00Z"}'

# Connection profile control plane
curl http://localhost:8090/api/v1/connections/catalog
curl http://localhost:8090/api/v1/connections
curl http://localhost:8090/api/v1/connections/current
curl -X POST http://localhost:8090/api/v1/connections/default/set \
  -H "Content-Type: application/json" \
  -d '{"profile_id": "chatgpt-main"}'
curl -X POST http://localhost:8090/api/v1/connections/{profile_id}/credential/login/browser/start
curl -X POST http://localhost:8090/api/v1/connections/{profile_id}/test

# Skill catalog and override APIs
curl http://localhost:8090/api/v1/skills/catalog
curl "http://localhost:8090/api/v1/skills/changed?after=<cursor>"
curl -X POST http://localhost:8090/api/v1/skills/overrides \
  -H "Content-Type: application/json" \
  -d '{"skill_id": "memory", "allowImplicitInvocation": false}'

# Relay discovery and proxy control
curl http://localhost:8090/api/v1/relay/nodes

# Stream events (NDJSON)
curl -N http://localhost:8090/api/v1/sessions/{id}/events
```

### Policy Configuration (Optional)

Create `{workspace}/.alan/agents/default/policy.yaml` to override builtin policy profile rules.
When present, the file replaces the builtin profile rule set for that session;
alan does not implicitly merge policy files with builtin rules.

```yaml
rules:
  - id: deny-prod-delete
    tool: bash
    match_command: "kubectl delete"
    action: deny
    reason: protect production cluster

  - id: review-prod-deploy
    tool: bash
    match_command: "deploy --prod"
    action: escalate
    reason: explicit production boundary

default_action: allow
```

See [`docs/governance_current_contract.md`](docs/governance_current_contract.md)
for the current implementation contract. HITE target-design material now lives
in OpenSpec under
`openspec/specs/governance-tooling-contract/spec.md`.

---

## Contributing

If you want to contribute, start with:

- [CONTRIBUTING.md](CONTRIBUTING.md)
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)
- [SECURITY.md](SECURITY.md)
- [SUPPORT.md](SUPPORT.md)

---

## Development

```bash
just check          # format + lint + test
just fmt            # format code
just lint           # clippy
just test           # run all tests
just smoke          # mock smoke tests (no LLM needed)
just smoke-e2e      # TUI/daemon smoke path
just live-providers # live provider checks (needs credentials)
just verify         # fmt + lint + test + smoke (run after code changes)
just verify-full    # verify + real LLM e2e test (needs ~/.alan config)
just harness-autonomy-ci     # autonomy harness gate
just harness-repo-worker-ci  # repo-worker harness gate
just harness-compaction-ci   # compaction harness gate
just self-eval-ci            # structured self-eval gate
just coverage       # test coverage summary
just serve          # run the daemon in foreground
```

Run `just --list` for the full local gate and release command surface.

---

## Inspirations

- [Claude Code](https://claude.ai) — human-style reasoning and collaboration
- [Codex](https://openai.com/blog/openai-codex) — intelligence expressed through code
- [pi-mono](https://github.com/badlogic/pi-mono/) — minimal agent runtime design
- **Turing Machine** — computation as state transitions on a tape

---

## License

Apache License 2.0 — See [LICENSE](LICENSE) for details.
