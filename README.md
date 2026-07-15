# Alan

Alan is a programmable personal computing environment. The repository is in
early development and currently contains three usable layers:

- Alan OS substrate crates for namespaces, mounts, files, descriptors,
  Processes, `/proc`, `/srv`, and file-server composition;
- an Agent Execution Engine that runs the AI Turing-machine loop and projects
  Agent Process state through AgentFS;
- local hosts: a file-backed Rust terminal UI, direct management commands, and
  the native Alan for macOS terminal workspace.

The dedicated system Host boots the Service Manager and Root Agent Process.
Alan for macOS attaches to the matching stable/dev Host over its protected aP
endpoint; it renders Agent Processes by boot ID and PID without owning their
lifecycle or embedding Alan OS.

Package Service is the system owner for installed Skill distributions. It
publishes `/srv/package`; Quartermaster runs as the ordinary `/bin/q` Process.
Installing changes the catalog only. A Process sees immutable package content
at `/lib/pkg/<package-id>` only when its launch context carries an explicit
package reference.

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
├── shell-core/         # platform-neutral shell surface model
├── shell-core-ffi/     # C ABI for shell-core
└── alan/               # CLI host and linked TUI binary

clients/apple/          # Alan for macOS
openspec/               # canonical specifications and active changes
```

The target crate ownership map is recorded in
[ADR-0025](docs/adr/0025-target-crate-architecture.md). Some target services are
still represented only by contracts or partial file-server crates.

## Build and test

Rust 2024 and the repository-pinned Rust 1.97.0 toolchain are required.

```bash
just build
just test
just quality
just check
just install-hooks

cargo test --workspace
cargo test -p alan-agent-engine
cargo test -p alan-agent-protocol
cargo test -p alan-terminal-ui
```

`just quality` is the canonical non-mutating clean-code and architecture gate
used by the versioned pre-commit hook and required CI. CI remains authoritative
because local hooks can be bypassed with `--no-verify`.

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
alan host ...
alan connection ...
alan skills ...
alan shell ...
```

Examples:

```bash
alan host legacy-state inspect
alan host legacy-state cleanup --source-root /path/to/former/project

alan connection list
alan connection add chatgpt --profile chatgpt-main
alan connection login chatgpt-main browser
alan connection default set chatgpt-main
alan connection test chatgpt-main

alan skills validate /path/to/my-skill
alan shell state
alan shell pane list
```

Host files do not enter Alan OS because `alan` was launched from their
directory. Authorize a Host Mount explicitly, then use its Alan OS path from
Alan Shell. The retired `alan init`, `alan workspace`, and boot-time `--agent`
surfaces have no compatibility aliases.

Package management is performed inside Alan Shell, over namespace paths:

```text
q install --name my-skills /mnt/import/my-skills
q list
q upgrade my-skills /mnt/import/my-skills
q uninstall my-skills
```

`q` never receives a raw Host path or fetches remote URLs. The Host directory
must already be authorized and mounted beneath an Alan OS namespace path such
as `/mnt/import`.

## Configuration and state

Durable state is separated by owner and install channel:

```text
~/Library/Application Support/Alan/System Store/<channel>/
├── services/agent-runtime/    # rollout, checkpoint, cache, tmp, metadata
├── services/connections/      # non-secret connection metadata
├── services/memory/           # Memory Store backing
└── services/packages/         # package-owned state and explicit imports

~/Library/Application Support/Alan/Host Store/<channel>/
├── credentials/               # Host-owned secret material
└── auth.json                  # Host-managed provider auth
```

These are Host-private backing roots, never Process identity or implicit
mounts. Agent Definitions and Skills enter a Process only through descriptors
or installed Alan OS references. Memory Stores use explicit descriptors such
as `/memory`; raw backing paths never enter prompts or Agent-visible files.

On upgrade, recognized generated legacy state is removed and connection state
is migrated, verified, and only then deleted. Possibly authored Agent, persona,
policy, Skill, and Memory trees are reported but remain untouched until an
explicit `alan host legacy-state import` succeeds.

`ALAN_CONFIG_PATH` may point directly to an agent configuration file. New
user-facing configuration selects a connection with `connection_profile`; it
does not embed provider secrets.

## Specifications

OpenSpec is the only normative specification and planning surface. Create or
update `openspec/changes/<change-id>/` for proposed behavior. Files under
`openspec/changes/archive/` are historical and non-normative.

## License

Apache License 2.0.
