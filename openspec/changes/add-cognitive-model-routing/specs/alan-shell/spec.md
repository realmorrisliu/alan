## ADDED Requirements

### Requirement: Shadow evaluator selection is explicit and invocation-scoped
Bare Alan SHALL accept `--shadow-evaluator <profile>` to capture a separate finite-choice evaluator for its Root Agent input. The option SHALL default to absent and be rejected for subcommands. Host and Connection Service SHALL verify callable capability and capture immutable profile/model provenance without changing generation selection or effect governance.

#### Scenario: An evaluator is explicitly selected
- **WHEN** bare Alan starts with an available finite-choice evaluation profile
- **THEN** the Root Machine receives that separately captured evaluator and the actual interactive or redirected admission surface
- **AND** its acknowledged advice is exposed through the read-only Machine projection without selecting an execution path
- **AND** ordinary user work retains its existing generation and effect contracts

#### Scenario: Profile metadata does not prove capability
- **WHEN** the requested profile is missing, unavailable, or its published callable lacks finite-choice evaluation
- **THEN** invocation startup reports the selection failure before accepting input
- **AND** no fallback evaluator or generation-default replacement occurs

#### Scenario: A Root starts a child Agent Process
- **WHEN** a Root with an explicitly selected shadow evaluator spawns a child
- **THEN** the child SHALL NOT inherit the Root input shadow selection
