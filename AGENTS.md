# Alan Agent Guide

> Project status: early development. Public APIs may change without notice.

## Canonical component names

Use these names consistently in code, specs, docs, UI copy, and reviews.

| Name | Meaning |
| --- | --- |
| Alan | The product: a programmable personal computing environment. |
| Alan OS | Alan Kernel, file-server system services, Service Manager, Root Agent Process, Agent Runtime Service, hosts, and app integration conventions. |
| Alan Kernel / `alan-kernel` | Namespace and mounts, paths, files, descriptors, rights, credentials, Process table, `/proc`, and `/srv`. |
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
| Tool | A reusable executable in the Alan OS command namespace. |
| Skill | A manual-like knowledge package passed to Agent Processes by descriptor. |
| Memory Stores | File trees that own personal, continuity, app, and mounted-domain memory. |
| Alan Agent | An optional Agent Workspace app that inspects and steers Agent Processes through files. |
| Agent Execution Engine / `alan-agent-engine` | The current tape/model/Tool/policy/memory transition loop in `crates/agent-engine`. |
| Alan for macOS | Native Apple terminal host, renderer, input shell, windowing, and OS integration surface. |
| Alan Shell / `alan-shell` | The file-native shell. The current interactive product path is the Rust TUI in `crates/tui`. |
| Alan Apps | Apps with app-owned domain cores and Alan file-server adapters. |

## Architecture rules

Move touched code toward the accepted Alan OS ownership model recorded in
OpenSpec and the ADRs.

- Alan Kernel depends only on aP among Alan crates.
- Process owns lifecycle and identity.
- Agent Machine owns tape and transition-local state.
- AgentFS owns agent IO, requests, actions, and machine files.
- rollout/checkpoint files own durable execution evidence.
- Memory Stores and handoff files own continuity across Agent Processes.
- provider, sandbox, terminal, macOS, and app details stay behind adapters.
- agent-ness is a file-layout convention, never a second Kernel Process type.
- avoid introducing globally addressable Thread, Conversation, or execution
  manager objects.
- keep Alan for macOS attachment design out of unrelated changes; see ADR-0029.

When a touched area is transitional, make the durable target owner explicit and
keep the slice narrowly scoped.

## AI Turing Machine

Each Agent Process is modeled as a Turing machine:

| Concept | Implementation |
| --- | --- |
| Tape | `Tape` messages, context, and compaction summary |
| Transition function | LLM generation |
| State | Agent Machine files under `/agent/<pid>/machine` |
| Alphabet | Agent IO, machine events, and Tool Process results |
| Side effects | Tool spawn and file writes through descriptors |
| Halt | No more Tool calls; final text is emitted |

The Agent Execution Engine implements this loop. It is not Alan Kernel or Alan
OS itself.

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
├── shell-core/       # platform-neutral shell surface model
├── shell-core-ffi/   # C ABI facade
└── alan/             # CLI host and linked TUI

clients/apple/        # Alan for macOS
openspec/             # canonical specs and active changes
```

## Build and verification

Prefer Just for complete workflows:

```bash
just test
just check
just fmt
just lint
just build
just install-dev
just apple-shell-focused-tests
just apple-shell-ui-smoke
```

Focused Rust commands:

```bash
cargo test --workspace
cargo test -p alan-agent-engine
cargo test -p alan-agent-protocol
cargo test -p alan-terminal-ui
cargo test -p alan-shell-core -p alan-shell-core-ffi
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

For Alan for macOS changes, use `Alan Dev.app`, relaunch a fresh build, and
verify rendered behavior as well as source tests.

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
installed Alan OS references. A definition tree may contain `agent.toml`,
`persona/`, `skills/`, and `policy.yaml`; no Host-directory overlay is inferred.
Generated runtime evidence and Memory Store data belong to their owning System
Store services, never to a Host project directory.

## Specification workflow

OpenSpec is the sole normative spec and planning surface. Put proposals,
designs, task lists, and deltas under `openspec/changes/<change-id>/`. Do not
create alternative spec documents elsewhere. Historical files under
`openspec/changes/archive/` are immutable and non-normative.

## Product design context

Alan for macOS is terminal-first. Its personality is calm, precise, native,
and quiet. Use an Arc-like material sidebar, compact controls, restrained type,
and progressive disclosure. Do not turn the shell into a dashboard or expose
raw implementation identifiers in the default UI. Build a coherent light
appearance first.
