## MODIFIED Requirements

### Requirement: Alan Shell is a general namespace client over aP
Alan OS SHALL provide `alan-shell`, a client that operates the namespace only
through aP (the `alan-ap` protocol): walk/list, read, write, tail, and spawn. It
SHALL depend on the protocol alone and SHALL NOT link any file server or backend
crate. It SHALL hold no application state beyond the namespace.
This requirement describes the reusable aP command library and explicit
standalone driver, not the unified bare-`alan` Agent interaction. Agent Machine
owns unified input routing and execution while reusing these command facilities.

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
This requirement describes the reusable aP command library and explicit
standalone driver, not the unified bare-`alan` Agent interaction. Agent Machine
owns unified input routing and execution while reusing these command facilities.

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
This requirement describes the reusable aP command library and explicit
standalone driver, not the unified bare-`alan` Agent interaction. Agent Machine
owns unified input routing and execution while reusing these command facilities.

#### Scenario: A user talks to an agent
- **WHEN** a user writes a message to `/agent/<pid>/io/input` (one message is one
  framed unit, committed on clunk per the aP commit-on-clunk convention) and tails
  `/agent/<pid>/io/output`
- **THEN** the agent's streamed response prints in the shell, and a turn starts
  only on the complete message — never on a partial/truncated write
- **AND** this uses the same builtins that operate any process's IO

### Requirement: The first driver is line-oriented stdio
The Alan Shell StdioDriver SHALL remain a minimal line-oriented driver for
namespace operations when used without the terminal renderer. Rich terminal
rendering SHALL be provided by the separate `alan-terminal-ui` crate; Alan
Shell itself SHALL remain aP-only and MUST NOT acquire renderer or Agent runtime
dependencies.
This requirement describes the reusable aP command library and explicit
standalone driver, not the unified bare-`alan` Agent interaction. Agent Machine
owns unified input routing and execution while reusing these command facilities.

#### Scenario: The shell runs without a renderer
- **WHEN** the StdioDriver receives explicit Shell builtin input
- **THEN** a user can list, read, write, tail, and spawn through the namespace
- **AND** Ratatui rendering remains outside `alan-shell`

### Requirement: Bare Alan attaches to the Root Agent
Running bare `alan` SHALL start or attach to the matching dedicated Alan OS
Host. When stdin and stdout are terminals it SHALL attach the terminal renderer
to the Host-managed `/agent/root`. When stdin is redirected it SHALL submit
all stdin as one input to that Agent using the same prefix/routing contract as
interactive input and follow the one-shot standard-stream contract. If stdin is a terminal but stdout is not, it SHALL report an error
instead of waiting for terminal EOF. The CLI MUST NOT privately boot an Agent
Runtime or select an Agent Definition as Host startup behavior.

#### Scenario: User runs alan with no subcommand
- **WHEN** the system Host is ready and both stdin and stdout are terminals
- **THEN** the client attaches the file-backed renderer to `/agent/root`
- **AND** the Root Agent remains the Process and execution authority

#### Scenario: User runs alan with redirected IO
- **WHEN** stdin is not a terminal, regardless of stdout
- **THEN** stdin is submitted once with the unified input routing semantics
- **AND** stdout carries command output for a command or the final answer for Agent work
- **AND** diagnostics go to stderr and task failure is reported by a nonzero
  exit code
- **AND** it does not emit terminal UI control sequences

#### Scenario: One-shot starts during Root Agent PID handoff
- **WHEN** Root Agent publication is unavailable, stale or changes during initial attachment
- **THEN** the client retries attachment within a bounded startup window against one concrete Process
- **AND** it closes partial tails and reports failure without submitting when attachment cannot stabilize
- **AND** accepted input is correlated by submission identity, not a global idle observation

#### Scenario: User redirects stdout without redirecting stdin
- **WHEN** stdin is a terminal and stdout is not a terminal
- **THEN** the CLI reports that interactive mode requires terminal stdout
  instead of reading stdin until EOF
