## MODIFIED Requirements

### Requirement: Single human-in-the-end auto-approve mode
The agent SHALL operate in a single posture named `Autonomous` with the
serialized value `autonomous`, in which routine operations proceed without
prompting and operations needing judgment are escalated. The system SHALL NOT
expose multiple selectable approval modes or accept retired profile aliases.
Escalations SHALL be routed to the reviewer (see the
`autonomous-review-mode` capability) rather than always pausing for a human,
except for red-line operations which bypass the reviewer (deny outright, or go
straight to the human). A human remains the final fallback when the reviewer
denies past the circuit breaker, when an operation is on the always-human red
line, or when the reviewer is unavailable.

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

#### Scenario: Canonical profile name resolves to Autonomous
- **WHEN** a configuration specifies the profile value `autonomous`
- **THEN** it resolves to the single `Autonomous` posture

#### Scenario: Retired profile alias is rejected
- **WHEN** a configuration or API request specifies `auto_approve`,
  `auto-approve`, `autoapprove`, or `conservative`
- **THEN** deserialization fails instead of resolving the alias to `Autonomous`

#### Scenario: Malformed profile values are rejected
- **WHEN** a configuration or API request specifies an unrecognized profile string (e.g. `"conservativ"`) or a wrong-typed value (a boolean, number, or object)
- **THEN** deserialization fails with an error rather than silently resolving to `Autonomous`, so a typo'd profile surfaces as a config error instead of a false sense of a stricter mode
