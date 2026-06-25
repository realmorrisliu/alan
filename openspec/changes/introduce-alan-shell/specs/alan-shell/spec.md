## ADDED Requirements

### Requirement: Alan Shell is a general namespace client over aP
Alan OS SHALL provide `alan-shell`, a client that operates the namespace only
through aP (the `alan-ap` protocol): walk/list, read, write, tail, and spawn. It
SHALL depend on the protocol alone and SHALL NOT link any file server or backend
crate. It SHALL hold no application state beyond the namespace.

#### Scenario: The shell's dependencies are reviewed
- **WHEN** `alan-shell` dependencies are audited
- **THEN** they include `alan-ap` and not `alan-agentfs`, `alan-llmfs`,
  `alan-agent-engine`, or any other server/backend
- **AND** the shell reaches every resource by walking and opening files

### Requirement: The shell has generic builtins, no agent knowledge
`alan-shell` SHALL provide generic builtins, each expressed only through aP
operations: list/walk a directory, read a file (`cat`), write a file (`echo >`),
tail a stream (blocking watch from an offset), and spawn a process. `spawn` SHALL
be defined as the aP process-creation path — opening `/proc/clone` and writing an
exec spec (per `define-plan9-kernel-substrate`) — not a non-file operation. The
shell SHALL NOT provide any agent-specific command, mode, or `attach` sugar.
Control SHALL be writing a command to a `ctl` file by ownership: generic process
control (interrupt, cancel) to the kernel `/proc/<pid>/ctl`, and agent-runtime
control (such as compact, rollback) to the agent-runtime-owned `machine/ctl` in
the `/agent/<pid>` overlay (per `define-agent-file-layout-contract`).

#### Scenario: The same builtins operate any process
- **WHEN** a user inspects a process with `alan-shell`
- **THEN** `cat <pid>/io/output` and `tail <pid>/io/events` work the same whether
  the process is an agent or a compiler
- **AND** there is no agent-only command path

#### Scenario: Spawn uses the aP process-creation path
- **WHEN** a user spawns an executable
- **THEN** the shell opens `/proc/clone` (receiving the new pid at open), writes
  the exec spec (one or more writes), and clunks to commit/start — open → write(s)
  → clunk, per `define-plan9-kernel-substrate`
- **AND** it needs no operation outside aP, and the process starts only at clunk

#### Scenario: A process is controlled
- **WHEN** a user interrupts a process or compacts/rolls back an agent's tape
- **THEN** generic control goes to the kernel `/proc/<pid>/ctl` and runtime
  control goes to the agent-runtime-owned `machine/ctl`
- **AND** no dedicated per-action command exists in the shell, and the kernel ctl
  never carries runtime semantics

### Requirement: Talking to an agent is composition, not a feature
`alan-shell` SHALL let a user converse with an agent purely by composing generic
builtins: writing input to `/agent/<pid>/io/input` and tailing
`/agent/<pid>/io/output`. The shell SHALL NOT contain agent-aware conversation
logic.

#### Scenario: A user talks to an agent
- **WHEN** a user writes a message to `/agent/<pid>/io/input` and tails
  `/agent/<pid>/io/output`
- **THEN** the agent's streamed response prints in the shell
- **AND** this uses the same builtins that operate any process's IO

### Requirement: The shell tails streams concurrently with input
`alan-shell` SHALL support tailing a stream while still accepting user input, so
a streamed response prints as it arrives without blocking the prompt.

#### Scenario: A streamed response arrives
- **WHEN** the user is tailing an output stream
- **THEN** new records print as they arrive
- **AND** the user can still type and submit further input

### Requirement: The first driver is line-oriented stdio
The first `alan-shell` driver SHALL be a minimal line-oriented stdio
read-eval-print loop. Rich terminal rendering SHALL remain the responsibility of
`alan-terminal-ui` and is out of scope for this change.

#### Scenario: The shell runs without a renderer
- **WHEN** `alan-shell` runs as the line-oriented stdio driver
- **THEN** a user can list, read, write, tail, and spawn over the namespace
- **AND** Ratatui rendering is provided later by `alan-terminal-ui`, not by this
  change