- **AND** it does not start or attach to the Alan OS Host

#### Scenario: One-shot task fails before tape persistence
- **WHEN** accepted work fails before its input is persisted to Tape
- **THEN** its correlated failure appears on stderr with a nonzero exit
- **AND** intermediate Agent text is not presented as a successful final answer
- **AND** missing evidence is not interpreted as successful command completion

#### Scenario: One-shot task emits an intermediate assistant preamble
- **WHEN** a successful Agent-route task emits assistant content with tool calls followed
  by a final assistant answer
- **AND** the client observes `Idle` before its tape tail delivers that final
  answer
- **THEN** the client reads the pinned Root Agent Process tape after `Idle` and
  correlates the latest assistant answer with the submitted user record
- **AND** only that final answer is written to stdout
- **AND** a missing correlated final answer is reported as an unknown outcome
  rather than returning intermediate content

#### Scenario: A prior task settles while one-shot is attaching
- **WHEN** another submission settles while a redirected client attaches or waits in the queue
- **THEN** that result cannot complete the new client's submission
- **AND** acceptance, progress and terminal outcome are correlated to the new submission identity
- **AND** identical prompt text, a shared Idle event or Tape position alone is insufficient

#### Scenario: One-shot task runs longer than expected
- **WHEN** redirected work remains active without a terminal outcome
- **THEN** the client follows its correlated progress without an arbitrary client-side execution timeout
- **AND** Ctrl-C requests cancellation of that submission through the runtime control boundary without terminating the Agent
- **AND** a required answer with no response channel fails instead of waiting indefinitely

#### Scenario: A client overlaps another Root Agent task
- **WHEN** another TTY or redirected `alan` client owns the channel's
  nonblocking task-submission lease, or the Root Agent remains running or
  paused after its prior renderer exited
- **THEN** accepted ordinary input enters the Agent-owned ordered queue
- **AND** each client observes only the outcome correlated with its own submission
- **AND** admission failure is explicit rather than attaching to another task
- **AND** client leases do not become a separate queue or execution authority

#### Scenario: Root Agent Process changes during redirected task
- **WHEN** Root Agent PID changes while a redirected client follows accepted input
- **THEN** it attaches result/state streams against one concrete replacement Process and retries if identity changes while opening
- **AND** only reliable records correlated to the original submission can recover its outcome
- **AND** missing correlation reports unknown outcome rather than matching an older identical prompt
- **AND** pending work remains paused after recovery, and input is never automatically resubmitted or stdout duplicated

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
Alan SHALL route complete input submissions inside the attached Agent Runtime.
The first `!` selects exact command handling; the first `:` selects Agent
interpretation; either prefix applies once to the whole submission and bypasses
intent classification. Unprefixed input SHALL remain Agent input until qualified
automatic routing is enabled, then SHALL use typed command/Agent/ambiguous intent
classification without command rewriting. This contract SHALL apply to terminal
and redirected input. Explicit routing MUST NOT grant authority. Pending request
responses SHALL be consumed by their request before ordinary routing.

#### Scenario: Explicit command is submitted
- **WHEN** the user submits `!git status`
- **THEN** the Machine dispatches the exact `git status` body through the governed native shell adapter
- **AND** it does not generate or rewrite the command and the Host shell performs execution
- **AND** applicable rights, policy, approvals and sandbox checks still apply

#### Scenario: Agent override is submitted
- **WHEN** the user submits `:!explain this text`
- **THEN** the Agent receives `!explain this text` without recursively parsing its prefix
- **AND** Agent interpretation can use governed Tools when the request calls for them

#### Scenario: Empty override is submitted
- **WHEN** a submission contains only `!` or `:` and whitespace
- **THEN** it reports missing content and starts no action

#### Scenario: Slash controls and absolute paths are distinguished
- **WHEN** top-level input starts with `/`
- **THEN** recognized slash controls retain their explicit behavior and unknown controls fail
- **AND** an absolute executable path is submitted through `!`, such as `!/usr/bin/git status`

