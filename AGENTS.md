# Alan Agent Guide

> Project status: early development. Public APIs may change without notice.

## Canonical component names

Use these names consistently in code, specs, docs, UI copy, and reviews.

| Name | Meaning |
| --- | --- |
| Alan | The product: a programmable personal computing environment. |
| alan9 | alan9 Kernel, file-server system services, Service Manager, Root Agent Process, Agent Runtime Service, hosts, and app integration conventions. |
| alan9 Kernel / `alan-kernel` | Namespace and mounts, paths, files, descriptors, rights, credentials, Process table, `/proc`, and `/srv`. |
| Standard Namespace | `/proc`, `/agent`, `/srv`, `/bin`, `/lib`, `/man`, and `/mnt`. |
| Service Manager | The system Process that starts and supervises services and boot units. |
| File-Server Service | A long-running Process that exports a mountable file tree. |
| Service Handle Registry / `/srv` | The rendezvous tree for mountable service handles. |
| Package Service | The File-Server Service that owns installed package content, catalog state, lifecycle, and projections from its System Store subtree. |
| Quartermaster / `q` | The Package Service product surface and ordinary `/bin/q` Process command. |
| Agent Runtime Service | The internal file-server service that executes Agent Processes and serves AgentFS at `/agent`. |
| Process | Bounded execution with PID, parent, descriptors, credentials, lifecycle, streams, status, and exit state. |
| Agent Process | An ordinary Process recognized by its AgentFS file layout. `/proc/<pid>` is lifecycle truth; `/agent/<pid>` is its agent view. |
| Root Agent Process | The always-available root of the agent process tree, surfaced through `/agent/root`. |
| Agent Executable | An executable bound into `/bin` that creates an Agent Process when spawned. |
| Tool | A reusable executable in the alan9 command namespace. |
| Skill | A manual-like knowledge package passed to Agent Processes by descriptor. |
| Memory Stores | File trees that own personal, continuity, app, and mounted-domain memory. |
| Alan Agent | An optional Agent Workspace app that inspects and steers Agent Processes through files. |
| Agent Execution Engine / `alan-agent-engine` | The current tape/model/Tool/policy/memory transition loop in `crates/agent-engine`. |
| Alan for macOS | Retired desktop product; App and shell-core/FFI source removed (ADR-0054). |
| Alan Shell / `alan-shell` | The file-native shell. Bare `alan` uses the file-backed TUI on terminal stdin/stdout to attach to `/agent/root`; redirected stdin submits one Agent task and writes its answer to stdout. |
| Alan Apps | Apps with app-owned domain cores and Alan file-server adapters. |

## Architecture rules

Move touched code toward the accepted alan9 ownership model recorded in
OpenSpec and the ADRs.

- alan9 Kernel depends only on aP among Alan crates.
- Process owns lifecycle and identity.
- Agent Machine owns tape and transition-local state.
- AgentFS owns agent IO, requests, actions, and machine files.
- rollout/checkpoint files own durable execution evidence.
- Memory Stores and handoff files own continuity across Agent Processes.
- provider, sandbox, terminal, macOS, and app details stay behind adapters.
- agent-ness is a file-layout convention, never a second Kernel Process type.
- bare `alan` attaches its terminal renderer to the existing `/agent/root`; it
  does not create a Shell Process or own Root Agent lifecycle (ADR-0056).
- avoid introducing globally addressable Thread, Conversation, or execution
  manager objects.
- prefer existing terminal hosts, especially Herdr; do not rebuild desktop topology (ADR-0054).
- distinguish accepted mixed-Machine direction (ADR-0055) from current generation-only execution.

When a touched area is transitional, make the durable target owner explicit and
keep the slice narrowly scoped.

## AI Turing Machine

Each Agent Process uses an AI Turing Machine abstraction. ADR-0055 accepts the
following mixed-capability direction; typed evaluation is not implemented yet:

| Concept | Implementation |
| --- | --- |
| Tape | `Tape` messages, context, and compaction summary |
| Transition function | Machine advancement using deterministic code, typed evaluation or generation |
| State | Agent Machine files under `/agent/<pid>/machine` |
| Alphabet | Agent IO, machine events, and Tool Process results |
| Side effects | Tool spawn and file writes through descriptors |
| Completion | Work may complete with a structured result; wait, failure and Process exit are distinct |

