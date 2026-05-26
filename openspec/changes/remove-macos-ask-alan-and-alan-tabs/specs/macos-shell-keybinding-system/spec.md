## ADDED Requirements

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
