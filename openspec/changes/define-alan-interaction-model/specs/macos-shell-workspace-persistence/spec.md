## MODIFIED Requirements

### Requirement: Workspace manifest is the restore authority
The macOS shell SHALL use the current versioned content-container workspace
manifest as the sole authoritative source for restoring Spaces, Tabs, pin
snapshots, Tab lifecycle metadata, and the last selected Space/Tab across app
restarts. It SHALL NOT restore from a persistent shell-state snapshot or migrate
an earlier manifest shape.

#### Scenario: Manifest is present
- **WHEN** Alan for macOS starts and a valid current workspace manifest exists
  for `window_main`
- **THEN** Alan loads Spaces, Tabs, pin snapshots, lifecycle metadata, and the
  last selected Space/Tab from that manifest
- **AND** Alan materializes the current shell state from the manifest rather
  than another persisted snapshot

#### Scenario: Manifest is missing
- **WHEN** Alan for macOS starts and no workspace manifest exists for
  `window_main`
- **THEN** Alan creates a default current manifest with one default Space
  whose selected Tab is the workspace home surface defined by
  `alan-interaction-model`
- **AND** no default terminal Tab is required; Alan Shell tabs are created
  explicitly as one tab type among others
- **AND** Alan uses that manifest as the restore authority for the launched
  shell state

#### Scenario: Unsupported manifest schema exists
- **WHEN** the manifest path contains a terminal-only, `quick_terminal`, or
  otherwise unsupported schema
- **THEN** Alan preserves it as corrupt or unsupported evidence and creates a
  default current manifest
- **AND** Alan does not invoke a legacy decoder, upgrade, or fallback

#### Scenario: Obsolete shell-state file exists
- **WHEN** an Application Support `shell-state-*.json` file remains on disk
- **THEN** Alan does not discover or read it during startup
- **AND** the file cannot become workspace restore authority
