## ADDED Requirements

### Requirement: Workspace Manifest Stores Terminal Profile References
The macOS shell workspace manifest SHALL persist Terminal Profile references for
Spaces and terminal content without embedding machine-local Terminal Profile
definitions.

#### Scenario: Space profile reference is saved
- **WHEN** a Space is bound to Terminal Profile `alan`
- **THEN** the workspace manifest stores `terminal_profile_id` `alan` on that
  Space record
- **AND** the manifest does not store the `alan` profile command, Unix user,
  color, icon, or default working directory definition

#### Scenario: Terminal content profile reference is saved
- **WHEN** a terminal content instance is created using Terminal Profile `univer`
- **THEN** the terminal content restore payload stores `terminal_profile_id`
  `univer`
- **AND** restore can explain which Terminal Profile the content was created
  with

#### Scenario: Old manifest decodes without profile fields
- **WHEN** alan reads a workspace manifest created before Terminal Profiles
- **THEN** alan decodes the manifest successfully
- **AND** missing `terminal_profile_id` fields are treated as absent profile
  references

#### Scenario: Missing local profile does not rewrite manifest
- **WHEN** alan restores a manifest that references Terminal Profile `lab` but
  the local profile store does not define `lab`
- **THEN** alan preserves the `lab` reference in the workspace manifest
- **AND** alan does not delete the Space, terminal content, or missing reference
  during normal restore

### Requirement: Workspace Manifest Keeps Profile Reference Ownership Narrow
The macOS shell workspace manifest SHALL treat `terminal_profile_id` as a local
startup reference and SHALL NOT make Terminal Profile definitions portable
workspace state.

#### Scenario: Workspace is shared to another Mac
- **WHEN** a workspace manifest containing `terminal_profile_id` values is used
  on a Mac with different local Terminal Profiles
- **THEN** alan resolves matching local ids when available
- **AND** alan shows missing-profile fallback for unmatched ids
- **AND** alan does not attempt to synthesize profile definitions from the
  workspace manifest
