# Alan

**Alan is an agent built to be the next computer.**

Alan starts from the premise that the next computer is not just a device people
operate through apps, but an agent that can help humans act across the digital
world. Here, "computer" does not mean a bare-metal machine. It means the
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
> Governance model note: HITE governance sections in this README reflect the accepted V2 target design and may be in migration until implementation is complete.
> The authoritative current implementation contract lives in
> `docs/governance_current_contract.md`.

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

1. **Generic Core** — `alan-runtime` is provider-agnostic, domain-agnostic, and hosting-agnostic
2. **Checkpointed Reasoning** — Every thought, action, and observation is durably recorded
3. **Separation of Concerns** — Core handles state transitions; the `alan` binary handles lifecycle & CLI
4. **Skills over Plugins** — Capabilities are Markdown-based instructions, not compiled code
5. **Human-in-the-End** — Humans own outcomes, not operations ([docs →](docs/README.md))

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Clients                              │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐                  │
│  │   TUI    │  │  alan    │  │   API    │                   │
│  │  (Bun)   │  │   ask    │  │ (HTTP/WS)│                   │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘                  │
└───────┼─────────────┼─────────────┼─────────────────────────┘
        │             │             │
        └─────────────┴─────────────┘
                      │
              ┌───────▼────────┐
              │      alan      │  ← Unified CLI & daemon
              │  daemon server │
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
              │  alan-runtime │  ← Agent runtime (transition fn + tape)
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
│   ├── protocol/     # Event/Op protocol definitions + ContentPart
│   ├── llm/          # LLM provider adapters (Gemini, OpenAI, Anthropic)
│   ├── runtime/      # Core runtime: tape, session, agent loop, skills
│   ├── tools/        # Builtin tool implementations
│   └── alan/         # Unified CLI & daemon (ask, chat, workspace, daemon)
├── clients/
│   ├── tui/          # Terminal UI (Bun + TypeScript)
│   └── apple/        # Native Apple client (SwiftUI, macOS/iOS)
└── docs/             # Architecture, contracts, maintainer notes, testing strategy
```

### Crates

| Crate           | Role                                                                |
| --------------- | ------------------------------------------------------------------- |
| `alan-protocol` | Wire format — Events (output), Operations (input), ContentPart      |
| `alan-llm`      | Pluggable LLM adapters — OpenAI Responses API, OpenAI Chat Completions API, OpenAI Chat Completions API-compatible, Google Gemini GenerateContent API, Anthropic Messages API, and OpenRouter SDK-backed chat |
| `alan-runtime`  | Core engine — session, tape, agent loop, tool registry, skills      |
| `alan-tools`    | Builtin tool implementations (`read_file`, `bash`, `grep`, etc.)    |
| `alan`          | Unified CLI & daemon — workspace lifecycle, HTTP/WS API, ask, chat  |

---

## Features

- **Multi-Provider LLM**: OpenAI Responses API, OpenAI Chat Completions API, OpenAI Chat Completions API-compatible, Google Gemini GenerateContent API, Anthropic Messages API, OpenRouter
- **Streaming Responses**: Real-time token streaming with tool call support
- **Layered Tool Profiles**:
  - Core (default): `read_file`, `write_file`, `edit_file`, `bash`
  - Read-only exploration: `read_file`, `grep`, `glob`, `list_dir`
  - All built-ins: core + exploration tools (7 total)
- **Skill System**: Markdown-based capability packages with public Codex/Claude-compatible `SKILL.md` portability, explicit activation, implicit catalog listing, progressive disclosure, and delegated child-agent execution
- **Capability-Package Hosting**: Built-in first-party packages, agent-root `skills/` directories, and public `.agents/skills/` installs resolve into one `ResolvedCapabilityView`; packages can expose portable skills, child-agent roots, and resource directories without requiring `package.toml`
- **Skill Management Surface**: daemon APIs expose the local skill catalog, change polling, and skill override writes
- **Session Persistence**: Rollout recording with pause/resume/replay
- **HITE Governance**: Humans define boundaries, policy decides (`allow/deny/escalate`), and the current execution backend applies a best-effort local guard (current backend: `workspace_path_guard` with protected subpaths and only plain shell commands with statically addressable paths; shell control flow is rejected, common wrapper forms such as `env`/`command`/`builtin`/`exec`/`time`/`nice`/`nohup`/`timeout`/`stdbuf`/`setsid` are rejected, process path references under protected subpaths are blocked, glob patterns are rejected, direct nested shell/code evaluators are disabled, direct opaque command dispatchers such as `xargs`/`find -exec` are rejected, and a curated set of common direct script interpreters such as `python file.py`/`bash script.sh`/`awk -f script.awk` are rejected; the backend checks explicit path-like argv references and redirection targets but does not infer utility-specific operand roles for arbitrary bare tokens, and arbitrary program-internal writes or dispatch such as `git init`/`git add`/`git config --local`, `find -delete`, build/task runners, or utility-specific script/DSL modes like `sed -f` are not inspected by this backend and instead rely on governance. Public session APIs report this backend as `execution_backend`.)
- **Policy Profiles**: Builtin `autonomous`/`conservative` presets, overridable via `policy.yaml` in the resolved agent-root chain
- **Steering-First Execution**: In-turn `input` can interrupt tool batches and reprioritize the next step
- **WebSocket + HTTP API**: Real-time bidirectional communication
- **Context Compaction**: Automatic summarization when context grows large
- **One-Shot Ask**: `alan ask` for non-interactive queries with text/json/quiet output modes
- **Thinking Support**: Optional reasoning/thinking display with canonical named effort control
- **Session Rollback**: Undo last N turns within a session

---

## Thinking / Reasoning Support

alan exposes `model_reasoning_effort` as the canonical runtime config control.
The old public `thinking_budget_tokens` field has been removed; provider-native
budgets are derived internally from named effort presets when a provider requires
budget-shaped wire fields. Current provider behavior:

- **Anthropic Messages API**: native thinking blocks, thinking signature, and redacted thinking blocks; named effort maps to provider budget presets
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
- `alan ask --thinking` controls whether thinking deltas are shown in text-mode CLI output.

---

## Quick Start

### Prerequisites

- Rust 1.85+ (2024 edition)
- [just](https://github.com/casey/just) (task runner, optional but recommended)

### Building

```bash
git clone <repo-url>
cd Alan
cargo build --release

