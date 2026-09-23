## MODIFIED Requirements

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

### Requirement: Alan enters the system Shell
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

## REMOVED Requirements

### Requirement: Interactive Alan Shell is an ordinary Process
**Reason**: The terminal CLI is an external renderer/Host client attached to the
existing Root Agent. Creating a second Shell Process for the TTY would add a
redundant identity and lifecycle without serving the first usable task loop.
**Migration**: Keep Alan Kernel Process identity on the Root Agent and Tools;
keep terminal input and rendering at the existing LocalAttachment and
`alan-terminal-ui` boundaries.

## ADDED Requirements

### Requirement: Terminal task input is distinct from Shell evaluation
The interactive bare-`alan` renderer SHALL submit each user entry as one task
to `/agent/root/io/input`. It SHALL NOT parse arbitrary text as Alan Shell
builtins or infer execution authority from shell-looking text. A leading `!`
SHALL explicitly request execution of the exact remainder using the existing
`bash` Tool. Agent-originated effects MUST continue through the existing Agent
Runtime, Tool governance, Namespace access, explicit Host Mounts, credentials,
and sandbox.

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

#### Scenario: AWK preserves program text while projecting supported file paths
- **WHEN** an authorized AWK command has a namespace path in a `-v` assignment
  or program text and the selected sandbox permits that AWK script form
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
