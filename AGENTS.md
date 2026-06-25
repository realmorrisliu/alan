# alan - AI Agent Guide

> **⚠️ Project Status: Early Development**
>
> This project is actively being developed. APIs may change without notice.

---

## Component Names

Use these names consistently in docs, specs, code comments, UI copy, and review
comments:

| Name | Meaning |
| ---- | ------- |
| **Alan** | The product: a programmable personal computing environment. Do not use "programmable environment" as a separate product name. |
| **Alan OS** | The operating-system boundary for Alan: Alan Kernel, file-server system services, Service Manager, Root Agent Process, Agent Runtime Service, hosts, and app integration conventions. It is not the CLI, HTTP/WS compatibility transport, TUI, macOS app, or agent engine. |
| **Alan Kernel** / `alan-kernel` | The file-tree substrate inside Alan: namespace and mounts, paths, files, descriptors, access rights, credentials, process table, and a single `Process` category. Agent-ness is a file-layout convention, not a Kernel type. Streams are file kinds; process output, requests, actions, and events are files. |
| **Standard Namespace** | The canonical Alan OS root layout: `/proc`, `/agent`, `/srv`, `/bin`, `/lib`, `/man`, and `/mnt`. Alan-specific packages and mounted service trees live under `/lib` or `/mnt`, not as new top-level roots by default. |
| **Service Manager** | The Alan OS system Process that starts, stops, restarts, and supervises system services and boot units. It replaces the former daemon as the canonical lifecycle concept. |
| **File-Server Service** | A long-running Process that exports a file tree which other processes mount or bind into their namespace. Alan OS services are file servers, not HTTP APIs. |
| **Service Handle Registry** / `/srv` | The Plan 9-style rendezvous tree where running file servers post mountable handles. `/srv` is not the service state tree. |
| **Agent Runtime Service** | The file-server service that executes Agent Processes and serves AgentFS at `/agent`. It is an internal system service, not an app-facing API. |
| **Process** | A bounded execution with PID, parent, descriptors, credentials, lifecycle, input/output streams, status, and exit state. |
| **Agent Process** | An ordinary Process that runs an agent, recognized by conforming to the agent file-layout — not a separate Kernel type. It lives in `/proc/<pid>` (the source of truth) and is surfaced through the `/agent/<pid>` view. |
| **Root Agent Process** | The always-available Agent Process at the root of the agent process tree, exposed through `/agent/root`. It coordinates child Agent Processes; it is not root permission, the Service Manager, a root chat session, or the Alan Agent UI. |
| **Agent Executable** | An executable that creates an Agent Process when spawned. Agent executables are command files bound into `/bin`, not RPC/API methods. |
| **Tool** | A reusable executable installed into the Alan OS command namespace. Tools provide actions; permissions come from descriptors, access rights, and policy. |
| **Skill** | A manual-like knowledge package installed into the Alan OS namespace and passed to Agent Processes by descriptor. Skills provide understanding; they do not execute. |
| **Memory Stores** | Personal, system-continuity, app, and workspace file trees that own memory authority. Agent memory kinds such as working, episodic, semantic, and procedural describe how memory is used, not who owns it. |
| **Alan Agent** | A built-in but optional Agent Workspace app for inspecting, steering, and organizing Agent Processes. It is not required to run agents and is not the Root Agent Process or Agent Runtime Service. |
| **Agent Execution Engine** / `alan-runtime` | Current implementation of the agent Turing-machine loop: tape, model calls, tool compatibility, skills, policy, memory, and persistence. It backs Agent Runtime Service work; it is not Alan Kernel. Future crate name: `alan-agent-engine`. |
| **Alan for macOS** | Native Apple host for Alan: renderer, input shell, windowing, and OS integration surface. |
| **Alan Shell** / future `alan-shell` | The primary shell for Alan OS: a Plan 9 `rc`-like and Acme-like interaction surface for files, processes, Agent Processes, Tools, Skills, Memory Stores, and services. The current implementation path is Ratatui in `crates/tui`. |
| **Alan Agent App Module** / future `alan-agent` | Optional workspace module that reads agent files (status, io, requests, actions, machine) as a client and renders from those files, not from core-owned view snapshots. |
| **Alan Apps** | Apps such as Alan Agent and Groove Master that run on Alan OS with app-owned domain cores and Alan adapters. |

---

## Architecture Progression Principle

When implementing OpenSpec changes, move the codebase toward the accepted Alan
OS target shape, not just the smallest local patch.

- Prefer directory structure, crate boundaries, module names, and public APIs
  that match the target architecture already recorded in OpenSpec and this
  guide.
- When touching old compatibility code, add or extract the next durable layer
  if it clarifies ownership and reduces future migration risk. For example,
  keep Alan Kernel semantics in `alan-kernel`, Agent Runtime Service and
  AgentFS-oriented execution behind `alan-agent-engine` / `alan-runtime`, Alan
  Shell interaction code in `alan-shell` or `crates/tui` during migration,
  optional Alan Agent workspace code in `alan-agent`, and native host
  integration in Alan for macOS.
