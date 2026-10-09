# Alan

Alan is a programmable personal computing environment. The repository is in
early development. Its retained implementation contains:

- alan9 substrate crates for namespaces, mounts, files, descriptors,
  Processes, `/proc`, `/srv`, and file-server composition;
- an Agent Execution Engine that runs the AI Turing-machine loop and projects
  Agent Process state through AgentFS;
- a terminal Shell entry, an Agent renderer and direct management commands.

Alan for macOS is retired; its App, helper and shell-core/FFI source has been
removed, together with the Ghostty dependency and `alan shell` desktop-control
command. The file-native Alan Shell remains supported. Herdr is the preferred
terminal host, without making Alan dependent on Herdr or claiming native Alan
agent detection already exists.
See [ADR-0054](docs/adr/0054-retire-desktop-client-prefer-terminal-hosts.md).

Each bare `alan` invocation with `ALAN_INSTANCE_RUNTIME_DIR` unset or set to a
directory not in use by another invocation boots and owns a foreground alan9
instance and its Root Agent Process. Herdr sessions using separate runtime
directories have separate Roots and endpoints while product stores remain
shared. Reusing one explicit runtime directory allows only one owner; another
invocation fails to acquire it rather than sharing its Root. Detaching a
terminal view is separate from exiting the Alan process; when Alan exits, its
instance stops. A new invocation never automatically adopts earlier work;
resuming durable execution requires explicit user selection, whose recovery
flow remains in active implementation work.

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
not alan9 Kernel or the alan9 system boundary.

[ADR-0055](docs/adr/0055-agent-machine-composes-typed-capabilities.md) accepts
deterministic, typed evaluation and generation operations within one Machine.
Jev support is not implemented. System 1/System 2 are not mandatory child
Processes or permission levels. Current execution remains generation-based.

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
└── alan/               # CLI host and linked TUI binary

openspec/               # canonical specifications and active changes
```

The target crate ownership map is recorded in
[ADR-0025](docs/adr/0025-target-crate-architecture.md). Some target services are
still represented only by contracts or partial file-server crates.

## Build and test

Rust 2024 and the repository-pinned Rust 1.97.0 toolchain are required.
The canonical quality gate also expects `python3`, `rg` and the pinned OpenSpec
CLI on `PATH`. Python is also used to read Cargo's build-artifact reports for installation.

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

`just cache-status` reports actual Cargo output roots, allocated size, ownership,
and activity. `just cache-clean` previews eligible cleanup; add `--apply` to remove
registered idle compiler output. Existing unregistered caches and directories
containing harness reports or other unknown entries are retained. These commands
also inspect marked temporary Connection/Package Service stores in the current
Host temporary directory. Only a dead owner, exclusive marker lock, unchanged
ownership and no open consumer permit cleanup. Reused/live PIDs, unknown content,
and historical unmarked PID caches are retained. Build/install/
release and quality workflows hold output leases; ordinary Cargo commands still
use Cargo's own locks. Keep final reports outside disposable compiler directories.

`just quality` is the canonical non-mutating clean-code and architecture gate
used by the versioned pre-commit hook and required CI. CI remains authoritative
because local hooks can be bypassed with `--no-verify`.

Standalone CLI distribution:

```bash
just install
just standalone-distribution-test
```

Standalone installation and release usage is documented
[here](docs/standalone_cli_distribution.md). Desktop source has been removed;
Herdr is the preferred external terminal host.

## CLI

Running bare `alan` with terminal stdin and stdout starts its foreground
instance and opens the file-backed Agent renderer for that instance's
`/agent/root`. With redirected stdin, the invocation submits one Agent task,
writes the final answer to stdout, sends diagnostics to stderr, and returns the
task's exit status. A leading `!` in the interactive renderer requests a
command through the governed `bash` Tool; it does not execute a host command
directly.
The two-line prompt shows project, model and activity information above `: `
for Agent input or `! ` for an explicit command. Use `/project` to choose a
project and approve its access, `/status` to inspect current state, and `/model`
to inspect the Connection-owned catalog and select a model for subsequent input.
Already-admitted work keeps its earlier model binding; an unavailable catalog
is reported explicitly.

Tool results have compact summaries. Press Ctrl+O to inspect retained details,
Space/b to page, and Esc to return to the draft. Completion candidates appear
below the input; Tab inserts a candidate and Enter executes a selected slash
command. `/help` lists the available controls.

The current direct command families are:

```text
alan host ...
alan connection ...
alan skills ...
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
```

Alan Shell operates on the alan9 namespace inside the running instance; it does
not provide desktop window or pane control.

Host files do not enter alan9 because `alan` was launched from their
directory. Authorize a Host Mount explicitly, then use its alan9 path from
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
must already be authorized and mounted beneath an alan9 namespace path such
as `/mnt/import`.

## Configuration and state

Durable state is separated by owner:

```text
~/Library/Application Support/Alan/System Store/
├── services/agent-runtime/    # rollout, checkpoint, cache, tmp, metadata
├── services/connections/      # non-secret connection metadata
├── services/memory/           # Memory Store backing
└── services/packages/         # package-owned state and explicit imports

~/Library/Application Support/Alan/Host Store/
├── credentials/               # Host-owned secret material
└── auth.json                  # Host-managed provider auth
```

These are Host-private backing roots, never Process identity or implicit
mounts. Agent Definitions and Skills enter a Process only through descriptors
or installed alan9 references. Memory Stores use explicit descriptors such
as `/memory`; raw backing paths never enter prompts or Agent-visible files.

Old stable/dev stores remain unchanged until an explicit source is adopted with
`alan legacy-state migrate-installation --from stable|dev`. Startup does not
select one automatically. See the [installation and migration guide](docs/standalone_cli_distribution.md).
Authored legacy content uses an explicit `alan legacy-state import`; it is never
an implicit definition or Skill overlay.

`ALAN_CONFIG_PATH` may point directly to an agent configuration file. New
user-facing configuration selects a connection with `connection_profile`; it
does not embed provider secrets.

## Specifications

OpenSpec is the only normative specification and planning surface. Create or
update `openspec/changes/<change-id>/` for proposed behavior. Files under
`openspec/changes/archive/` are historical and non-normative.

## License

Apache License 2.0.
