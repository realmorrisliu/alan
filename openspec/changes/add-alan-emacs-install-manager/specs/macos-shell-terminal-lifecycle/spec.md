## ADDED Requirements

### Requirement: Alan Terminal Provides Simple Editor Environment
Alan for macOS terminal panes SHALL expose simple editor environment variables
that point command-line editor workflows at the user's installed Emacs command
without requiring wrapper paths or command arguments.

#### Scenario: Terminal boot environment includes editor variables
- **WHEN** Alan for macOS creates a terminal boot profile
- **THEN** the boot environment includes `EDITOR=emacs`
- **AND** it includes `VISUAL=emacs`
- **AND** these values do not include wrapper paths, shell snippets, or
  additional command arguments

#### Scenario: Existing terminal metadata remains present
- **WHEN** editor environment variables are added to a terminal boot profile
- **THEN** existing `ALAN_SHELL_*`, terminal profile, cwd, install channel, and
  shell launch environment metadata remain present according to the existing
  terminal lifecycle contract