- Keep each step scoped to the change being implemented: do not perform broad
  unrelated rewrites, but do leave the touched area with clearer ownership,
  smaller modules, and more readable layering than before.
- Prefer explicit adapter/projection boundaries over leaking HTTP transport,
  TUI, macOS, provider, sandbox, or memory-store details into Alan Kernel or app
  domain models.
- If a first implementation slice starts as a compatibility bridge, make the
  bridge's temporary nature visible in names, docs, and tests so the next slice
  can replace it without rediscovering the intended architecture.

---

## Core Concept: AI Turing Machine

alan treats each Agent Process as a **Turing machine** where the LLM is the transition function:

| TM Concept              | alan Implementation                                        |
| ----------------------- | ---------------------------------------------------------- |
| **Tape**                | `Tape` — messages, context items, and conversation summary |
| **Transition Function** | LLM generation — maps (state, input) → (action, new state) |
| **State**               | Agent Machine state exposed under `/agent/<pid>/machine`   |
| **Alphabet**            | Agent IO, machine events, and Tool process results          |
| **Side Effects**        | Spawning Tools and writing Files through descriptors        |
| **Halt**                | No more tool calls, final text response emitted            |

`alan-runtime` currently implements the Agent Execution Engine that will back
Agent Runtime Service work. It is the generic Turing-machine loop for Agent
Processes, not Alan OS or Alan Kernel. Alan Kernel semantics are namespace and
mounts, paths, files, descriptors, access rights, credentials, a single
`Process` category, and the process table; agent-ness is a file-layout
convention, not a Kernel type. Agent-specific IO, requests, actions, machine
tape, and checkpoints are AgentFS files served above Kernel.
Alan Agent is only an optional Agent Workspace app.

### Hosting And Execution Model

```
┌─────────────────────────────────────────────────────────────┐
│  Agent Executable                                           │
│  Launchable file bound into /bin                            │
│  • executable image + descriptors passed at spawn           │
├─────────────────────────────────────────────────────────────┤
│  Agent Process                                              │
│  Running Process in /proc and /agent/<pid>                  │
│  • descriptors, credentials, lifecycle, IO, requests        │
├─────────────────────────────────────────────────────────────┤
│  Agent Machine                                              │
│  Turing-machine view served by AgentFS                      │
│  • tape, state, machine events, checkpoints                 │
├─────────────────────────────────────────────────────────────┤
│  Agent Workspace                                            │
│  Optional app/view over Agent Processes                     │
│  • inspect, steer, organize, promote work                   │
└─────────────────────────────────────────────────────────────┘
```

Current `Session` and HTTP/WS surfaces are compatibility transport details.
Target Alan OS creation is `spawn` of Agent Executables; target attachment is
opening or watching `/proc` and `/agent` files.

---

## Technology Stack

| Aspect        | Technology                            |
| ------------- | ------------------------------------- |
| Language      | Rust (Edition 2024)                   |
| Build Tool    | Cargo + Just                          |
| Async Runtime | Tokio                                 |
| Web Framework | Axum                                  |
| Serialization | Serde (JSON, YAML, TOML)              |
| Tracing       | tracing, tracing-subscriber           |
| HTTP Client   | reqwest                               |
| LLM Providers | ChatGPT/Codex managed Responses surface, OpenAI Responses API, OpenAI Chat Completions API, OpenAI Chat Completions API-compatible, Google Gemini GenerateContent API, Anthropic Messages API, OpenRouter SDK-backed chat |
| License       | Apache License 2.0                    |

---

## Design Context

### Users

alan's native macOS experience is designed first for the project owner and
developer users who live inside terminal, code, and agent workflows. They need a
real terminal workspace that remains fast to scan, comfortable for long sessions,
and readable by both humans and agents.

The core job is to organize developer work into spaces, tabs, and splits while
making alan available as an optional capability layered onto the terminal rather
than the reason the interface exists.

### Brand Personality

The product personality is calm, precise, and intelligent.

alan should feel confident and quiet: a native tool that understands developer
workflow, removes unnecessary surface area, and exposes power through structure
instead of decoration. The interface should evoke focus, control, and trust
rather than novelty or spectacle.

### Aesthetic Direction

The target visual reference is Arc's macOS browser shell: a translucent,
material-driven sidebar that blends with the desktop background, vertical
space/tab organization, lightweight rows, compact controls, and an expansive
content area. The goal is not a fixed pale-purple sidebar color; the sidebar
should feel like native material interacting with the user's wallpaper and
window environment.

Build light mode first. Dark mode can come later, but the initial design system
should be coherent and polished in light appearance.

The product must not look like VS Code, a traditional terminal chrome, a web app,
a dashboard, or an interface filled with complex/redundant buttons. Avoid
card-heavy layouts, page-like headers, visible implementation jargon, and
debug-first composition.

### Design Principles

1. Terminal first: the active terminal tab is the center of gravity, with alan
   status and debug details kept secondary.
2. Arc-like organization: spaces and tabs live in a native, material sidebar
   built for scanning and switching, not in dashboard sections.
3. Calm precision: prefer fewer controls, restrained typography, subtle
   selection states, and clear hierarchy over decorative emphasis.
