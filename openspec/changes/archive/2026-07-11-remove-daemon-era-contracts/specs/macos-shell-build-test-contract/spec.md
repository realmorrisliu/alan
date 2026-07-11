## MODIFIED Requirements

### Requirement: Channel isolation has focused verification

Changes to install-channel resolution SHALL include focused validation that stable and dev channels resolve distinct Alan homes, configuration, credential, agent-definition, model, singleton, shell-control, and app-install boundaries.

#### Scenario: Channel paths are checked

- **WHEN** channel-aware path resolution changes
- **THEN** focused tests verify stable channel state resolves under `~/.alan`
- **AND** focused tests verify dev channel state resolves under `~/.alan-dev`
- **AND** tests cover connections, credentials, agents, models, managed auth, registry, global public Skill sources, and other state owned by surviving components

#### Scenario: Shell-control namespaces are checked

- **WHEN** channel-aware shell-control paths change
- **THEN** focused tests verify stable and dev shell-control socket paths differ
- **AND** commands for one channel do not read binding files from the other channel

#### Scenario: Side-by-side smoke is checked

- **WHEN** dev channel support is considered ready for local use
- **THEN** maintainers can run an automated or documented manual smoke with stable Alan and Alan Dev installed together
- **AND** the smoke verifies distinct bundle identifiers, commands, configuration, credentials, and Alan-home state

## ADDED Requirements

### Requirement: Apple validation rejects unowned product sources

The Apple validation matrix SHALL fail when a source group without a current architecture owner or
focused verification boundary gains product target membership.

#### Scenario: Unowned source is added

- **WHEN** an Xcode project or build script adds a source outside the documented active owners
- **THEN** a focused source-membership or architecture check fails
- **AND** the failure requires an accepted owner contract instead of accepting an unavailable stub
