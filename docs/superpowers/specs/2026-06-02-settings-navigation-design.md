# Settings Navigation Information Architecture

## Context

Alan's macOS Settings tab currently renders every settings section in one long
scrolling page. The underlying model already separates content into Interface,
Terminal Profiles, Terminal Accounts, Accounts, Sessions, Capabilities, and
Local sections, but the visual hierarchy does not help users choose the setting
area they need.

The target experience is a calm, native, Arc-like settings surface that makes
settings easy to scan without making the tab feel like a dashboard or a debug
panel.

## Goals

- Add a left settings navigation inside the Settings tab.
- Group settings by user task rather than internal implementation boundary.
- Show only the selected settings group in the main content area.
- Preserve the existing row rendering, direct controls, redaction rules, and
  compact unavailable states.
- Keep the first implementation focused on light mode and current Settings
  content.

## Non-Goals

- Do not add new settings.
- Do not make provider account rows directly editable.
- Do not expose raw credentials, custom command bodies, runtime IDs, or debug
  payloads in normal settings rows.
- Do not persist the selected Settings group in the first pass.
- Do not redesign the outer shell sidebar or tab model.

## Recommended Approach

Use a two-column Settings layout:

- Left column: a compact Settings navigation list.
- Right column: the selected group content.

The left navigation contains six user-task groups:

1. General
2. Terminal
3. Accounts
4. Sessions
5. Capabilities
6. Advanced

The default selected group is General. Reopening Settings starts on General.
Clicking a navigation row changes the selected group without changing the outer
Alan shell tab or workspace state.

## Group Mapping

### General

General contains interface preferences:

- Appearance
- Sidebar
- Inactive split dimming

### Terminal

Terminal contains local terminal startup and terminal-entry configuration:

- Default Terminal Profile
- New Terminal Profile
- Existing terminal profiles
- Sudo behavior
- Managed terminal account
- Mac login session

Terminal combines the current Terminal Profiles and Terminal Accounts sections
because both are about local terminal entry. This keeps them separate from LLM
provider accounts.

### Accounts

Accounts contains provider connection state:

- Connection profile
- Selected profile
- Provider
- Model
- Credential
- Account actions

When the daemon is unavailable, Accounts keeps the current compact unavailable
row instead of showing a page-level failure.

### Sessions

Sessions contains new-session runtime defaults:

- Governance
- Reasoning effort
- Streaming
- Stream recovery

### Capabilities

Capabilities contains skill catalog state:

- Enabled skills
- Implicit invocation
- Unavailable skills

When the daemon or workspace skill catalog is unavailable, Capabilities keeps
the current compact unavailable row.

### Advanced

Advanced contains local installation, paths, daemon endpoint, and diagnostics:

- App
- Install channel
- Command line tool
- Daemon endpoint
- Updates
- Alan home
- Public skills
- Shell state
- Shell control
- Performance Diagnostics
- Export Recent Diagnostics

This moves implementation-facing local state out of the default reading path
while keeping it available for maintenance and support.

## Component Design

`ShellSettingsSurfaceSnapshot` remains the source of truth for settings rows.
The new grouping layer maps existing sections into navigation groups without
changing row-level redaction or mutability.

`ShellSettingsContentView` owns:

- The two-column layout.
- `@State selectedGroup`, defaulting to `.general`.
- Existing settings summary refresh logic.
- Existing bindings for appearance, sidebar visibility, split dimming, and
  performance diagnostics.

`ShellSettingsNavigationView` renders:

- The six group rows.
- Lightweight selected state.
- SF Symbol icons for quick scanning.

The navigation should feel quieter than the main shell sidebar. It should use
compact rows, restrained typography, and native-feeling selection treatment.

`ShellSettingsGroupView` renders:

- The selected group title.
- The rows for that group.
- Existing `ShellSettingsSectionView` or equivalent row-container reuse.

`ShellSettingsSectionView`, `ShellSettingsRow`, `ShellSettingsValueLabel`, and
`ShellSettingsDivider` should be reused where possible. The current row card
style can remain, but the right pane should no longer render all top-level
sections in a single scroll.

## Data Flow

1. `ShellSettingsContentView` builds `ShellSettingsSurfaceSnapshot.make(...)`
   from local and remote summaries.
2. A grouping helper maps snapshot sections into `ShellSettingsNavigationGroup`
   values.
3. The navigation view receives the groups and a binding to the selected group.
4. The content view renders rows for only the selected group.
5. Direct controls continue to update their existing `@AppStorage` or runtime
   bindings.

Remote refresh failures only affect rows backed by remote data. They do not
replace the entire Settings surface.

## Error Handling

- Daemon unavailable: Accounts and Capabilities render their existing compact
  unavailable rows.
- Workspace skill catalog unavailable: Capabilities renders the existing compact
  unavailable reason.
- Diagnostics export disabled: the export action remains disabled when
  performance diagnostics are disabled.
- Missing or unexpected section data: the grouping helper should omit empty
  groups only if the source snapshot provides no rows for that group. The six
  expected groups should exist in normal snapshots.

## Testing

Add focused model tests for:

- Navigation group default order is General, Terminal, Accounts, Sessions,
  Capabilities, Advanced.
- General maps only the current interface rows.
- Terminal includes Terminal Profiles and Terminal Accounts rows.
- Accounts does not include terminal profile or terminal account rows.
- Advanced contains local installation, path, daemon endpoint, and diagnostics
  rows.
- Existing secret redaction tests still pass for visible grouped text.
- Existing compact unavailable behavior remains for Accounts and Capabilities.

For UI verification after implementation:

- Build and relaunch the Alan Dev app fresh.
- Inspect Settings in light mode.
- Verify General is selected by default.
- Verify selecting each left navigation item updates only the right pane.
- Verify Terminal and Advanced handle longer row lists without overlap.
- Verify row controls and disabled diagnostics export state still render
  correctly.

## Scope Boundary

This design is scoped to Settings information architecture and view structure.
It does not change settings semantics, daemon APIs, terminal profile storage,
connection profile behavior, skill catalog behavior, or local diagnostics
capture.