4. Native material over flat color: use macOS materials and light-mode surfaces
   that blend with the desktop instead of hard-coded themed panels.
5. Progressive disclosure: default UI hides raw IDs, bindings, runtime phases,
   and diagnostics unless the user opens an explicit debug surface.

---

## Project Structure

```
alan/
├── Cargo.toml                 # Workspace configuration
├── README.md                  # Project overview
├── AGENTS.md                  # This file
├── justfile                   # Development tasks
├── rustfmt.toml               # Code formatting config
├── clippy.toml                # Lint configuration
├── .tarpaulin.toml            # Code coverage config
├── crates/
│   ├── protocol/              # Event/Op protocol (the "alphabet")
│   │   └── src/
│   │       ├── lib.rs         # Re-exports
│   │       ├── event.rs       # Event, EventEnvelope (turn/text/thinking/tool/yield/plan/compaction/error)
│   │       └── op.rs          # Op, Submission, InputMode, GovernanceConfig, ToolCapability
│   │
│   ├── llm/                   # LLM adapters (the "transition function")
│   │   └── src/
│   │       ├── lib.rs         # LlmProvider trait, Message, ToolDefinition (+ MockLlmProvider feature)
│   │       ├── google_gemini_generate_content.rs  # Google Gemini GenerateContent API
│   │       ├── openai_responses.rs
│   │       ├── openai_chat_completions.rs
│   │       ├── openrouter.rs       # OpenRouter SDK-backed chat adapter
│   │       └── anthropic_messages.rs
│   │
│   ├── runtime/               # Agent Execution Engine (current alan-runtime)
│   │   ├── prompts/           # Embedded prompt templates
│   │   │   ├── runtime_base.md
│   │   │   ├── system.md
│   │   │   └── persona/       # Workspace persona templates
│   │   │       ├── AGENTS.md
│   │   │       ├── BOOTSTRAP.md
│   │   │       ├── HEARTBEAT.md
│   │   │       ├── ROLE.md
│   │   │       ├── SOUL.md
│   │   │       ├── TOOLS.md
│   │   │       └── USER.md
│   │   ├── skills/            # Built-in skill/package assets
│   │   │   ├── memory/SKILL.md
│   │   │   ├── alan-shell-control/SKILL.md
│   │   │   ├── plan/SKILL.md
│   │   │   ├── repo-coding/   # First-party repo-scoped coding package
│   │   │   ├── skill-creator/SKILL.md
│   │   │   └── workspace-manager/SKILL.md
│   │   └── src/
│   │       ├── lib.rs         # Public exports
│   │       ├── agent_definition.rs # Resolved agent definitions
│   │       ├── agent_root.rs  # AgentRoot overlay discovery/resolution
│   │       ├── config.rs      # Agent-facing config + connection-profile resolution
│   │       ├── connections.rs # Connection profiles, provider descriptors, secret store
│   │       ├── models.rs      # Model catalog metadata
│   │       ├── paths.rs       # alan home / workspace path helpers
│   │       ├── tape.rs        # Tape (messages, context, compaction)
│   │       ├── session.rs     # Session lifecycle + persistence
│   │       ├── approval.rs    # Pending approval / interaction checkpoint types
│   │       ├── policy.rs      # Policy engine (policy over execution backend)
│   │       ├── rollout.rs     # JSONL persistence format
│   │       ├── llm.rs         # LlmClient wrapper
│   │       ├── retry.rs       # Retry logic with backoff
│   │       ├── manager/
│   │       │   ├── mod.rs
│   │       │   └── state.rs   # WorkspaceConfigState, WorkspaceInfo
│   │       ├── prompts/
│   │       │   ├── assembler.rs
│   │       │   ├── loader.rs
│   │       │   └── workspace.rs
│   │       ├── runtime/       # Agent loop + turn execution
│   │       │   ├── mod.rs     # RuntimeConfig
│   │       │   ├── engine.rs  # spawn(), RuntimeHandle
│   │       │   ├── agent_loop.rs
│   │       │   ├── child_agents.rs
│   │       │   ├── compaction.rs
│   │       │   ├── memory_flush.rs
│   │       │   ├── memory_promotion.rs
│   │       │   ├── memory_recall.rs
│   │       │   ├── memory_surfaces.rs
│   │       │   ├── prompt_cache.rs
│   │       │   ├── response_guardrails.rs
│   │       │   ├── turn_driver.rs
│   │       │   ├── turn_executor.rs
│   │       │   ├── turn_state.rs
│   │       │   ├── turn_support.rs
│   │       │   ├── tool_orchestrator.rs
│   │       │   ├── tool_policy.rs
│   │       │   ├── virtual_tools.rs
│   │       │   ├── loop_guard.rs
│   │       │   └── submission_handlers.rs
│   │       ├── skills/        # Skill system
│   │       │   ├── mod.rs
│   │       │   ├── types.rs
│   │       │   ├── loader.rs
│   │       │   ├── registry.rs
│   │       │   └── injector.rs
│   │       └── tools/         # Tool trait + registry
│   │           ├── mod.rs
│   │           ├── context.rs
│   │           ├── registry.rs
│   │           └── sandbox.rs
│   │
│   ├── tools/                 # Builtin tool implementations (alan-tools)
│   │   └── src/
│   │       └── lib.rs         # Tool profiles: core(4), read-only(4), all(7)
│   │
│   ├── alan-kernel/           # Alan Kernel semantic UNIX substrate primitives
│   │   └── src/
│   │       └── lib.rs
│   │
│   ├── tui/                   # Rust inline terminal UI linked into alan
│   │   └── src/
│   │       ├── lib.rs         # TUI run loop entrypoint
│   │       ├── daemon_client.rs # Daemon HTTP/event client
│   │       ├── history.rs     # Event reducer and transcript cells
│   │       └── terminal.rs    # Crossterm/Ratatui terminal session
│   │
│   ├── skill-tools/           # Shared authoring/eval helper tooling
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── eval.rs
│   │   │   └── main.rs
│   │   └── tests/
│   │
│   └── alan/                  # CLI & daemon (current host-service implementation/gateway)
│       └── src/
│           ├── main.rs        # CLI entry point (clap)
│           ├── lib.rs         # Library exports
│           ├── host_config.rs # Host-local daemon/client config
│           ├── cli/           # CLI commands
│           │   ├── mod.rs
│           │   ├── init.rs    # `alan init` command
│           │   ├── connection.rs # `alan connection` profile/auth commands
│           │   ├── skills.rs  # `alan skills` inspection commands
│           │   ├── skill_authoring.rs # `alan skills init/validate/eval`
│           │   ├── workspace.rs # `alan workspace` commands
│           │   ├── shell.rs   # `alan shell` control commands
│           │   └── daemon.rs  # Daemon control commands
│           ├── daemon/        # HTTP/WebSocket server
│           │   ├── mod.rs
│           │   ├── server.rs  # Axum server setup
│           │   ├── routes.rs  # HTTP API routes
│           │   ├── state.rs   # AppState
│           │   ├── auth_control.rs
│           │   ├── connection_api.rs
│           │   ├── connection_control.rs
│           │   ├── connection_routes.rs
│           │   ├── relay.rs
│           │   ├── remote_control.rs
│           │   ├── scheduler.rs
│           │   ├── websocket.rs
│           │   ├── workspace_resolver.rs
│           │   ├── runtime_manager.rs
│           │   ├── task_store.rs
│           │   └── session_store.rs
│           ├── registry.rs    # Workspace registry (CLI)
│           └── skill_catalog.rs # Resolved skill catalog snapshots
│
└── clients/
    └── apple/                 # Native Apple client (SwiftUI, macOS/iOS)
```

