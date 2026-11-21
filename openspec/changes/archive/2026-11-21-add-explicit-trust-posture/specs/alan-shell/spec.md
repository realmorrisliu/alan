# Spec Delta

## MODIFIED Requirements

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
the current trusted-user-selected approval posture and actual execution isolation.
Namespace file access and service credentials retain their existing authority.

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
- **AND** the trusted-user-selected approval posture and isolation govern execution;
  confined native execution retains its explicit Host Mount boundaries and
  Host-account native execution does not fabricate aP access

#### Scenario: A namespace path is used as shell data
- **WHEN** an authorized `!` command uses a namespace path in a recognized data
  position such as `echo`/`printf` data, a Git commit message, or an AWK
  assignment/program
- **THEN** the data argument remains the original namespace path text
- **AND** in confined execution the redirection target resolves only within the
  authorized Host Mount; explicit Host-account execution follows its selected
  native path semantics without rewriting command data

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
