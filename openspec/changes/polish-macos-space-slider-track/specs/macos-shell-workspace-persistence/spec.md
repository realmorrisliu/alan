## ADDED Requirements

### Requirement: Workspace Manifest Stores Space Presentation Icons
The macOS shell workspace manifest SHALL persist optional Space presentation
icon metadata separately from Terminal Profile definitions so the top Space
slider can render stable Space icons across launches without broadening profile
ownership.

#### Scenario: Space icon metadata is saved
- **WHEN** a Space has explicit presentation icon metadata
- **THEN** the workspace manifest stores that icon metadata on the Space record
- **AND** the `ShellSpace` projection exposes the same icon metadata for
  sidebar rendering
- **AND** the manifest does not treat the Space icon as a Terminal Profile icon,
  terminal content icon, command icon, or provider configuration field

#### Scenario: Old manifest decodes without Space icon metadata
- **WHEN** alan reads a valid workspace manifest created before Space
  presentation icons existed
- **THEN** alan decodes the manifest successfully
- **AND** each Space without icon metadata receives a deterministic default
  presentation icon for UI display
- **AND** alan does not rewrite the manifest solely because the default icon was
  applied for display

#### Scenario: Invalid Space icon metadata falls back safely
- **WHEN** alan reads a Space record whose presentation icon metadata is absent,
  empty, or unsupported by the local icon renderer
- **THEN** alan keeps the Space record and its Tabs intact
- **AND** alan displays the deterministic default Space icon for that Space
- **AND** alan preserves the original manifest evidence unless the user later
  explicitly changes the Space icon

#### Scenario: Terminal Profile reference ownership remains narrow
- **WHEN** a Space has both `terminal_profile_id` and Space presentation icon
  metadata
- **THEN** alan uses `terminal_profile_id` only as the Space's default terminal
  launch profile reference
- **AND** alan uses the Space presentation icon only for Space navigation
  surfaces
- **AND** changing one field does not silently rewrite the other
