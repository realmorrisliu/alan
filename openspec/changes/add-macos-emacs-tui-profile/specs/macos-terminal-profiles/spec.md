## ADDED Requirements

### Requirement: Managed Emacs TUI Profile Preset
Alan for macOS SHALL provide a first-party Emacs TUI Terminal Profile preset
that launches terminal Emacs as local terminal content without requiring manual
custom-command setup.

The preset SHALL use stable profile id `emacs_tui`, user-facing title `Emacs`,
and Terminal Profile launch mode `custom_command`. The preset SHALL prefer
`emacsclient -nw -a ""`, SHALL fall back to `emacs -nw`, and SHALL keep the
profile channel-local rather than storing it in workspace manifests, agent
definitions, provider connection profiles, or credential stores.

#### Scenario: Emacs preset is available
- **WHEN** Alan for macOS loads Terminal Profiles for the active install channel
- **THEN** the profile list includes the managed Emacs TUI preset with
  `terminal_profile_id` `emacs_tui`
- **AND** the preset is not selected as the default Terminal Profile unless the
  user explicitly chooses it as a Space or global default

#### Scenario: Emacs preset uses terminal profile validation
- **WHEN** Alan validates the managed Emacs TUI preset
- **THEN** alan validates the existing custom-command runner needed to start the
  profile
- **AND** alan does not reject the profile solely because `emacsclient` or
  `emacs` is not currently available on PATH

#### Scenario: Emacs executable is missing
- **WHEN** a terminal starts with the Emacs TUI preset and neither
  `emacsclient` nor `emacs` is available in the terminal PATH
- **THEN** the terminal prints a concise actionable message in the pane
- **AND** alan returns the pane to a normal login shell rather than failing
  Ghostty surface creation

#### Scenario: Emacs profile metadata is non-secret
- **WHEN** a terminal starts with the Emacs TUI preset
- **THEN** alan exposes the existing non-secret Terminal Profile metadata,
  including `ALAN_TERMINAL_PROFILE_ID=emacs_tui`, to the terminal environment
- **AND** alan does not expose provider credentials, tokens, or arbitrary buffer
  contents through that metadata
