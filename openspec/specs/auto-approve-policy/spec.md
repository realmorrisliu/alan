# auto-approve-policy Specification

## Purpose
Defines Alan's single canonically serialized Autonomous approval posture,
deterministic escalation boundaries, strict profile parsing, non-persistent
permissions, keyboard-driven approval and structured input surfaces, decision
context, and interrupt behavior.
## Requirements
### Requirement: Single human-in-the-end auto-approve mode
Alan SHALL default to the governed autonomous posture serialized as `autonomous`
with explicit project delegation, and SHALL offer invocation-only elevated
postures strictly through trusted-user invocation controls: `ConfinedSmart`
(System 1 smart approval inside OS confinement), `ConfinedTrusted` (per-action
policy and Guardian bypass inside OS confinement), and `HostAccountTrusted`
(unsandboxed Host-account native execution requiring separate explicit opt-in).
Approval posture SHALL remain separate from resource scope and execution
isolation. In governed autonomous execution and `ConfinedSmart`, routine valid
delegated operations SHALL proceed without repeated permission prompts,
unresolved judgment SHALL use qualified reviewer or System 1 decision
evaluation or a correlated human request, and red-line operations SHALL bypass
automated reviewers (deny outright or go straight to the human). Fully trusted
postures (`ConfinedTrusted` and `HostAccountTrusted`) SHALL bypass per-action
policy red lines and Guardian, not protocol validation, Process/OS rights,
active isolation, lifecycle, or budgets. Isolation SHALL remain separately
selected. Project files, saved task history, and model output MUST NOT activate
`ConfinedSmart`, `ConfinedTrusted`, or `HostAccountTrusted`, and none of these
elevated postures SHALL persist into a new invocation. Retired profile aliases
and malformed profile values SHALL be rejected.

#### Scenario: Default startup
- **WHEN** the user starts Alan without a new explicit trust selection
- **THEN** governed autonomous execution and confined selection apply

#### Scenario: Routine delegated work
- **WHEN** a read, edit, build, test or local commit is within current delegated scope and backend guarantees
- **THEN** it proceeds without repeated approval and without acquiring additional rights

#### Scenario: Escalations route to the reviewer
- **WHEN** an operation needs judgment in governed autonomous execution and is not on a red line
- **THEN** it is routed to the reviewer rather than immediately pausing for a human

#### Scenario: Reviewer cannot expand scope
- **WHEN** a reviewer or System 1 decision approves an operation outside delegated authority
- **THEN** the action does not execute without a separate trusted-user grant

#### Scenario: Red-line operations do not reach the reviewer
- **WHEN** an operation in governed autonomous or `ConfinedSmart` execution is catastrophic (deny) or dangerous-but-uncontainable (always-human)
- **THEN** it is denied outright or surfaced to the human, and neither the reviewer nor System 1 smart approval decides it

#### Scenario: ConfinedSmart execution inside OS sandbox
- **WHEN** the user explicitly selects `ConfinedSmart` for an invocation
- **THEN** active OS sandbox confinement and explicit Host Mount bounds remain enforced
- **AND** reviewer-judged boundary-crossing escalations are triaged via the Decisions API with calibration-aware thresholds and fail-closed human fallback

#### Scenario: Trusted confined execution
- **WHEN** the user disables per-action approvals but does not select Host-account execution
- **THEN** the active granted scope and sandbox remain, and blocked effects fail without prompting or broadening

#### Scenario: Trusted Host-account execution
- **WHEN** the user separately selects Host-account native execution
- **THEN** native commands use actual account authority and UI discloses absence of Alan sandbox confinement

#### Scenario: Untrusted activation attempt
- **WHEN** a project file, instruction file, rollout history, or Agent requests `ConfinedSmart`, `ConfinedTrusted`, or `HostAccountTrusted`
- **THEN** the trusted control path rejects it

#### Scenario: Canonical profile name resolves to Autonomous
- **WHEN** a configuration specifies the profile value `autonomous`
- **THEN** it resolves to the default governed `Autonomous` posture

#### Scenario: Retired profile alias is rejected
- **WHEN** a configuration or API request specifies `auto_approve`,
  `auto-approve`, `autoapprove`, or `conservative`
- **THEN** deserialization fails instead of resolving the alias to `Autonomous`

#### Scenario: Malformed profile values are rejected
- **WHEN** a configuration or API request specifies an unrecognized profile string (e.g. `"conservativ"`) or a wrong-typed value (a boolean, number, or object)
- **THEN** deserialization fails with an error rather than silently resolving to `Autonomous`, so a typo'd profile surfaces as a config error instead of a false sense of a stricter mode

### Requirement: Conservative escalation boundary before OS sandboxing
These escalation rules SHALL apply only to governed autonomous execution. Fully
trusted execution instead follows its explicitly selected isolation without
per-action escalation. In governed autonomous execution, deterministic checks
SHALL admit only effects supported by current delegated scope and actual backend
enforcement. When an OS sandbox backend is active, the policy SHALL allow
sandboxed bash and policy-permitted network to proceed without prompting, while
still escalating effects that escape the sandbox (writes beyond explicit
writable Host Mounts and disallowed network) and operations of unknown
capability. When no OS sandbox backend is active, the policy SHALL escalate
network access, writes beyond the Process's explicit writable namespace
authority, explicitly destructive or irreversible operations, and operations of
unknown capability. An exact user-origin publication delegation MAY satisfy the
prior per-call publication approval, but ordinary push SHALL NOT authorize
force-push or remote deletion. Privilege expansion, control-policy changes, new
private resources, and uncontainable effects SHALL require a trusted-user
decision or denial; reviewer output MUST NOT grant them.

