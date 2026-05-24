## ADDED Requirements

### Requirement: Global public skill sources are channel-scoped
Alan SHALL resolve and mutate global public skill install sources according to
the active install channel. The stable channel SHALL keep `~/.agents/skills/`;
the dev channel SHALL use a separate global public skill source.

#### Scenario: Stable global public skills are discovered
- **WHEN** stable-channel Alan discovers global public skill packages
- **THEN** it discovers packages under `~/.agents/skills/`
- **AND** existing stable public skill compatibility remains unchanged

#### Scenario: Dev global public skills are discovered
- **WHEN** dev-channel Alan discovers global public skill packages
- **THEN** it discovers packages under `~/.agents-dev/skills/`
- **AND** it does not discover `~/.agents/skills/` as an implicit fallback

#### Scenario: Dev installs a global skill
- **WHEN** a dev-channel command installs or updates a global public skill package
- **THEN** it writes under `~/.agents-dev/skills/`
- **AND** it does not create, modify, or remove packages under `~/.agents/skills/`

### Requirement: Workspace skill sources remain workspace-authored
Install-channel isolation SHALL NOT change the portable workspace public skill
source path.

#### Scenario: Workspace public skills are discovered
- **WHEN** either channel discovers portable public skill packages in a workspace
- **THEN** `<workspace>/.agents/skills/` remains the workspace public skill source
- **AND** packages discovered there are treated as workspace-authored content rather than channel-private global data

#### Scenario: Workspace skill writes generated output
- **WHEN** a workspace skill run writes generated runtime output, evaluation cache, or logs through Alan-managed paths
- **THEN** those generated outputs are channel-scoped
- **AND** the source skill package under `<workspace>/.agents/skills/` remains unchanged unless the user explicitly edits or installs into that workspace source
