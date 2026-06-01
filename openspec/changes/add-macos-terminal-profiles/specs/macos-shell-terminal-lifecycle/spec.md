## ADDED Requirements

### Requirement: Terminal Startup Uses Resolved Terminal Profile
The macOS terminal lifecycle SHALL launch terminal content using the resolved
Terminal Profile while preserving the existing Ghostty-backed terminal surface
creation path.

#### Scenario: Terminal content starts with profile command
- **WHEN** terminal content is created with `terminal_profile_id` `alan`
- **AND** local Terminal Profile `alan` is a `sudo_user` profile for Unix user
  `alan`
- **THEN** alan resolves the terminal boot command to the structured sudo-user
  launch for `alan`
- **AND** Ghostty surface creation still receives the command, working
  directory, and environment through the existing terminal boot profile

#### Scenario: Profile metadata is projected to terminal environment
- **WHEN** terminal content starts with a resolved Terminal Profile
- **THEN** alan exposes non-secret profile metadata such as profile id and launch
  kind through terminal environment variables
- **AND** alan does not expose provider credentials or secret values through
  those variables

#### Scenario: Custom command startup is marked active
- **WHEN** terminal content starts with a `custom_command` Terminal Profile
- **THEN** alan treats the terminal startup as a foreground command until the
  terminal runtime reports completion or a shell-integration state update

### Requirement: Terminal Restore Reuses Stored Profile Reference
The macOS terminal lifecycle SHALL restore terminal content using its stored
Terminal Profile reference when one exists.

#### Scenario: Restored terminal uses stored profile
- **WHEN** alan restores terminal content from a workspace manifest with
  `terminal_profile_id` `univer`
- **THEN** alan launches the restored terminal using the current local
  definition of Terminal Profile `univer`

#### Scenario: Edited profile affects future restore
- **WHEN** terminal content stores `terminal_profile_id` `alan`
- **AND** the local `alan` Terminal Profile definition changes before app
  restart
- **THEN** the restored terminal uses the updated local `alan` profile
  definition

#### Scenario: Missing restored profile falls back
- **WHEN** alan restores terminal content with `terminal_profile_id` `lab`
- **AND** local Terminal Profile `lab` is missing
- **THEN** alan launches the restored terminal with the login-shell fallback
- **AND** alan reports the missing profile state in shell metadata