#### Scenario: Sandboxed bash proceeds when a backend is active
- **WHEN** an OS sandbox backend is active and a shell command is confined to
  the Process's writable Host Mount projection
- **THEN** the command proceeds without prompting

#### Scenario: Sandbox-escaping effect still escalates
- **WHEN** an operation would write beyond the Process's explicit writable
  namespace authority or perform disallowed network access
- **THEN** the policy escalates it for human approval even when a backend is active

#### Scenario: Network access escalates
- **WHEN** an operation is classified as network capability and no OS sandbox backend is active
- **THEN** the policy escalates it for human approval

#### Scenario: Out-of-authority write escalates
- **WHEN** an operation would write beyond the Process's explicit writable
  namespace authority
- **THEN** the policy escalates it for human approval

#### Scenario: Destructive operation escalates
- **WHEN** an operation is explicitly destructive or irreversible and no OS sandbox backend is active
- **THEN** the policy escalates it for human approval

#### Scenario: Unknown capability escalates by default
- **WHEN** an operation's capability cannot be classified
- **THEN** the policy escalates it rather than auto-allowing it

#### Scenario: Delegated branch push
- **WHEN** an ordinary push matches a current user-approved repository, remote and ref scope
- **THEN** it needs no repeated approval, while credentials and backend permissions remain independently checked

#### Scenario: Force push is not ordinary push
- **WHEN** a force-push or remote deletion lacks its own exact delegated capability
- **THEN** the runtime asks the user or denies it before effects

#### Scenario: Scope cannot be enforced
- **WHEN** the active backend cannot enforce a promised delegated resource or network boundary
- **THEN** the runtime reports that limitation without automatically choosing Host-account execution

### Requirement: Permissions are never silently widened
A one-operation approval SHALL apply only to that correlated operation and MUST
NOT silently widen future authority or write persistent permission grants. A
separate explicit trusted-user action MAY save scoped project delegation for
reuse after identity and revocation checks. Project content, model output, and
ordinary task memory SHALL NOT create or modify standing delegation.

#### Scenario: One action is approved
- **WHEN** the user approves one escalated action
- **THEN** a later operation that would otherwise escalate still escalates
- **AND** no standing delegation or persistent permission grant is created

#### Scenario: Delegation is explicitly saved
- **WHEN** the user separately saves a bounded project delegation
- **THEN** later matching actions reuse it only while identity, scope and revocation state remain valid

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

### Requirement: Trust controls are invocation-local and authority-preserving
The runtime SHALL accept posture/isolation changes only via trusted user controls,
ordering postures strictly as `Governed < ConfinedSmart < ConfinedTrusted <
HostAccountTrusted` for equal-or-narrower child inheritance. Interactive confined
toggling SHALL cycle only among confined postures and MUST NOT promote to
`HostAccountTrusted`. Revocation, cancellation, and budgets SHALL stay active;
downgrades SHALL invalidate stale decisions without claiming retroactive confinement.

#### Scenario: New invocation after trusted work
- **WHEN** the user resumes a prior `ConfinedSmart` or trusted task in another invocation without newly selecting an elevated posture
- **THEN** recovery uses the governed default and revalidates authority

#### Scenario: Child spawn enforces equal-or-narrower posture
- **WHEN** a parent Process running under `ConfinedSmart` spawns a child Process
- **THEN** the child may run under `ConfinedSmart` or `Governed`
- **AND** requesting `ConfinedTrusted` or `HostAccountTrusted` is rejected

#### Scenario: Downgrade during execution
- **WHEN** the user reduces trust while a native process or System 1 decision is already running
- **THEN** stale pending assessments cannot authorize execution, new incompatible effects pause, and existing lifecycle controls remain available; UI does not claim that process was retroactively sandboxed

### Requirement: ConfinedSmart applies calibration-aware System 1 approval and fails closed
Under `TrustPosture::ConfinedSmart`, reviewer-judged escalations SHALL be
evaluated via `connections/<conn>/decide` with distinct policy gates for
`CalibrationKind::Calibrated` and `CalibrationKind::Uncalibrated` responses.
Uncalibrated confidence SHALL NOT be treated as calibrated probability. Any
evaluator error, timeout, malformed response, stale authority revision, or
below-threshold confidence SHALL fail closed to one pending human request.

#### Scenario: Calibrated System 1 decision satisfies allow gate
- **WHEN** a reviewer-judged action under `ConfinedSmart` receives a
  `CalibrationKind::Calibrated` decision with verdict `allow`, predicate `safe
  == true`, confidence at or above the calibrated threshold, and risk score at
  or below the calibrated ceiling
- **THEN** the action executes inside the active OS confinement without
  interrupting the user

#### Scenario: Uncalibrated evaluator meets stricter gate or defers to human
- **WHEN** a reviewer-judged action under `ConfinedSmart` is evaluated by an
  `Uncalibrated` provider
- **THEN** runtime applies the stricter uncalibrated policy thresholds rather
  than the calibrated thresholds
- **AND** escalates to a pending human approval request whenever those stricter
  thresholds are not met

#### Scenario: System 1 evaluator fails or times out
- **WHEN** the decision Connection is unavailable, exceeds its deadline,
  returns malformed evidence, or is invalidated by a grant or posture change
- **THEN** Alan fails closed by presenting one correlated human approval
  request
- **AND** never treats the failure as implicit approval
