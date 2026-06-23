## ADDED Requirements

### Requirement: Single human-in-the-end auto-approve mode
The agent SHALL operate in a single auto-approve posture in which routine operations proceed without prompting and only operations needing human judgment are escalated. The system SHALL NOT expose multiple selectable approval modes.

#### Scenario: Routine operation proceeds automatically
- **WHEN** the policy classifies an operation as a read or an in-workspace write
- **THEN** the operation proceeds without prompting the user

#### Scenario: No mode switcher
- **WHEN** a user interacts with the agent
- **THEN** there is exactly one approval posture and no selectable mode (such as default/auto-edit/plan/bypass)

### Requirement: Conservative escalation boundary before OS sandboxing
Until OS-level sandbox enforcement is available, the policy SHALL escalate network access, writes outside the workspace, explicitly destructive or irreversible operations, and operations of unknown capability.

#### Scenario: Network access escalates
- **WHEN** an operation is classified as network capability
- **THEN** the policy escalates it for human approval

#### Scenario: Out-of-workspace write escalates
- **WHEN** an operation would write outside the workspace
- **THEN** the policy escalates it for human approval

#### Scenario: Destructive operation escalates
- **WHEN** an operation is explicitly destructive or irreversible
- **THEN** the policy escalates it for human approval

#### Scenario: Unknown capability escalates by default
- **WHEN** an operation's capability cannot be classified
- **THEN** the policy escalates it rather than auto-allowing it

### Requirement: Permissions are never silently widened
The auto-approve posture SHALL NOT remember an approval in a way that broadens future grants, and SHALL NOT write persistent permission grants.

#### Scenario: Approval does not create a standing grant
- **WHEN** a user approves an escalated operation
- **THEN** a later operation that would otherwise escalate still escalates
- **AND** no persistent permission grant is written

### Requirement: Approvals are answered by key without typing
When the agent escalates for approval, the TUI SHALL let the user respond with selection or number keys and SHALL NOT require typing a word into the composer.

#### Scenario: Confirmation via key
- **WHEN** an approval prompt is shown
- **THEN** the user can approve or reject using selection or number keys

#### Scenario: No internal identifiers shown
- **WHEN** an approval prompt is shown
- **THEN** no internal identifier such as `request_id` appears on screen

### Requirement: Approvals show what is being approved and why
An approval prompt SHALL display the operation's content and the policy rationale.

#### Scenario: Edit approval shows the diff
- **WHEN** an edit/write operation is escalated
- **THEN** the prompt shows the structured diff of the change

#### Scenario: Command approval shows the command
- **WHEN** a shell command is escalated
- **THEN** the prompt shows the command line

#### Scenario: Rationale is shown
- **WHEN** an approval prompt is shown
- **THEN** it shows the decision's capability and reason
- **AND** when structured content is unavailable it falls back to the flat preview

### Requirement: Structured input renders as a form
Structured-input requests SHALL render as an inline form with keyboard navigation and validation, not as hand-typed JSON.

#### Scenario: Form honors question kinds
- **WHEN** a structured-input request contains questions of varying kinds (text, boolean, number, integer, single-select, multi-select)
- **THEN** the TUI renders an appropriate field per question with its options
- **AND** invalid entries are reported before submission

### Requirement: Interrupt is always available
The user SHALL be able to interrupt a running turn at any time.

#### Scenario: Esc interrupts mid-turn
- **WHEN** the user presses Esc while the agent is running
- **THEN** the agent is interrupted
