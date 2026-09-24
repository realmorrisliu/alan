# alan-shell Specification

## Purpose
Defines Alan Shell as a protocol-only, application-agnostic namespace client
with generic file/process builtins, concurrent stream tailing, composable Agent
Process interaction, and a line-oriented stdio driver.

## Requirements

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
- **WHEN** a user writes a message to `/agent/<pid>/io/input` (one message is one
  framed unit, committed on clunk per the aP commit-on-clunk convention) and tails
  `/agent/<pid>/io/output`
- **THEN** the agent's streamed response prints in the shell, and a turn starts
  only on the complete message — never on a partial/truncated write
- **AND** this uses the same builtins that operate any process's IO

### Requirement: The shell tails streams concurrently with input
`alan-shell` SHALL support tailing a stream while still accepting user input, so
a streamed response prints as it arrives without blocking the prompt.

#### Scenario: A streamed response arrives
- **WHEN** the user is tailing an output stream
- **THEN** new records print as they arrive
- **AND** the user can still type and submit further input

### Requirement: The first driver is line-oriented stdio
The Alan Shell StdioDriver SHALL remain a minimal line-oriented driver for
namespace operations when used without the terminal renderer. Rich terminal
rendering SHALL be provided by the separate `alan-terminal-ui` crate; Alan
Shell itself SHALL remain aP-only and MUST NOT acquire renderer or Agent runtime
dependencies.

#### Scenario: The shell runs without a renderer
- **WHEN** the StdioDriver receives explicit Shell builtin input
- **THEN** a user can list, read, write, tail, and spawn through the namespace
- **AND** Ratatui rendering remains outside `alan-shell`

### Requirement: An explicitly launched interactive Alan Shell is an ordinary Process
An explicitly launched interactive Alan Shell SHALL run as an ordinary Alan OS
Process with Alan OS credentials, a namespace, descriptors, cwd, PID, and
parentage; executables it invokes SHALL become child Processes. A renderer host
SHALL attach input and output to that Process rather than acting as a hidden
execution manager. This requirement applies to Shell Process entry such as
Local Entry Service and does not apply to the bare `alan` renderer, which
attaches directly to `/agent/root`.

#### Scenario: A renderer attaches to an explicit Shell Process
- **WHEN** Local Entry Service creates `/bin/alan-shell` for an interactive
  renderer
- **THEN** it creates an ordinary Shell Process with Alan OS credentials,
  namespace, descriptors, cwd, PID, and parentage
- **AND** the renderer attaches to that Process instead of creating a hidden
  Shell Process

### Requirement: Bare Alan attaches to the Root Agent
Running bare `alan` SHALL start or attach to the matching dedicated Alan OS
Host. When stdin and stdout are terminals it SHALL attach the terminal renderer
to the Host-managed `/agent/root`. When stdin is redirected it SHALL submit
stdin as one task to that Agent and follow the one-shot standard-stream
contract. If stdin is a terminal but stdout is not, it SHALL report an error
instead of waiting for terminal EOF. The CLI MUST NOT privately boot an Agent
Runtime or select an Agent Definition as Host startup behavior.

#### Scenario: User runs alan with no subcommand
- **WHEN** the system Host is ready and both stdin and stdout are terminals
- **THEN** the client attaches the file-backed renderer to `/agent/root`
- **AND** the Root Agent remains the Process and execution authority

#### Scenario: User runs alan with redirected IO
- **WHEN** stdin is not a terminal, regardless of stdout
- **THEN** stdin is submitted as one task and only the final answer is written
  to stdout
- **AND** diagnostics go to stderr and task failure is reported by a nonzero
  exit code
- **AND** it does not emit terminal UI control sequences

#### Scenario: One-shot starts during Root Agent PID handoff
- **WHEN** redirected `alan` starts while the Service Manager publishes a
  detached old PID or temporarily publishes an empty or zero PID during
  supervised restart, or the PID changes between initial activity snapshot,
  tail attachment, and final idle verification
- **THEN** it retries the complete activity/tail/idle attachment sequence
  within a bounded startup window, attaching to the replacement once ready
- **AND** it closes partial tails before retrying and preserves same-PID busy
  or attachment errors
- **AND** it reports the attach error after retries are exhausted without
  submitting task input

#### Scenario: User redirects stdout without redirecting stdin
- **WHEN** stdin is a terminal and stdout is not a terminal
- **THEN** the CLI reports that interactive mode requires terminal stdout
  instead of reading stdin until EOF
- **AND** it does not start or attach to the Alan OS Host

#### Scenario: One-shot task fails before tape persistence
- **WHEN** the Root Agent reports a running task failure before writing the
  submitted user message to tape
- **THEN** the client reports the task error on stderr with a nonzero exit code
- **AND** the Root Agent exposes a terminal UI error before idle without
  requiring a tape record
- **AND** any intermediate assistant content is not reported as a successful
  final answer

#### Scenario: One-shot task emits an intermediate assistant preamble
- **WHEN** a successful task emits assistant content with tool calls followed
  by a final assistant answer
- **AND** the client observes `Idle` before its tape tail delivers that final
  answer
- **THEN** the client reads the pinned Root Agent Process tape after `Idle` and
  correlates the latest assistant answer with the submitted user record
- **AND** only that final answer is written to stdout
- **AND** a missing correlated final answer is reported as an unknown outcome
  rather than returning intermediate content

