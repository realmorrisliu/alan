# Spec Delta

## ADDED Requirements

### Requirement: Smart approval operates strictly inside the active OS boundary
`TrustPosture::ConfinedSmart` SHALL keep active OS sandbox confinement and
explicit Host Mount grants as the security boundary. Effects already contained
by active OS confinement SHALL run without invoking `decide()`. Decision
verdicts and scores SHALL remain evidence only and MUST NOT disable OS
confinement or widen grants. Without an active OS confinement backend,
`ConfinedSmart` SHALL keep the conservative pre-sandbox boundary.

#### Scenario: Contained action skips System 1 evaluation
- **WHEN** an Agent runs under `ConfinedSmart` inside an active OS confinement
  backend and proposes a command whose effects remain inside the granted
  confinement
- **THEN** the command executes without calling `connections/<conn>/decide`

#### Scenario: System 1 verdict cannot widen a Host Mount grant
- **WHEN** a proposed action writes outside explicit writable Host Mounts or
  requests ungranted private resources
- **THEN** a favorable System 1 verdict cannot authorize the effect without an
  explicit trusted-user grant

#### Scenario: No OS confinement backend is active under ConfinedSmart
- **WHEN** no OS sandbox backend is active on the current platform
- **THEN** Alan discloses the absence of confinement and keeps the conservative
  pre-sandbox boundary in force