# Or use just
just build
```

### Installation

The supported macOS distribution is app-first. The signed `Alan.app` bundle
contains the `alan` CLI and `alan-tui` executable under
`Contents/Resources/bin`.

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

Homebrew links the embedded `alan` and `alan-tui` binaries into its prefix.
When installing `Alan.app` directly, use **Tools > Install Command Line
Tools...** in the app to create PATH-visible symlinks. `~/.alan/bin` is not a
supported install location.

### Configuration

Create `~/.alan/agents/default/agent.toml`:

If you launch `alan chat` or `alan-tui` without a config file, the first-run wizard now starts
with user-facing service presets such as OpenAI API Platform, ChatGPT/Codex login,
OpenRouter, Kimi Coding, DeepSeek, Google Gemini via Vertex AI, and Anthropic API.
Raw API-family selection is kept behind `Advanced / custom setup`, but the generated files
now use the canonical connection-profile surface shown below.

```toml
# agent.toml
llm_request_timeout_secs = 180
tool_timeout_secs = 30

# ~/.alan/connections.toml
version = 1
default_profile = "openai-main"

[credentials.openai-main]
kind = "secret_string"
provider_family = "openai_responses"
label = "OpenAI API Platform credential"
backend = "alan_home_secret_store"

[profiles.openai-main]
provider = "openai_responses"
label = "OpenAI API Platform"
credential_id = "openai-main"
source = "managed"

[profiles.openai-main.settings]
base_url = "https://api.openai.com/v1"
model = "gpt-5.4"

# OpenRouter profile example:
# alan connection add openrouter --profile openrouter-main --setting model=moonshotai/kimi-k2.6
# alan connection set-secret openrouter-main

# Optional explicit pin
# connection_profile = "openai-main"

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
package-local child-agent run. The detailed execution, fallback, and
availability semantics are part of the legacy skill-system contract material
being migrated into OpenSpec.

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

# Interactive chat (launches TUI)
alan chat

# One-shot question
alan ask "What does this project do?"
alan ask "Explain main.rs" --workspace ./my-project
alan ask "Summarize" --output json      # NDJSON for automation
alan ask "Summarize" --output quiet     # text only at end
alan ask "Think step by step" --thinking --timeout 60
# ask defaults to autonomous governance profile

# Inspect resolved skills, packages, package exports, and availability
alan skills list
alan skills packages

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
    "governance": {"profile": "autonomous", "policy_path": ".alan/agents/default/policy.yaml"},
    "streaming_mode": "on"
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
#   "governance": {...},
#   "streaming_mode": "on"
# }
# Note: 409 returned when the workspace already has an active runtime.

# Read session metadata + persisted messages
curl http://localhost:8090/api/v1/sessions/{id}/read

# Read persisted message history only
curl http://localhost:8090/api/v1/sessions/{id}/history

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
just verify         # fmt + lint + test + smoke (run after code changes)
just verify-full    # verify + real LLM e2e test (needs ~/.alan config)
just coverage       # test coverage summary
just serve          # run the daemon in foreground
```

---

## Inspirations

- [Claude Code](https://claude.ai) — human-style reasoning and collaboration
- [Codex](https://openai.com/blog/openai-codex) — intelligence expressed through code
- [pi-mono](https://github.com/badlogic/pi-mono/) — minimal agent runtime design
- **Turing Machine** — computation as state transitions on a tape

---

## License

Apache License 2.0 — See [LICENSE](LICENSE) for details.