### Crate Dependency Graph

```
alan-protocol (base — no internal deps)
    ↑
alan-llm (depends on alan-protocol)
    ↑
alan-runtime (Agent Execution Engine; depends on alan-protocol, alan-llm)
    ↑        ↑
alan-tools   alan (depends on alan-protocol, alan-runtime)

alan-kernel (Alan Kernel semantic UNIX substrate primitives)
```

---

## Build and Test Commands

### Using Just (Recommended)

```bash
just             # List available commands
just test        # Run all tests
just check       # Format + lint + test
just fmt         # Format code
just lint        # Clippy lints
just serve       # Run the daemon
just build       # Release build
just install     # Install signed release Alan.app and PATH-visible CLI links
just release-check # Check release signing/notarization setup without building
just release     # Build, sign, notarize, staple, and archive the public macOS release app
just uninstall   # Remove the local app install and alan-owned CLI links
just clean       # Clean build artifacts
just coverage    # Show coverage summary
just coverage-detail    # Detailed coverage
just coverage-html      # HTML coverage report
```

### Using Cargo

```bash
cargo build --release
cargo test --workspace
cargo test -p alan-runtime
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo run --bin alan
```

---

## Code Style Guidelines

### Rustfmt Configuration

See `rustfmt.toml`: Edition 2024, 100-char max width, 4-space indent, alphabetical imports.

```toml
edition = "2024"
max_width = 100
tab_spaces = 4
hard_tabs = false
newline_style = "Unix"
reorder_imports = true
use_field_init_shorthand = true
```

### Clippy Configuration

See `clippy.toml`: Cognitive complexity ≤ 30, enum variant ≤ 300 bytes, too-many-args ≤ 7.

```toml
cognitive-complexity-threshold = 30
enum-variant-size-threshold = 300
too-many-arguments-threshold = 7
too-many-lines-threshold = 100
type-complexity-threshold = 250
```

### Coding Conventions

1. **Naming**: Standard Rust — `snake_case`, `PascalCase`, `SCREAMING_SNAKE_CASE`
2. **Error Handling**: `anyhow` for apps, `thiserror` for libs, `?` for propagation
3. **Async**: `tokio` runtime, `#[async_trait]` for trait async methods
4. **Observability**: `tracing` for structured logging (never `println!`)
5. **Documentation**: `///` doc comments on all public APIs
6. **Module structure**: Each module has `mod.rs` or is a file with submodules

---

## Testing Strategy

Tests include inline `#[cfg(test)]` modules, extracted white-box suites, and
integration tests such as `crates/alan/tests/*`. The `alan-llm` crate provides
a `MockLlmProvider` behind the `mock` feature.

