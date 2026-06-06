## ADDED Requirements

### Requirement: Emacs TUI Uses Normal Terminal Lifecycle
The macOS terminal lifecycle SHALL run Emacs TUI panes through the same
Ghostty-backed terminal ContentInstance lifecycle used by other Terminal
Profile-backed panes.

#### Scenario: Emacs terminal starts through resolved profile
- **WHEN** terminal content is created with `terminal_profile_id` `emacs_tui`
- **THEN** alan resolves the managed Emacs TUI Terminal Profile
- **AND** Ghostty surface creation receives the resolved command, working
  directory, and environment through the existing terminal boot profile

#### Scenario: Emacs process stays bound to terminal content identity
- **WHEN** an Emacs TUI pane enters alternate-screen mode, updates its terminal
  title, or receives terminal runtime metadata
- **THEN** alan keeps runtime handle, scrollback, metadata, pending delivery,
  and teardown state bound to the same terminal ContentInstance id
- **AND** alan does not remount the pane as GUI editor content

#### Scenario: Restored Emacs tab relaunches as terminal Emacs
- **WHEN** alan restores terminal content from a workspace manifest with
  `terminal_profile_id` `emacs_tui`
- **THEN** alan relaunches the pane using the current managed Emacs TUI profile
  definition
- **AND** alan does not claim continuity with the previous app instance's Emacs
  process or PTY

#### Scenario: Closing Emacs follows terminal close guard
- **WHEN** the user closes an Emacs TUI pane whose process is still active
- **THEN** alan applies the same destructive terminal close guard used for other
  active terminal work
- **AND** confirmed close finalizes the terminal ContentInstance through the
  existing terminal lifecycle boundary
