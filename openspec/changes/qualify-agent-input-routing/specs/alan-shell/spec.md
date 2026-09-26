## MODIFIED Requirements

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
