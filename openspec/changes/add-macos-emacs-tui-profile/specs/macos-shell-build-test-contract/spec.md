## ADDED Requirements

### Requirement: Emacs TUI Profile Has Focused Verification
The Apple client SHALL verify the managed Emacs TUI Terminal Profile and New
Emacs Tab workflow through focused automated tests, script-level checks, or
documented manual notes where live TUI behavior cannot be fully automated.

#### Scenario: Emacs profile preset verification
- **WHEN** the Terminal Profile store is loaded in a focused test
- **THEN** verification confirms the Emacs TUI preset is present with stable id
  `emacs_tui`, title `Emacs`, and custom-command launch semantics

#### Scenario: New Emacs tab state verification
- **WHEN** the New Emacs Tab command is executed against a shell model or
  control-plane test host
- **THEN** verification confirms the created terminal content stores
  `terminal_profile_id` `emacs_tui`
- **AND** focused Space, tab, pane, cwd, and terminal content identity are
  updated through the normal terminal tab path

#### Scenario: Emacs keyboard verification
- **WHEN** an Emacs TUI pane is focused in a running app verification pass
- **THEN** verification covers printable input, Escape, Tab, Backspace,
  Control-key sequences such as `C-x` and `C-c`, Meta input, paste, terminal
  mouse mode, and alternate-screen behavior
- **AND** alan does not consume those keys as shell workspace commands unless an
  explicit app-reserved `Command` shortcut owns the key

#### Scenario: Emacs lifecycle verification
- **WHEN** an Emacs TUI pane exits, is split, moves to the background, restores
  after restart, or is closed with active work
- **THEN** verification confirms alan reports terminal lifecycle state
  truthfully and preserves normal terminal ContentInstance ownership