The Agent Execution Engine currently implements a generation/Tool loop, not
the entire target above. Namespace Tape is currently a text projection;
rollout/checkpoint recovery must not be equated with that projection alone.
The engine is not alan9 Kernel or alan9 itself.

## Current repository structure

```text
crates/
├── ap/               # aP protocol and FileServer trait
├── kernel/           # namespace, Process table, /proc, /srv
├── agentfs/          # AgentFS at /agent
├── hostfs/           # mounted host directories
├── llmfs/            # LLM Connections as files
├── memfs/            # Memory Store file server
├── routefs/          # file-native routing
├── editfs/           # editable buffers
├── branchfs/         # branching execution files
├── shell/            # aP-only shell builtins
├── agent-protocol/   # Event/Op execution alphabet
├── llm/              # provider adapters
├── agent-engine/     # Agent Execution Engine
├── tools/            # builtin Tool implementations
├── tui/              # file-backed Ratatui renderer/input loop
└── alan/             # CLI host and linked TUI

openspec/             # canonical specs and active changes
```

## Build and verification

Prefer Just for complete workflows:

```bash
just test
just check
just quality
just fmt
just lint
just build
just install-hooks
just install
just standalone-distribution-test
```

`just quality` is the canonical non-mutating clean-code and architecture gate.
CI is the merge-blocking authority; `just install-hooks` enables the same gate
as local pre-commit feedback. The hook can be bypassed locally with
`--no-verify`, but required CI cannot.

Focused Rust commands:

```bash
cargo test --workspace
cargo test -p alan-agent-engine
cargo test -p alan-agent-protocol
cargo test -p alan-terminal-ui
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Desktop App, shell-core and FFI source has been removed. Validate standalone
CLI/Host behavior and macOS Rust platform adapters; no Apple UI build is needed.

## Rust style

- Rust edition 2024, four spaces, 100-column target.
- `anyhow` for application errors and `thiserror` for library errors.
- Tokio for async execution.
- `tracing` for observability.
- Document public APIs with `///` comments.
- Keep modules small enough that ownership is legible.
- Use inline tests for small local behavior, adjacent extracted white-box
  suites for large private-access tests, and crate integration tests for public
  boundaries. Follow `rust-test-placement-contract` in OpenSpec.

## Configuration

The only direct runtime config override is:

```text
ALAN_CONFIG_PATH=/absolute/path/to/agent.toml
```

Operator-facing provider setup is connection-profile driven:

```bash
alan connection list
alan connection current
alan connection add chatgpt --profile chatgpt-main
alan connection login chatgpt-main browser
alan connection add openai_responses --profile openai-main --setting model=gpt-5.4
alan connection set-secret openai-main
alan connection default set chatgpt-main
alan connection test chatgpt-main
```

Connection metadata lives in the channel Connection Service subtree of the
System Store. Credentials and managed auth state use their owning Host stores.
Agent config may select a profile with `connection_profile = "profile-id"` but
must not contain new inline secrets.

Host-private backing is channel-isolated:

```text
~/Library/Application Support/Alan/System Store/<channel>/
~/Library/Application Support/Alan/Host Store/<channel>/
```

Agent Definitions and Skills resolve only from explicit descriptors or
installed alan9 references. A definition tree may contain `agent.toml`,
`persona/`, `skills/`, and `policy.yaml`; no Host-directory overlay is inferred.
Generated runtime evidence and Memory Store data belong to their owning System
Store services, never to a Host project directory.

## Specification workflow

OpenSpec is the sole normative spec and planning surface. Put proposals,
designs, task lists, and deltas under `openspec/changes/<change-id>/`. Do not
create alternative spec documents elsewhere. Historical files under
`openspec/changes/archive/` are immutable and non-normative.

## Product design context

Alan is terminal-first, with Herdr as the preferred host. Keep terminal output
calm, precise and readable with progressive disclosure and usable scrollback.
Do not recreate a sidebar, workspace manager or terminal emulator. Herdr-native
Alan detection is not yet implemented; ordinary terminal operation comes first.
Read a change's disposition before executing its tasks; parked/cancelled work
does not authorize implementation.