```bash
# Run all tests
cargo test --workspace

# Run tests for specific crate
cargo test -p alan-runtime
cargo test -p alan-tools
cargo test -p alan-protocol
cargo test -p alan-llm

# Run with mock feature
cargo test -p alan-llm --features mock
```

### Test Patterns

- Small, local unit tests may stay in the same file as the code they test
- Large private-access Rust suites should move to extracted white-box test files
  adjacent to the implementation module instead of growing inline test blocks
- Use `tempfile::TempDir` for filesystem tests
- Use `MockLlmProvider` for testing LLM-dependent code
- All protocol types have serialization/deserialization tests

For new or materially edited Rust tests, follow the OpenSpec Rust test
placement contract at
`openspec/specs/rust-test-placement-contract/spec.md`:
choose inline unit tests, extracted white-box tests, or crate-level integration
tests deliberately.

### Specification Workflow

Use OpenSpec for all Alan change proposals, design docs, task lists, and spec
deltas. Do not create Superpowers spec files under `docs/superpowers/specs/`
for this repository. When a change needs a written design or implementation
plan, create or update an `openspec/changes/<change-id>/` change instead.

---

## Configuration

### Environment Variables (Directly Read by Runtime/CLI)

```bash
# Config file path override
ALAN_CONFIG_PATH=/absolute/path/to/agent.toml

# Server
BIND_ADDRESS=0.0.0.0:8090

# CLI daemon endpoint override
ALAN_AGENTD_URL=http://127.0.0.1:8090
```

Additional host-only environment variables exist for remote access, relay mode,
and shell binding; keep those documented with the corresponding daemon/shell
specs rather than treating them as agent config. Host-facing daemon/client
settings live in `~/.alan/host.toml`; `ALAN_CONFIG_PATH` is for agent-facing
configuration only.

### Connection Profiles And Agent Config

Operator-facing model/provider setup is connection-profile driven:

- `~/.alan/connections.toml` stores non-secret connection profile metadata,
  provider settings, credential references, and the default profile.
- Secret API-key credentials are stored outside `agent.toml` by the host secret
  store; managed ChatGPT/Codex login state lives outside `agent.toml` in the
  managed auth store.
- `agent.toml` may pin a profile with `connection_profile = "profile-id"`.
- Runtime-only knobs such as timeouts, compaction thresholds, durability,
  memory, and `skill_overrides` remain in `agent.toml`.

Inline provider fields such as `llm_provider`, `*_api_key`, `*_base_url`, and
`*_model` are internal resolved provider state / compatibility surface. Do not
add them to new user-facing `agent.toml` examples.

Provider setup is managed through `alan connection`:

```bash
alan connection list
alan connection current --workspace /path/to/workspace
alan connection add chatgpt --profile chatgpt-main
alan connection login chatgpt-main browser
alan connection add openai_responses --profile openai-main --setting model=gpt-5.4
alan connection add openrouter --profile openrouter-main --setting model=moonshotai/kimi-k2.6
alan connection set-secret openai-main
alan connection default set chatgpt-main
alan connection pin chatgpt-main --scope global
alan connection test chatgpt-main
```

Daemon clients use the same model through `/api/v1/connections/*`. Connection
profile and provider/auth contract material lives in OpenSpec at
`openspec/specs/provider-connection-contract/spec.md`.

Agent definitions are resolved from `AgentRoot`s on disk:

```text
~/.alan/agents/default/          # global default agent root
~/.alan/agents/<name>/           # global named agent root
<workspace>/.alan/agents/default/ # workspace default agent root
<workspace>/.alan/agents/<name>/ # workspace named agent root
```

Each root may contribute `agent.toml`, `persona/`, `skills/`, and `policy.yaml`.
alan also scans the standard public skill install directories `~/.agents/skills/`
and `<workspace>/.agents/skills/` as single-skill package sources for the
global and workspace default layers.
Default workspace agents resolve `~/.alan/agents/default -> <workspace>/.alan/agents/default`.
Named agents extend that chain with `~/.alan/agents/<name> -> <workspace>/.alan/agents/<name>`.
This is definition overlay, not runtime parent-child inheritance.
The former singular default root `.alan/agent/` is not read; move authored default
agent files to `.alan/agents/default/`.

### Config File

Agent-facing configuration loads from `~/.alan/agents/default/agent.toml` or
`ALAN_CONFIG_PATH`:

```toml
connection_profile = "chatgpt-main"

[[skill_overrides]]
skill = "memory"
allow_implicit_invocation = false

[[skill_overrides]]
skill = "plan"
allow_implicit_invocation = false

[[skill_overrides]]
skill = "workspace-manager"
allow_implicit_invocation = false

llm_request_timeout_secs = 180
tool_timeout_secs = 30
max_tool_loops = 0
tool_repeat_limit = 4
# Optional explicit override; otherwise derived from the model catalog
context_window_tokens = 128000
# Deprecated hard-threshold alias:
# compaction_trigger_ratio = 0.8
# Preferred dual-threshold form:
# compaction_soft_trigger_ratio = 0.72
# compaction_hard_trigger_ratio = 0.8
prompt_snapshot_enabled = false
prompt_snapshot_max_chars = 8000
# Optional canonical reasoning effort. If omitted and the selected model is in
# alan's model catalog, alan uses the model's catalog default.
model_reasoning_effort = "medium"

[memory]
enabled = true
strict_workspace = true

[durability]
required = false
```