#### Scenario: A pending form contains prefix characters
- **WHEN** input answers an active form or confirmation and begins with `!` or `:`
- **THEN** the characters are response data and do not start a command or new Agent task

#### Scenario: Automatic classification is not enabled
- **WHEN** unprefixed `git status` is submitted in the explicit-prefix delivery slice
- **THEN** it follows the existing governed Agent interpretation path
- **AND** the product does not claim automatic command routing is enabled

#### Scenario: Classification cannot determine intent
- **WHEN** qualified automatic routing returns ambiguous intent
- **THEN** Alan requests clarification before dispatching either execution path

#### Scenario: Evaluation fails
- **WHEN** evaluation is unavailable, times out or returns a malformed result
- **THEN** Alan uses bounded governed Agent fallback or reports unavailable generation
- **AND** evaluation failure never selects direct execution or bypasses cancellation

#### Scenario: Direct command fails
- **WHEN** command parsing, authorization or execution fails
- **THEN** the failure is reported without automatic rewriting, repair or redispatch

#### Scenario: Redirected input uses an explicit prefix
- **WHEN** stdin contains `!git status` and then reaches EOF
- **THEN** the whole input is submitted once for command execution
- **AND** no terminal is implicitly opened to collect additional input

#### Scenario: Renderer slash commands stay local
- **WHEN** normal top-level composer input is a recognized slash control
- **THEN** it uses its existing local or runtime-control behavior without becoming an ordinary submission
- **AND** an unknown slash control reports an error rather than entering automatic routing

#### Scenario: Shell-looking text is entered in the terminal renderer
- **WHEN** the user submits unprefixed `ls /mnt/project`
- **THEN** the explicit-prefix slice treats it as governed Agent input
- **AND** after qualified automatic routing is enabled it is classified without rewriting
- **AND** command-like syntax alone never grants execution authority

#### Scenario: User explicitly requests a shell command
- **WHEN** the user submits `!<command>`
- **THEN** the exact remainder selects governed Host shell execution in the Machine
- **AND** the existing native Tool execution boundary is reused without a generation step
- **AND** required permissions and approvals still apply

#### Scenario: A namespace path is used as shell data
- **WHEN** an aP path occurs in native command text
- **THEN** its text is passed unchanged and has only the selected Host shell's meaning
- **AND** no argument, script body or redirection target is translated to Host backing
- **AND** purely virtual resources use explicitly invoked task-oriented alan9 commands that encapsulate aP

#### Scenario: AWK preserves data while projecting supported file paths
- **WHEN** a governed native command invokes AWK with assignments, program text or file operands
- **THEN** the shell and AWK interpret all arguments in the native filesystem
- **AND** Alan does not rewrite `-f`, input-file operands or embedded aP-looking strings
- **AND** policy may reject execution but may not silently repair it

#### Scenario: Explicit StdioDriver builtin is entered
- **WHEN** the explicit standalone driver receives a builtin command
- **THEN** its deterministic supported grammar decides the operation
- **AND** unknown or unsupported syntax fails rather than being inferred as a script or action

## ADDED Requirements

### Requirement: Unified commands reuse a governed Host shell
The command route SHALL pass its unchanged body to the selected Host shell through
one governed native Tool execution boundary shared with Agent commands. Native
PATH and filesystem rules SHALL determine executable and operand lookup. Shell
composition, redirection, expansion and multiline scripts SHALL use that shell's
semantics. Alan MUST NOT implement implicit aP lookup, command path rewriting or
fallback shell selection. One script SHALL remain one submission. Explicit `!`
SHALL bypass generation but MUST NOT bypass permissions or sandbox policy.

#### Scenario: Relative executable is invoked
- **WHEN** `!./tool arg` executes with an authorized native project cwd
- **THEN** the Host shell resolves `./tool` relative to that directory
- **AND** Alan does not rewrite it to its namespace `/bin`

