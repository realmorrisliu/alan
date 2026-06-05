## ADDED Requirements

### Requirement: Settings Surface Uses Task-Oriented Sections
Alan macOS Settings SHALL organize configuration and local status into
task-oriented sections rather than exposing storage files, raw implementation
IDs, or one-off controls as the primary information architecture.

The default section order SHALL be:

- Interface
- Accounts
- Sessions
- Capabilities
- Local

#### Scenario: Settings tab renders grouped sections
- **WHEN** the user opens the Settings content tab
- **THEN** alan shows Interface, Accounts, Sessions, Capabilities, and Local as
  distinct settings sections in the shell content area
- **AND** Interface appears before advanced or diagnostic sections
- **AND** the content remains inside the existing shell tab chrome

#### Scenario: Existing interface preferences remain available
- **WHEN** the Settings tab is active
- **THEN** alan exposes appearance mode, sidebar visibility, and inactive split
  pane dimming as editable Interface preferences
- **AND** changing those preferences updates the same app-level preference state
  used by the main shell surface

#### Scenario: Storage details are secondary
- **WHEN** Settings presents profile, session, skill, daemon, CLI, update, or
  local data information
- **THEN** alan uses user-facing labels for the primary row text
- **AND** raw file names such as `agent.toml`, `connections.toml`, `host.toml`,
  and raw content identifiers appear only as secondary diagnostic detail when
  needed

### Requirement: Settings Surface Preserves Configuration Boundaries
Alan macOS Settings SHALL present configuration through the existing authority
for each configuration family and MUST NOT become an independent parser/editor
for runtime or credential files.

#### Scenario: Account summary uses connection control surfaces
- **WHEN** Settings presents connection profile, provider, model, credential, or
  connection-test state
- **THEN** alan uses the connection control-plane surface or typed client model
  derived from that surface
- **AND** Settings does not read secret material or display secret values

#### Scenario: Capability summary uses skill catalog state
- **WHEN** Settings presents skill or capability state
- **THEN** alan uses the resolved skill catalog and override state exposed by
  the skill management surface
- **AND** Settings presents `enabled` and `allow_implicit_invocation` as
  user-facing capability state rather than legacy mount-mode labels

#### Scenario: Local summary uses install-channel helpers
- **WHEN** Settings presents local app identity, channel, daemon, CLI, update,
  shell-control, or alan-home status
- **THEN** alan derives those values from install-channel, host-config, CLI
  installer, update-policy, or shell-control helpers
- **AND** dev-channel Settings uses dev-channel labels and locations rather than
  silently falling back to stable-channel state

### Requirement: Settings Editing Is Progressive And Safe
Alan macOS Settings SHALL distinguish immediately editable preferences from
summaries and advanced controls whose writes affect credentials, daemon routing,
agent runtime behavior, skills, or local install state.

#### Scenario: First-phase editable controls are limited
- **WHEN** the first grouped Settings implementation ships
- **THEN** local Interface preferences are directly editable
- **AND** Accounts, Capabilities, and Local rows default to read-only summaries
  or focused actions unless their existing control-plane write path is wired
- **AND** Settings avoids freeform editing of `agent.toml`, `connections.toml`,
  `host.toml`, `models.toml`, or credential stores

#### Scenario: Advanced runtime controls use disclosure
- **WHEN** Settings exposes runtime controls such as reasoning effort,
  streaming, recovery, tool limits, timeouts, compaction, prompt snapshots, or
  skill overrides
- **THEN** alan places advanced controls behind progressive disclosure or a
  compact advanced section
- **AND** the default view remains focused on everyday app, account, session,
  capability, and local status tasks

#### Scenario: Sensitive account actions are explicit
- **WHEN** the user performs account actions such as login, logout, set key, or
  test connection from Settings
- **THEN** alan presents those actions as explicit commands with provider and
  profile context
- **AND** Settings does not expose raw token or API-key contents after the
  action completes

### Requirement: Settings Surface Keeps Shell-Native Density
Alan macOS Settings SHALL use compact native-feeling row groups, restrained
typography, and calm hierarchy that fit the terminal-first shell instead of a
page-like dashboard or marketing/settings portal.

#### Scenario: Settings rows stay scannable
- **WHEN** Settings renders multiple sections
- **THEN** each row has one primary label and at most one focused trailing
  control, value, or action
- **AND** secondary text is concise and does not repeat the section heading

#### Scenario: Settings avoids dashboard chrome
- **WHEN** Settings is active
- **THEN** alan does not add a hero header, metric cards, nested cards,
  decorative gradients, or a separate settings navigation shell
- **AND** the Settings surface remains visually subordinate to the surrounding
  shell workspace

#### Scenario: Unavailable status is calm and actionable
- **WHEN** a Settings data source such as daemon connection state, skill catalog,
  update policy, or CLI install status is unavailable
- **THEN** alan shows a compact unavailable status in the relevant row or
  section
- **AND** alan avoids raw stack traces or debug payloads in the default Settings
  view