Connection profile metadata lives separately:

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
credential_id = "chatgpt"
source = "managed"

[profiles.chatgpt-main.settings]
base_url = "https://chatgpt.com/backend-api/codex"
model = "gpt-5.3-codex"
account_id = ""
```

Model metadata resolves in this order:

1. Bundled catalog
2. `~/.alan/models.toml`
3. `{workspace}/.alan/models.toml`

Overlay catalogs currently extend `openai_chat_completions_compatible` models only. Official
`openai_responses` and `openai_chat_completions` models stay pinned to alan's curated catalog.

---

## HTTP API

The daemon exposes REST, WebSocket, connection-management, and skill-catalog endpoints.
Canonical route ownership lives in `crates/alan/src/daemon/api_contract.rs`;
the examples below remain the stable public paths.

```bash
# Health check
curl http://localhost:8090/health

# Create session; profile_id is optional. If omitted, alan resolves a pinned
# connection_profile / default profile during runtime startup.
curl -X POST http://localhost:8090/api/v1/sessions \
  -H "Content-Type: application/json" \
  -d '{
    "workspace_dir": "/path/to/workspace",
    "agent_name": "default",
    "profile_id": "chatgpt-main",
    "governance": {"profile": "conservative", "policy_path": ".alan/agents/default/policy.yaml"},
    "streaming_mode": "on",
    "partial_stream_recovery_mode": "continue_once"
  }'

# Create autonomous session (fewer runtime interruptions)
curl -X POST http://localhost:8090/api/v1/sessions \
  -H "Content-Type: application/json" \
  -d '{"governance": {"profile": "autonomous"}}'

# Create-session response includes:
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
#   "durability": {"durable": false, "required": false}
# }
# Note: returns 409 when the workspace already has an active runtime.

# List sessions
curl http://localhost:8090/api/v1/sessions

# Get session
curl http://localhost:8090/api/v1/sessions/{id}

# Read session metadata + persisted message history
curl http://localhost:8090/api/v1/sessions/{id}/read

# Read persisted message history only
curl http://localhost:8090/api/v1/sessions/{id}/history

# Read reconnect handoff state for mobile/TUI recovery
curl http://localhost:8090/api/v1/sessions/{id}/reconnect_snapshot

# Submit operation (start a new turn)
curl -X POST http://localhost:8090/api/v1/sessions/{id}/submit \
  -H "Content-Type: application/json" \
  -d '{"op": {"type": "turn", "parts": [{"type": "text", "text": "Hello!"}]}}'

# Steering input during a running turn; mode defaults to "steer" when omitted.
curl -X POST http://localhost:8090/api/v1/sessions/{id}/submit \
  -H "Content-Type: application/json" \
  -d '{"op": {"type": "input", "mode": "follow_up", "parts": [{"type": "text", "text": "Then summarize it."}]}}'

# Stream events (NDJSON)
curl -N http://localhost:8090/api/v1/sessions/{id}/events

# Read buffered events (poll API)
curl "http://localhost:8090/api/v1/sessions/{id}/events/read?after_event_id=e-123&limit=50"
# Response includes:
# {
#   "session_id": "...",
#   "gap": false,
#   "oldest_event_id": "e-100",
#   "latest_event_id": "e-123",
#   "events": [...]
# }

# Resume stalled runtime channel (server-side recovery)
curl -X POST http://localhost:8090/api/v1/sessions/{id}/resume

# Fork session from latest rollout; optional body can override workspace/profile/governance.
curl -X POST http://localhost:8090/api/v1/sessions/{id}/fork

# Roll back in-memory turns (non-durable; does not survive restart)
curl -X POST http://localhost:8090/api/v1/sessions/{id}/rollback \
  -H "Content-Type: application/json" \
  -d '{"turns": 2}'
# Response includes:
# {
#   "submission_id": "...",
#   "accepted": true,
#   "durability": {"durable": false, "scope": "in_memory"},
#   "warning": "Rollback is in-memory only and will not survive runtime restart."
# }

# Trigger manual context compaction; body is optional.
curl -X POST http://localhost:8090/api/v1/sessions/{id}/compact \
  -H "Content-Type: application/json" \
  -d '{"focus": "preserve open todos and file paths"}'

# Schedule or sleep a session/run until a future instant.
curl -X POST http://localhost:8090/api/v1/sessions/{id}/schedule_at \
  -H "Content-Type: application/json" \
  -d '{"wake_at": "2026-04-24T09:00:00Z"}'
curl -X POST http://localhost:8090/api/v1/sessions/{id}/sleep_until \
  -H "Content-Type: application/json" \
  -d '{"wake_at": "2026-04-24T09:00:00Z"}'

