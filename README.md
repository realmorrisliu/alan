<p align="center">
  <img src="website/public/alan-riso-mascot.jpg" alt="Alan — Risograph illustration of a rabbit at a retro terminal with Turing paper tape" width="190" />
</p>

<h1 align="center">Alan</h1>

<p align="center">
  <strong>Everything is a file. Even an agent.</strong><br />
  A programmable personal computing environment built on <code>alan9</code>.
</p>

<p align="center">
  <a href="https://alanworks.app">Website</a> ·
  <a href="https://realmorrisliu.com/thoughts/tags/alan">Essays</a> ·
  <a href="LICENSE">Apache-2.0</a> ·
  <a href="docs/standalone_cli_distribution.md">Install</a> ·
  <a href="openspec/">OpenSpec</a>
</p>

---

```bash
curl -fsSL https://alanworks.app/install.sh | sh   # install signed/notarized binary (macOS/Linux)
alan login            # connect ChatGPT, OpenRouter, Vertex AI, Magpie, or OpenAI
alan                  # boot a foreground alan9 instance & open the terminal UI
git diff | alan       # pipe stdin for a single task -> stdout
alan update           # update to the latest release
```

Alan is a programmable personal computing environment. The repository is in
early development. Its retained implementation contains:

- alan9 substrate crates for namespaces, mounts, files, descriptors,
  Processes, `/proc`, `/srv`, and file-server composition;
- an Agent Execution Engine that runs the AI Turing-machine loop and projects
  Agent Process state through AgentFS;
- a terminal Shell entry, an Agent renderer and direct management commands.

Alan for macOS is retired; its App, helper and shell-core/FFI source has been
removed, together with the Ghostty dependency and `alan shell` desktop-control
command. The file-native Alan Shell remains supported. Alan is standard-terminal-native: it
runs in any ordinary POSIX/VT terminal (Ghostty, iTerm2, Kitty, WezTerm,
Terminal.app, tmux, SSH) and depends on no particular terminal host.
See [ADR-0054](docs/adr/0054-retire-desktop-client-prefer-terminal-hosts.md) and
[ADR-0061](docs/adr/0061-terminal-native-os-boundary-quartermaster-and-first-party-providers.md).

Each bare `alan` invocation with `ALAN_INSTANCE_RUNTIME_DIR` unset or set to a
directory not in use by another invocation boots and owns a foreground alan9
instance and its Root Agent Process. Terminal sessions using separate runtime
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

Standalone CLI distribution (prebuilt releases + local source workflow):

```bash
curl -fsSL https://alanworks.app/install.sh | sh                       # stable channel (default)
curl -fsSL https://alanworks.app/install.sh | sh -s -- --channel beta  # rolling beta from main
alan update                                        # built-in self-updater (`alan update channel beta|stable`)

just install                                       # developer workflow: build & install from source
just standalone-distribution-test                  # verify local source installer & archive contract
bash scripts/test-remote-installer.sh              # verify remote POSIX curl installer contract
```

Remote curl installation, `alan update` (`stable` via `latest.json` and `beta` via
`beta.json`), Apple Developer ID signing and notarization, the release manifest
schema, and `.alan-cli-manifest` ownership are documented
[here](docs/standalone_cli_distribution.md). Desktop source has been removed;
any ordinary terminal hosts Alan.

## CLI

Running bare `alan` with terminal stdin and stdout starts its foreground
instance and opens the file-backed Agent renderer for that instance's
`/agent/root`. With redirected stdin, the invocation submits one Agent task,
writes the final answer to stdout, sends diagnostics to stderr, and returns the
task's exit status. A leading `!` in the interactive renderer requests a
command through the governed `bash` Tool; it does not execute a host command
directly.
The two-line prompt shows project, model and trust information above `: `
for Agent input or `! ` for an explicit command. Ordinary project working
directories auto-mount `ReadWrite` on launch (excluding `/`, `$HOME`, and
`/tmp`; pass `--no-auto-mount` to opt out). Use `/login` to connect a provider,
`/project` to manage Host Mounts, `/model` to switch models or reasoning effort,
`/issue` (or `/bug`) to draft and confirm a redacted GitHub issue,
`/new` or `/fork` to start fresh or edit an earlier prompt, `/permissions` or
`Shift+Tab` to toggle trust posture (`governed` ↔ `confined/trusted`),
`Ctrl+Z` to suspend, and `/status` to inspect current state.
Already-admitted work keeps its earlier model binding; an unavailable catalog
is reported explicitly.

Tool results have compact summaries. Press Ctrl+O to inspect retained details,
Space/b to page, and Esc to return to the draft. Completion candidates appear
below the input; Tab inserts a candidate and Enter executes a selected slash
command. `/help` lists the available controls.

The current direct command families are:

```text
alan login [provider]
alan update [--check]
alan host ...
alan connection ...
alan q ...
alan workflow ...
alan skills ...
```

Examples:

```bash
alan login            # guided OAuth/key onboarding (chatgpt, openrouter, vertex, magpie, openai)
alan update --check   # check for a newer release via https://alanworks.app/latest.json

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

Outside the auto-mounted project `cwd` (or with `--no-auto-mount`), Host files
never enter alan9 implicitly. Authorize any additional Host Mount explicitly
via `/project`, then use its alan9 path from Alan Shell. The retired `alan init`,
`alan workspace`, and boot-time `--agent` surfaces have no compatibility aliases.

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
