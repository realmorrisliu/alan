## MODIFIED Requirements

### Requirement: Terminal Startup Uses Resolved Terminal Profile
The macOS terminal lifecycle SHALL launch terminal content using the resolved
Terminal Profile while preserving the existing Ghostty-backed terminal surface
creation path. When no explicit or Space-bound Terminal Profile applies, terminal
startup SHALL use the built-in `Login shell` identity.

#### Scenario: Terminal content starts with profile command
- **WHEN** terminal content is created with `terminal_profile_id` `alan`
- **AND** local Terminal Profile `alan` is a `sudo_user` profile for Unix user
  `alan`
- **THEN** alan resolves the terminal boot command to the structured sudo-user
  launch for `alan`
- **AND** Ghostty surface creation still receives the command, working
  directory, and environment through the existing terminal boot profile

#### Scenario: Unbound terminal starts login shell
- **WHEN** terminal content is created without an explicit
  `terminal_profile_id`
- **AND** the selected Space has no `terminal_profile_id`
- **THEN** alan resolves terminal startup to the current user's login shell
- **AND** alan does not capture a separate global default Terminal Profile for
  that terminal content

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