# Delete session
curl -X DELETE http://localhost:8090/api/v1/sessions/{id}

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
```

WebSocket: connect to `/api/v1/sessions/{id}/ws` for real-time bidirectional communication.

---

## Key Concepts

### Events (Output Protocol)

Events are emitted by the runtime to notify frontends of state changes:

- `TurnStarted` / `TurnCompleted` — Turn boundaries
- `ThinkingDelta` — Streaming reasoning content
- `TextDelta` — Streaming response content
- `ToolCallStarted` / `ToolCallCompleted` — Tool execution
- `PlanUpdated` — Transport-level plan snapshot
- `SessionRolledBack` — In-memory rollback notification
- `CompactionObserved` / `MemoryFlushObserved` — Structured compaction and
  pre-compaction memory-flush outcomes
- `Yield` — Engine is suspended and waiting for external input
- `Warning` — Non-fatal warning
- `Error` — Something went wrong

Server transports wrap events in `EventEnvelope` with stable cursor metadata:
`event_id`, `sequence`, `session_id`, `submission_id`, `turn_id`, `item_id`,
and `timestamp_ms`.

### Operations (Input Protocol)

Operations are submitted by users to control the agent:

- `Turn` — Start a new user turn
- `Input` — Submit input with mode `steer`, `follow_up`, or `next_turn`
- `Resume` — Resume a pending `Yield` request
- `Interrupt` — Stop current execution
- `RegisterDynamicTools` — Add client-provided tools
- `SetClientCapabilities` — Negotiate adaptive client yield/UI capabilities
- `CompactWithOptions` — Trigger context compaction with optional `focus`
- `Rollback` — Roll back N turns

### Tools

`alan-tools` provides layered built-in tool profiles:

- **Core (default)**: `read_file`, `write_file`, `edit_file`, `bash`
- **Read-only exploration**: `read_file`, `grep`, `glob`, `list_dir`
- **All built-ins**: core + exploration tools (7 total)

Tool details:

| Tool         | Capability | Description                            |
| ------------ | ---------- | -------------------------------------- |
| `read_file`  | Read       | Read file contents (with offset/limit) |
| `write_file` | Write      | Write content to file                  |
| `edit_file`  | Write      | Search/replace text in file            |
| `bash`       | Read/Write/Network (dynamic) | Execute shell commands     |
| `grep`       | Read       | Search file contents with regex        |
| `glob`       | Read       | Find files matching pattern            |
| `list_dir`   | Read       | List directory contents                |

Runtime virtual tools (not from `alan-tools`, injected by runtime):

- `request_confirmation` — pause and emit `Yield(confirmation)`
- `request_user_input` — pause and emit `Yield(structured_input)`
- `update_plan` — update in-memory plan state in current turn
- `invoke_delegated_skill` — parent runtimes only; launches a delegated
  package-local launch target. Launch-root runtimes intentionally keep nested
  delegation disabled in V1.

### Tool Governance

Runtime applies tool decisions in two stages:

1. `PolicyEngine` returns `allow | escalate | deny`.
2. If execution proceeds, the current `workspace_path_guard` backend applies a
   best-effort execution guard for workspace paths and shell shape checks.

`workspace_path_guard` is not a strict OS sandbox and does not guarantee full
network/process isolation. It enforces workspace containment, blocks protected
subpaths such as `.git`, `.alan`, and `.agents`, and conservatively rejects
shell shapes it cannot validate.

Session governance is configured via:

```json
{
  "governance": {
    "profile": "autonomous",
    "policy_path": ".alan/agents/default/policy.yaml"
  }
}
```

Policy resolution order is:

1. `governance.policy_path`, if provided
2. the highest-precedence existing `policy.yaml` in the resolved `AgentRoot` chain
3. builtin profile defaults

When a policy file is found, it replaces the builtin profile rule set for that session.
There is no session-wide approval cache for governance escalations: each
`escalate` outcome emits its own recoverable `Yield` and each approval applies
only to that pending checkpoint.

### Skills

Skill-system contract material lives in OpenSpec at
`openspec/specs/skill-system-contract/spec.md`.
For implementation details and current runtime surfaces, see
`docs/skills_and_tools.md`. This section is only the agent-guide summary.

Skills are Markdown-based capabilities with YAML frontmatter:

```markdown
---
name: skill-name
description: What this skill does and when to use it
metadata:
  short-description: Brief description
  tags: ["tag1", "tag2"]
---

# Instructions

