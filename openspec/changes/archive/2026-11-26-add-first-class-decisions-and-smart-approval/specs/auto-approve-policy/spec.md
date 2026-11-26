# Spec Delta

## MODIFIED Requirements

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

### Requirement: Trust controls are invocation-local and authority-preserving
The runtime SHALL accept posture/isolation changes only through trusted user
controls and SHALL order postures strictly as `Governed < ConfinedSmart <
ConfinedTrusted < HostAccountTrusted` when enforcing equal-or-narrower child
Process inheritance. Interactive confined posture toggling SHALL cycle only
among confined postures and MUST NOT promote to `HostAccountTrusted`.
Revocation, cancellation, and budget controls SHALL remain active, and a
downgrade SHALL invalidate incompatible pending decisions without claiming
retroactive confinement or rollback.

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

## ADDED Requirements

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
