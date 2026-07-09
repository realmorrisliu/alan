## MODIFIED Requirements

### Requirement: Safe terminal close and transcript restore are verified
The Apple client SHALL include focused automated tests and running-app smoke
evidence for terminal close guarding, bounded transcript snapshot persistence,
app-restart transcript restore, restored transcript panel presentation, and
restored transcript dismissal.

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
- **THEN** verification confirms the restored terminal shows the prior output in the accepted restored-context presentation
- **AND** the restored terminal accepts new input in a newly started shell at the restored cwd

#### Scenario: Restored transcript dismissal tested
- **WHEN** a restored terminal content has visible restored transcript context
- **THEN** tests verify supported clear actions remove the restored transcript from shell state, runtime restored-cache state, and subsequent persisted manifests
- **AND** view or model coverage verifies the restored panel text is terminal-aligned rather than centered as a narrow text block
