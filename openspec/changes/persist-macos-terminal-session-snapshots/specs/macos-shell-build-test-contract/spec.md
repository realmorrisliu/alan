## ADDED Requirements

### Requirement: Safe terminal close and transcript restore are verified
The Apple client SHALL include focused automated tests and running-app smoke
evidence for terminal close guarding, bounded transcript snapshot persistence,
and app-restart transcript restore.

#### Scenario: Active close guard tested
- **WHEN** tests request pane, tab, window, app, or Quick Terminal close for terminal content with active work
- **THEN** tests verify that close requires confirmation and does not mutate shell state or finalize runtimes before confirmation

#### Scenario: Idle close bypass tested
- **WHEN** tests request close for idle shell, exited terminal, or non-terminal content
- **THEN** tests verify that Alan does not require active-work confirmation solely because a shell process exists

#### Scenario: Manifest transcript round trip tested
- **WHEN** tests persist a workspace manifest containing terminal transcript snapshots
- **THEN** tests verify old manifests without snapshots still decode
- **AND** new manifests preserve bounded transcript lines, dimensions, cwd, title, focus, truncation metadata, and content identity through a round trip

#### Scenario: Restart transcript restore smoke tested
- **WHEN** a running-app smoke produces visible terminal output, closes or quits Alan through a confirmed path, and relaunches the freshly installed app
- **THEN** verification confirms the restored terminal shows the prior output without an extra restored-session banner
- **AND** the restored terminal accepts new input in a newly started shell at the restored cwd