#### Scenario: A prior task settles while one-shot is attaching
- **WHEN** a prior Root Agent task reaches `Idle` after one-shot startup begins
- **AND** its tape records or UI `Idle` event arrive after an earlier tail
  snapshot
- **THEN** the client confirms `Idle` before opening fresh tape and UI tails,
  then rechecks the pinned Root Agent PID and activity before submitting
- **AND** the fresh tape baseline includes the prior task even when its prompt
  matches the new input
- **AND** an `Idle` or error event completes the new task only after its own
  correlated `Running` event

#### Scenario: One-shot task runs longer than expected
- **WHEN** the redirected task has not reached a terminal outcome
- **THEN** the client continues waiting without an arbitrary client-side timeout
- **AND** Ctrl-C remains pending until `Running` confirms that the submitted
  task has been accepted, then writes a turn interrupt through
  `/agent/root/machine/ctl`
- **AND** the client reports interruption without terminating the Root Agent
  Process

#### Scenario: A client overlaps another Root Agent task
- **WHEN** another TTY or redirected `alan` client owns the channel's
  nonblocking task-submission lease, or the Root Agent remains running or
  paused after its prior renderer exited
- **THEN** the new client fails clearly and does not attach its result to the
  other client's task
- **AND** it does not write new task input while the Root Agent is active
- **AND** the user can retry after the active task settles

#### Scenario: Root Agent Process changes during redirected task
- **WHEN** the Root Agent PID changes while the client waits for the submitted
  task
- **THEN** the client resolves one replacement Root Agent PID and opens both
  tape and UI tails against that concrete Process path
- **AND** it retries the pair if the Root Agent PID changes while either tail
  is opening
- **AND** it recovers a result only from records appended after the captured
  tape baseline or from UI activity correlated to this submission
- **AND** if the replacement history cannot establish that correlation, it
  reports that the outcome is unknown rather than reusing an older identical
  prompt
- **AND** it never resubmits input or duplicates stdout

#### Scenario: A one-shot tail closes before Root Agent polling observes restart
- **WHEN** either tape or UI tail reaches EOF or returns an IO error while the
  supervised Root Agent is being replaced
- **THEN** the client checks the published Root Agent PID before failing the
  task
- **AND** it waits through a temporarily unavailable Root Agent and attaches
  both tails to the replacement once its PID is published
- **AND** it recovers a correlated result from replacement history, or reports
  an unknown outcome when that history cannot establish the submitted result
- **AND** if the Root Agent PID is unchanged, it reports the original tail
  failure after one PID-poll grace interval instead of retrying a broken stream

### Requirement: Terminal task input is distinct from Shell evaluation
The interactive bare-`alan` renderer SHALL submit each ordinary task entry as
one task to `/agent/root/io/input`. On the normal composer submission path, it
SHALL dispatch leading-`/` entries as renderer-local slash commands; recognized
commands perform their local action and unknown commands report a local error.
Slash commands are not Agent task entries. Input responding to an active Agent
yield or form SHALL continue through that interaction's existing flow. The
renderer SHALL NOT parse arbitrary text as Alan Shell builtins or infer
execution authority from shell-looking text. A leading `!` SHALL explicitly
request execution of the exact remainder using the existing `bash` Tool.
Agent-originated effects MUST continue through the existing Agent Runtime,
Tool governance, Namespace access, explicit Host Mounts, credentials, and
sandbox.

#### Scenario: Renderer slash commands stay local
- **WHEN** the normal composer submission path receives a slash command
- **THEN** a recognized command performs its renderer-local action
- **AND** an unknown command reports a local error
- **AND** neither command is submitted as an Agent task

#### Scenario: Shell-looking text is entered in the terminal renderer
- **WHEN** a user submits text such as `ls /mnt/project` in the interactive
  renderer
- **THEN** the renderer submits it as Agent task text, not as a StdioDriver
  command
- **AND** any resulting operation is subject to the Agent's existing Tool and
  Namespace authority

#### Scenario: User explicitly requests a shell command
- **WHEN** the user submits `!<command>` in the terminal renderer
- **THEN** the Agent is asked to run the exact `<command>` through its existing
  `bash` Tool without rewriting or adding commands
- **AND** Tool policy, required approval, sandbox, and explicit Host Mount
  boundaries still apply

#### Scenario: A namespace path is used as shell data
- **WHEN** an authorized `!` command uses a namespace path in a recognized data
  position such as `echo`/`printf` data, a Git commit message, or an AWK
  assignment/program
- **THEN** the data argument remains the original namespace path text
- **AND** the redirection target still resolves only within the authorized
  Host Mount

#### Scenario: AWK preserves data while projecting supported file paths
- **WHEN** an authorized AWK command has a namespace path in a `-v` assignment,
  a positional `name=value` operand after its program, or program text and the
  selected sandbox permits that AWK script form
- **AND** it uses a namespace path for a `-f` script or input file
- **THEN** assignment and program text keep the namespace path unchanged
- **AND** the script and input file operands resolve to their authorized Host
  Mount paths, including input operands after `--`
- **AND** a conservative sandbox may reject opaque AWK script forms rather than
  executing them without protected-path validation

#### Scenario: Explicit StdioDriver builtin is entered
- **WHEN** the StdioDriver receives the same text as explicit builtin input
- **THEN** only its fixed generic grammar decides whether the operation is
  supported
- **AND** unknown text fails rather than being inferred as a script or action
