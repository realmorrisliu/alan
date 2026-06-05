# macos-shell-keybinding-system Specification

## Purpose
Define the fixed first-version macOS shell keybinding contract, including
registry-backed default shortcuts, menu hints, target semantics, conflict
detection, and input precedence.
## Requirements
### Requirement: Default Keybindings Are Registry Backed
The macOS shell SHALL declare default keybindings on shell action registry
descriptors, and keyboard dispatch SHALL invoke actions through the same
registry used by menus and context menus.

#### Scenario: Shortcut invokes registered action
- **WHEN** the user presses a registered shell shortcut
- **THEN** alan resolves the matching action descriptor and invokes its handler
  through the shell action registry

#### Scenario: Menu displays shortcut hint
- **WHEN** a shell menu item is built from a registry action descriptor that has
  a default shortcut
- **THEN** the menu item displays the native shortcut hint from that descriptor

#### Scenario: Shortcut descriptor is missing
- **WHEN** an action has no default shortcut descriptor
- **THEN** alan may expose the action in menus or context menus without a
  keyboard shortcut

### Requirement: First-Version Keybindings Are Fixed Defaults
The first version of the macOS shell keybinding system SHALL provide fixed
default shortcuts and SHALL NOT expose user-custom shortcut preferences, config
files, or import/export behavior.

#### Scenario: User opens settings
- **WHEN** the first-version keybinding system ships
- **THEN** alan does not expose a shortcut customization preference screen

#### Scenario: Workspace manifest is saved
- **WHEN** workspace state is persisted
- **THEN** alan does not write user-custom shortcut bindings to the workspace
  manifest

#### Scenario: Connection or agent config loads
- **WHEN** shell startup loads connection, host, or agent configuration
- **THEN** alan does not derive user-custom shortcut bindings from those config
  files

### Requirement: Default Coverage Favors High-Frequency Shell Actions
The macOS shell SHALL preserve existing shortcuts and add defaults only for
high-frequency Tab, pane, Find, and Space navigation actions in the first
version.

#### Scenario: Existing shortcut exists
- **WHEN** an existing shell shortcut is still valid
- **THEN** alan preserves its key equivalent and routes it through the registry

#### Scenario: Tab navigation shortcut is pressed
- **WHEN** the user presses the default shortcut for next or previous Tab
- **THEN** alan switches Tab inside the current Space

#### Scenario: Tab structural shortcut is pressed
- **WHEN** the user presses the default shortcut for moving or pinning the
  current Tab
- **THEN** alan invokes the registered action against the current selected Tab

#### Scenario: Space navigation shortcut is pressed
- **WHEN** the user presses the default shortcut for next Space, previous Space,
  or numeric Space selection
- **THEN** alan switches the current Space without creating, renaming, or
  deleting Spaces

#### Scenario: Space management action has no default shortcut
- **WHEN** the first version exposes create Space, rename Space, or delete Space
  actions
- **THEN** alan exposes them without default keyboard shortcuts

### Requirement: Keyboard Shortcuts Target Current Selection
Keyboard shortcuts SHALL operate on the current selected Space, selected Tab,
and focused pane. They SHALL NOT target a hovered row or a non-selected context
menu row.

#### Scenario: Pin shortcut is pressed
- **WHEN** the user presses the Pin/Unpin Tab shortcut
- **THEN** alan toggles pin state for the current selected Tab

#### Scenario: Move Tab shortcut is pressed
- **WHEN** the user presses Move Tab Left or Move Tab Right
- **THEN** alan reorders the current selected Tab in the current Space

#### Scenario: Pane shortcut is pressed
- **WHEN** the user presses a pane-focused shell shortcut
- **THEN** alan applies it to the focused pane inside the current selected Tab

#### Scenario: Context menu remains open
- **WHEN** a context menu is open on a non-selected Tab and the user invokes a
  keyboard shortcut outside that menu's direct command handling
- **THEN** alan targets the current selected Tab, not the context menu Tab

### Requirement: Default Shortcut Conflicts Are Detected
The macOS shell SHALL detect duplicate default shortcuts in the same dispatch
context before shipping the keybinding system.

#### Scenario: Duplicate default shortcut exists
- **WHEN** two enabled shell actions declare the same key equivalent and modifier
  set in the same dispatch context
- **THEN** verification fails with both action IDs and the conflicting shortcut

#### Scenario: Same key is valid in separate contexts
- **WHEN** the same key equivalent is intentionally used in separate contexts
  with non-overlapping dispatch ownership
- **THEN** verification records the context boundary and does not fail

### Requirement: Input Precedence Is Explicit
The macOS shell SHALL define shortcut precedence so Find, terminal input, and
shell actions do not compete unpredictably.

#### Scenario: Find is active
- **WHEN** Find is active and the user presses a Find-owned key
- **THEN** Find handles the key before shell action dispatch

#### Scenario: Terminal owns input
- **WHEN** the focused terminal view owns a key sequence as terminal input
- **THEN** alan delivers the sequence to the terminal rather than invoking a
  shell action

#### Scenario: Shell action shortcut is available
- **WHEN** Find does not own the key, the terminal does not own the sequence,
  and a matching available shell action exists
- **THEN** alan invokes the registered shell action

#### Scenario: Action is unavailable
- **WHEN** a registered shortcut matches an action that is unavailable in the
  current shell state
- **THEN** alan does not mutate shell state and reports a stable unavailable
  reason for diagnostics where appropriate

### Requirement: Command-P is not an Ask alan keybinding
The macOS shell keybinding system SHALL NOT bind `Command-P` to Ask alan, a
floating command input, a command palette, or a replacement alan launcher.

#### Scenario: Default shortcuts are enumerated
- **WHEN** default shell shortcut descriptors are validated
- **THEN** no descriptor maps `Command-P` to Ask alan, command input open, or
  first-party alan tab creation

#### Scenario: Command-P is pressed
- **WHEN** the user presses `Command-P` in the default shell
- **THEN** Alan does not intercept it to open an Ask alan or typed command input
  surface

### Requirement: First-party alan tab shortcut is absent
The macOS shell keybinding system SHALL NOT provide a default keyboard shortcut
for creating first-party alan tabs.

#### Scenario: Default shortcuts are validated
- **WHEN** shell shortcut descriptors are validated
- **THEN** no descriptor maps a key equivalent to New alan Tab or a `.alan`
  launch target