Step-by-step guidance for the agent...
```

Skills can be used:
1. Host-level force-select: a direct skill reference such as `$skill-id`
2. Implicitly: by being listed in the rendered skills catalog when
   `allow_implicit_invocation = true`, where `name` and `description` are the
   portable selection surface

The runtime `skill_id` is derived from the package directory name as a
normalized lower-case hyphenated slug. Force-select uses this runtime id, not a
skill-authored alias.

Capability sources:
- Built-in first-party packages embedded in the binary
- User public skills in `~/.agents/skills/`
- User agent-root packages in `~/.alan/agents/default/skills/` and `~/.alan/agents/<name>/skills/`
- Workspace public skills in `{workspace}/.agents/skills/`
- Workspace agent-root packages in `{workspace}/.alan/agents/default/skills/` and `{workspace}/.alan/agents/<name>/skills/`

Each resolved skill then carries runtime exposure fields:
- `enabled`
- `allow_implicit_invocation`

Package directories may also export supporting resources such as `scripts/`,
`references/`, `assets/`, optional authoring/eval assets under `evals/` and
`eval-viewer/`, and package-local launch targets under `agents/`.
Skills can declare availability gates through frontmatter such as
`capabilities.required_tools`, `compatibility.min_version`, and
`compatibility.dependencies`; unresolved constraints mark the skill unavailable
in runtime and `alan skills` output.
Resolved skill execution may be `inline` or
`delegate(target=package-launch-target)`; the full contract material lives in
OpenSpec.

## Agent skills

### Issue tracker

Issues and PRDs are tracked in GitHub Issues for `realmorrisliu/Alan` via `gh`.
See `docs/agents/issue-tracker.md`.

### Triage labels

Use the five canonical triage labels directly, with no aliases or remapping.
See `docs/agents/triage-labels.md`.

### Domain docs

Use a single-context layout for repo domain docs.
See `docs/agents/domain.md`.

---

## Development Workflow

1. **Before committing**: `just check`
2. **After Rust code changes under `crates/`**: run `just verify` for the core
   flow. If `~/.alan` LLM config is available locally, run `just verify-full`
   for the end-to-end validation path.
3. **Adding a new LLM provider**: Implement `LlmProvider` trait in
   `crates/llm/src/`, add connection-profile/provider-descriptor support,
   update model metadata, and document capability degradation
4. **Adding new tools**: Implement `Tool` trait in `crates/tools/src/`, register via `create_core_tools()`
5. **Adding skills**: Create a directory-backed skill package under
   `crates/runtime/skills/<skill-id>/` for first-party built-ins, use
   `alan skills init` for scaffolding ordinary packages, or add packages under
   an agent-root `skills/` directory / the zero-conversion public install
   directories under `.agents/skills/`. For the stable contract material, use
   `openspec/specs/skill-system-contract/spec.md`;
   for current implementation details, use `docs/skills_and_tools.md` and
   `docs/skill_authoring.md`.

### macOS App Testing Safety

For macOS app testing, scripted operation, visual QA, and Computer Use, use the
dev channel as the default target: `Alan Dev.app`,
`app.alanworks.macos.dev`, `alan-dev`, `~/.alan-dev`, and
`alan-dev-shell-control`.

Do not launch, quit, install over, inspect, or otherwise operate the user's
stable/native Alan app (`Alan.app`, `app.alanworks.macos`, `alan`, `~/.alan`)
for testing unless the user explicitly asks to work on the stable channel. The
stable/native app is the user's normal work environment; automation against it
can interrupt active work.

---

## Installation

```bash
# Homebrew cask distribution
brew install --cask alan

# Local developer install from this checkout
git clone <repo-url>
cd Alan
ALAN_DEVELOPER_ID_APPLICATION="Developer ID Application: Example (TEAMID)" just install

# Run
alan
```

The signed `Alan.app` bundle embeds the `alan` command under
`Contents/Resources/bin`. Homebrew links that embedded tool into its prefix.
For a direct app install, use **Tools > Install Command Line Tools...** in the
app to create PATH-visible symlinks. `~/.alan/bin` is not a supported
distribution path.

Local release/install scripts load only allowlisted assignments from
`ALAN_RELEASE_ENV_FILE` when set. Without that override they look for repo-local
env files such as `.env.release.local`, `.env.local`, and `.env`, then fall
back to `~/.alan/release.env` when present.
Use `.env.example` as the tracked template and keep real machine credentials in
ignored `.env`.
Supported private signing/notarization keys include
`ALAN_DEVELOPER_ID_APPLICATION`, `ALAN_SIGNING_IDENTITY`,
`ALAN_NOTARY_KEYCHAIN_PROFILE`, `APPLE_ID`, `APPLE_TEAM_ID`, and
`APPLE_APP_SPECIFIC_PASSWORD`.

For one-command public release automation, set `ALAN_NOTARY_KEYCHAIN_PROFILE`
plus `APPLE_ID` / `APPLE_TEAM_ID` / `APPLE_APP_SPECIFIC_PASSWORD`.
`just release-check` validates the setup and creates or refreshes the notary
keychain profile when those credentials are present; `just release` runs the
full signed, notarized, stapled release assembly.

---

## References

- **README.md**: Project philosophy and vision
- **docs/architecture.md**: Full architecture documentation
- **docs/governance_current_contract.md**: Authoritative current governance contract
- **openspec/specs/**: Long-lived specifications
- **openspec/changes/**: Active specification changes
- **docs/spec/README.md**: Legacy spec migration bridge
- **docs/spec/*.md**: Legacy compatibility bridge pages, not contract sources

### Inspirations

- [Claude Code](https://claude.ai) — human-style reasoning and collaboration
- [Codex](https://openai.com/blog/openai-codex) — intelligence expressed through code
- [pi-mono](https://github.com/badlogic/pi-mono/) — minimal agent runtime design
- **Turing Machine** — computation as state transitions on a tape

---

*Last updated: 2026-04-23*
*Project: alan v0.1.0 (early development)*
