## ADDED Requirements

### Requirement: Current repository surfaces exclude the retired daemon architecture
Alan SHALL reject live daemon-era contracts, code, commands, configuration, tests, fixtures, and
consumers from current repository surfaces. The guard SHALL cover canonical specs, active changes,
source code, build and release wiring, public help, environment variables, and current docs; it
SHALL exclude immutable OpenSpec archive history and SHALL distinguish unrelated Apple
`LaunchDaemon`, terminal-session, authentication-session, and third-party protocol terminology.

#### Scenario: Retired Alan daemon surface is reintroduced
- **WHEN** a current source, canonical spec, active change, command, configuration field, test, or
  current document reintroduces an Alan daemon module, daemon command, Session API route, relay,
  reconnect snapshot, or daemon-backed consumer
- **THEN** repository verification fails and identifies the live owner that must be removed or
  expressed through its canonical Process, file, namespace, or service boundary

#### Scenario: Historical archive records the former architecture
- **WHEN** an immutable file under `openspec/changes/archive/` contains daemon-era terminology
- **THEN** the current-surface guard ignores that historical record
- **AND** no current spec, code, help surface, or active change may cite it as current authority

#### Scenario: Unrelated platform terminology uses the same word
- **WHEN** current code uses Apple `LaunchDaemon` APIs or a terminal, authentication, or third-party
  protocol session that is not the retired Alan Agent Session
- **THEN** the semantic guard permits that owned use
- **AND** a broad word-only allowlist SHALL NOT hide an Alan daemon/session compatibility surface
