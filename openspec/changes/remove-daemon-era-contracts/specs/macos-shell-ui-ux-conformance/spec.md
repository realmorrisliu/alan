## REMOVED Requirements

### Requirement: Settings Surface Uses Task-Oriented Sections

**Reason**: The information architecture reserves an Agent section for daemon-backed profile, runtime, Skill, and local service data before a new macOS integration is designed.

**Migration**: Keep only local shell settings groups owned by the current macOS product.

### Requirement: Settings Surface Preserves Configuration Boundaries

**Reason**: The requirement binds Settings to retired connection, Skill, and host control-plane clients.

**Migration**: Limit Settings to configuration authorities already owned by the macOS shell and terminal stack.

### Requirement: Settings Editing Is Progressive And Safe

**Reason**: The progressive-editing model still treats Agent runtime, Session, Skill, and daemon routing controls as supported Settings families.

**Migration**: Retain safe progressive editing for local shell preferences only.

### Requirement: Settings Surface Keeps Shell-Native Density

**Reason**: Its unavailable-state contract treats daemon connection state as a Settings data source.

**Migration**: Preserve the visual-density rules for surviving local data sources.

### Requirement: Settings Uses Internal Task Navigation

**Reason**: The navigation contract exposes provider, credential, runtime, Skill, daemon endpoint, and Agent selector rows owned by the deleted client.

**Migration**: Navigate General, Terminal, and System settings only; future Agent integration requires a separate design.

### Requirement: Settings Uses Native Source List And Preference Detail

**Reason**: The source list requires an Agent group that no current macOS-owned data boundary can populate after cleanup.

**Migration**: Preserve native source-list geometry for the surviving local groups.

### Requirement: Settings Rows Use Precise Native Form Rhythm

**Reason**: The row-action contract includes copying a daemon endpoint as a first-class System action.

**Migration**: Keep native row rhythm and actions for locally owned paths, folders, updates, and diagnostics.

## ADDED Requirements

### Requirement: Settings uses local shell task sections

Alan for macOS Settings SHALL organize currently owned preferences into General, Terminal, and System groups. It SHALL NOT expose an Agent integration group until a later OpenSpec change defines its data and lifecycle boundary.

#### Scenario: Settings opens

- **WHEN** the user opens Settings in the shell content area
- **THEN** General, Terminal, and System are the available internal groups
- **AND** General is selected by default
- **AND** the surface contains no placeholder for an undecided Alan OS attachment

### Requirement: Settings preserves local configuration authorities

Alan for macOS Settings SHALL read and write each surviving setting through its existing macOS shell, terminal profile, managed terminal account, install-channel, update, shell-control, or diagnostics owner.

#### Scenario: Local preference is edited

- **WHEN** a user changes appearance, sidebar, inactive-pane dimming, terminal profile, or another supported local preference
- **THEN** Settings uses the same typed owner as the active shell feature
- **AND** it does not parse unrelated runtime, credential, or service files independently

### Requirement: Settings editing is progressive and locally bounded

Settings SHALL distinguish immediately editable local preferences from sensitive terminal-account actions, install facts, and diagnostics controls.

#### Scenario: Sensitive local action is selected

- **WHEN** a user provisions a managed terminal account or performs another privileged local action
- **THEN** Settings presents an explicit action with its local identity and safety state
- **AND** no raw secret is displayed after completion

### Requirement: Local Settings keeps shell-native density

Settings SHALL use compact native row groups, restrained typography, calm hierarchy, and concise unavailable states for surviving local sources.

#### Scenario: A local source is unavailable

- **WHEN** terminal profile, update, CLI install, shell-control, or diagnostics state cannot be read
- **THEN** Settings shows a compact unavailable status in the owning row
- **AND** it does not show raw diagnostics or add dashboard chrome

### Requirement: Settings uses native local task navigation

Settings SHALL render General, Terminal, and System as a compact native source list and SHALL show only the selected group's rows in the detail area.

#### Scenario: Group selection changes content

- **WHEN** the user selects a Settings group
- **THEN** the detail area updates without changing the outer shell sidebar, tab selection, split layout, or toolbar
- **AND** General owns app preferences, Terminal owns terminal profiles and managed terminal identity, and System owns install, update, shell-control, and diagnostics facts

### Requirement: Local Settings rows use precise native form rhythm

Settings rows SHALL align labels, secondary descriptions, values, toggles, and actions consistently while keeping actions limited to real operations owned by the local macOS product.

#### Scenario: System rows expose local actions

- **WHEN** System presents local paths, install facts, update state, or diagnostics
- **THEN** natural operations use compact native actions such as Copy, Show, or Export
- **AND** read-only facts remain honest values rather than disabled edit controls
- **AND** long metadata is available through native help or an explicit Copy or Show action
