# Alan

Alan is a programmable personal computing environment. The repository is in
early development and currently contains three usable layers:

- Alan OS substrate crates for namespaces, mounts, files, descriptors,
  Processes, `/proc`, `/srv`, and file-server composition;
- an Agent Execution Engine that runs the AI Turing-machine loop and projects
  Agent Process state through AgentFS;
- local hosts: a file-backed Rust terminal UI, direct management commands, and
  the native Alan for macOS terminal workspace.

The complete Service Manager boot path and the final Alan for macOS-to-Alan OS
attachment are not implemented yet. The attachment decision is deliberately
deferred by [ADR-0029](docs/adr/0029-remove-daemon-era-surfaces-before-replacement-design.md).

## Execution model

An agent is an ordinary Process whose file layout follows the Agent Process
convention:

```text
Agent Executable
    -> Agent Process in /proc/<pid>
        -> AgentFS view in /agent/<pid>
            -> Agent Machine tape, state, requests, actions, and checkpoints
```

- Process owns identity, lifecycle, credentials, descriptors, and exit state.
- Agent Machine owns tape and transition-local state.
- AgentFS owns agent IO, requests, actions, machine files, and live streams.
- rollout and checkpoint files are durable execution evidence.
- Memory Stores own continuity across Agent Processes.

`alan-agent-engine` is the current implementation of the transition loop. It is
not Alan Kernel or the Alan OS system boundary.

## Repository map

```text
crates/
├── ap/                 # aP file-service protocol
├── kernel/             # namespace, Process table, /proc, /srv
├── agentfs/            # /agent file server
├── hostfs/             # host directory file server
├── llmfs/              # LLM Connection file server
├── memfs/              # Memory Store file server
├── routefs/            # file-native message routing
├── editfs/             # editable-buffer file server
├── branchfs/           # branching execution file server
├── shell/              # aP-only Alan Shell builtins
├── agent-protocol/     # Event/Op execution alphabet
├── llm/                # provider adapters
├── agent-engine/       # Agent Execution Engine
├── tools/              # builtin Tool implementations
├── tui/                # file-backed Rust terminal UI
├── shell-core/         # platform-neutral workspace model
├── shell-core-ffi/     # C ABI for shell-core
└── alan/               # CLI host and linked TUI binary

clients/apple/          # Alan for macOS
openspec/               # canonical specifications and active changes
```

The target crate ownership map is recorded in
[ADR-0025](docs/adr/0025-target-crate-architecture.md). Some target services are
still represented only by contracts or partial file-server crates.

## Build and test

Rust 2024 and a current stable toolchain are required.

```bash
just build
just test
just check

cargo test --workspace
cargo test -p alan-agent-engine
cargo test -p alan-agent-protocol
cargo test -p alan-terminal-ui
```

Local macOS development:

```bash
just install-dev
just apple-shell-focused-tests
just apple-shell-ui-smoke
```

## CLI

Running `alan` without a subcommand starts the linked file-backed terminal UI.
The current direct command families are:

```text
alan connection ...
alan init ...
alan workspace ...
alan skills ...
alan shell ...
```

Examples:

```bash
alan init --path /path/to/workspace
alan workspace list
alan workspace info my-workspace

alan connection list
alan connection add chatgpt --profile chatgpt-main
alan connection login chatgpt-main browser
alan connection default set chatgpt-main
alan connection test chatgpt-main

alan skills list --workspace /path/to/workspace
alan shell state
alan shell pane list
```

## Configuration and state

Connection metadata and secrets are separate:

```text
~/.alan/connections.toml       # stable connection profiles
~/.alan/credentials/           # stable secret references/material
~/.alan/auth.json              # managed ChatGPT auth state
~/.alan-dev/...                # isolated dev-channel equivalents
```

These are host-private backing roots for the current implementation, not stable
Alan OS namespace paths or public file-format contracts. Alan OS exposes its
logical file namespace; changing how hosts persist that namespace is a separate
bootstrap and persistence design.

Agent definitions resolve from:

```text
~/.alan/agents/default/
~/.alan/agents/<name>/
<workspace>/.alan/agents/default/
<workspace>/.alan/agents/<name>/
```

Generated workspace state is channel-scoped:

```text
<workspace>/.alan/runtime/<channel>/
├── rollouts/
├── memory/
├── cache/
├── shell-restore/
├── metadata/
└── tmp/
```

`ALAN_CONFIG_PATH` may point directly to an agent configuration file. New
user-facing configuration selects a connection with `connection_profile`; it
does not embed provider secrets.

## Specifications

OpenSpec is the only normative specification and planning surface. Create or
update `openspec/changes/<change-id>/` for proposed behavior. Files under
`openspec/changes/archive/` are historical and non-normative.

## License

Apache License 2.0.