#### Scenario: Pipeline or multiline script is submitted
- **WHEN** an authorized command contains pipes, conditionals, redirection or internal newlines
- **THEN** the complete body is supplied as one script to the selected shell
- **AND** later failures report actual outcomes without claiming earlier effects were rolled back

#### Scenario: Namespace executable name also exists on the Host
- **WHEN** a native command names `q` or another name present in Alan `/bin`
- **THEN** only native shell lookup determines what executes
- **AND** Alan internal control requires an explicitly installed alan9 command that encapsulates namespace execution

### Requirement: Mount context does not imply native filesystem virtualization
Mounted aP resources SHALL determine authorized service reachability, not automatic
prompt inclusion or a matching native path. Delegated Host Mount grants SHALL
supply both HostFS access and native execution authority through their owner.
Virtual mounts MUST NOT imply native access. Scoped native cwd/path metadata SHALL
be supplied by the Host adapter for authorized command context; path strings SHALL
NOT grant access. Shell-facing output MUST NOT be rewritten to unusable aP aliases.

#### Scenario: Agent reads a virtual resource
- **WHEN** an Agent has a mounted virtual document service
- **THEN** a task-oriented alan9 command encapsulates aP access with existing rights and commit semantics
- **AND** ordinary Host `cat` is not transparently redirected to that service

#### Scenario: Native command needs a project path
- **WHEN** the command Process has a delegated local project grant
- **THEN** the Host adapter supplies its authorized native execution cwd/path
- **AND** the same grant constrains native access without exposing unrelated backing paths

### Requirement: Noninteractive results are correlated and cannot wait for absent input
Redirected execution SHALL write command output or the Agent final answer to
stdout, diagnostics to stderr and truthful exit status. If clarification or
approval needs an unavailable response channel it SHALL terminate with a
nonzero result, report completed and incomplete work accurately and MUST NOT
wait indefinitely, guess consent or implicitly open a terminal.

#### Scenario: Command exits unsuccessfully
- **WHEN** a redirected command exits with a nonzero status
- **THEN** Alan reports its correlated output and failure status without generating an explanation

#### Scenario: Clarification has no response channel
- **WHEN** redirected execution requires a user answer after stdin ended and no response channel exists
- **THEN** it fails with a diagnostic on stderr and nonzero exit status
- **AND** any already completed effects are not reported as rolled back

### Requirement: Alan internal control is encapsulated by task-oriented commands
Alan SHALL expose needed internal control operations as ordinary governed alan9
command executables primarily for Agent use. Normal user flows and Agent command
contracts SHALL NOT require aP paths, descriptors or commit-document knowledge.
Advanced users MAY explicitly invoke these commands; developer protocol inspection
MAY remain available. Commands SHALL reuse existing executable discovery, service
owners and caller-scoped authority, without a duplicate Host manager or state store.
Exact command names and schemas SHALL be documented with the delivered operations;
this contract does not authorize a speculative wrapper for every protocol operation.

#### Scenario: Agent controls internal work
- **WHEN** an Agent invokes an installed alan9 command for a supported internal operation
- **THEN** that command validates task arguments and uses aP internally with delegated caller authority
- **AND** it returns useful output, errors and exit status through existing Action evidence
- **AND** the Agent need not construct protocol transactions or manipulate internal file paths

#### Scenario: Command lacks internal authority
- **WHEN** a command can run as a native executable but lacks the requested service capability
- **THEN** the operation is denied without borrowing a broader ambient Host connection
- **AND** knowing a service path or grant ID does not supply the missing authority

#### Scenario: Internal operation is accepted asynchronously
- **WHEN** a command commits an operation whose work has not yet completed
- **THEN** it distinguishes accepted work and its reference from completed success
- **AND** commit errors and unknown outcomes are explicit without automatic resubmission

#### Scenario: Ordinary user edits a project
- **WHEN** a user asks the Agent to edit a project file and then runs a native command
- **THEN** the interaction uses project paths and task results without requiring aP terminology
- **AND** internal service diagnostics do not require the user to repair raw protocol documents
