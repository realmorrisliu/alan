# auto-approve-policy Specification

## Purpose
Defines Alan's single Autonomous approval posture, deterministic escalation
boundaries, non-persistent permissions, keyboard-driven approval and structured
input surfaces, decision context, and interrupt behavior.
## Requirements
### Requirement: Single human-in-the-end auto-approve mode
The agent SHALL operate in a single posture named `Autonomous` in which routine operations proceed without prompting and operations needing judgment are escalated. The system SHALL NOT expose multiple selectable approval modes. Escalations SHALL be routed to the reviewer (see the `autonomous-review-mode` capability) rather than always pausing for a human, except for red-line operations which bypass the reviewer (deny outright, or go straight to the human). A human remains the final fallback when the reviewer denies past the circuit breaker, when an operation is on the always-human red line, or when the reviewer is unavailable.

#### Scenario: Routine operation proceeds automatically
- **WHEN** the policy classifies an operation as a read or an in-workspace write
- **THEN** the operation proceeds without prompting the user

#### Scenario: No mode switcher
- **WHEN** a user interacts with the agent
- **THEN** there is exactly one approval posture (`Autonomous`) and no selectable mode

#### Scenario: Escalations route to the reviewer
- **WHEN** an operation needs judgment and is not on a red line
- **THEN** it is routed to the reviewer rather than immediately pausing for a human

#### Scenario: Red-line operations do not reach the reviewer
- **WHEN** an operation is catastrophic (deny) or dangerous-but-uncontainable (always-human)
- **THEN** it is denied outright or surfaced to the human, and the reviewer never decides it

#### Scenario: Legacy profile names resolve to Autonomous
- **WHEN** a configuration specifies a legacy profile value (such as `auto_approve`, `conservative`, or `autonomous`)
- **THEN** it resolves to the single `Autonomous` posture without error

#### Scenario: Malformed profile values are rejected
- **WHEN** a configuration or API request specifies an unrecognized profile string (e.g. `"conservativ"`) or a wrong-typed value (a boolean, number, or object)
- **THEN** deserialization fails with an error rather than silently resolving to `Autonomous`, so a typo'd profile surfaces as a config error instead of a false sense of a stricter mode

### Requirement: Conservative escalation boundary before OS sandboxing
When an OS sandbox backend is active, the policy SHALL allow sandboxed bash and policy-permitted network to proceed without prompting, while still escalating effects that escape the sandbox (writes outside the workspace and disallowed network) and operations of unknown capability. When no OS sandbox backend is active, the policy SHALL escalate network access, writes outside the workspace, explicitly destructive or irreversible operations, and operations of unknown capability.

#### Scenario: Sandboxed bash proceeds when a backend is active
- **WHEN** an OS sandbox backend is active and a shell command runs confined to the workspace
- **THEN** the command proceeds without prompting

#### Scenario: Sandbox-escaping effect still escalates
- **WHEN** an operation would write outside the workspace or perform disallowed network access
- **THEN** the policy escalates it for human approval even when a backend is active

#### Scenario: Network access escalates
- **WHEN** an operation is classified as network capability and no OS sandbox backend is active
- **THEN** the policy escalates it for human approval

#### Scenario: Out-of-workspace write escalates
- **WHEN** an operation would write outside the workspace
- **THEN** the policy escalates it for human approval

#### Scenario: Destructive operation escalates
- **WHEN** an operation is explicitly destructive or irreversible and no OS sandbox backend is active
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
