## ADDED Requirements

### Requirement: Workspace Manifest Stores Space-Local Tab Selection
The macOS shell workspace manifest SHALL persist each Space's remembered
selected Tab in addition to the globally selected Space and active Tab. Manifest
load, pruning, materialization, and writeback SHALL repair invalid Space-local
selected Tab references without deleting durable Spaces or fabricating Tabs for
empty Spaces.

#### Scenario: Manifest write stores each Space selection
- **WHEN** the user selects a non-first Tab in Space A
- **AND** the user selects a different Tab in Space B
- **THEN** the workspace manifest stores Space A's remembered selected Tab on the Space A record
- **AND** the workspace manifest stores Space B's remembered selected Tab on the Space B record
- **AND** the manifest still records the globally selected Space and active Tab for restart focus

#### Scenario: Restart restores inactive Space selections
- **WHEN** alan restarts from a workspace manifest with per-Space selected Tab records
- **THEN** the globally selected Space and active Tab are restored as the current shell focus
- **AND** every inactive Space keeps its remembered selected Tab for later Space switching
- **AND** switching to an inactive Space after restart selects that Space's remembered Tab instead of its first Tab

#### Scenario: Old manifest without Space-local selection decodes
- **WHEN** alan loads a valid workspace manifest that has global `selected_space_id` and `selected_tab_id` but no per-Space selected Tab fields
- **THEN** alan decodes the manifest successfully
- **AND** alan seeds the globally selected Space's remembered selected Tab from the global selected Tab when valid
- **AND** alan falls back to the first Tab for other Spaces until the user selects a Tab in those Spaces

#### Scenario: Selected Tab is pruned
- **WHEN** lifecycle pruning removes a Tab that a Space remembered as selected
- **THEN** alan repairs that Space's remembered selected Tab to the first retained Tab in the same Space
- **AND** if no Tabs remain in that Space, alan clears that Space's remembered selected Tab while keeping the Space record

#### Scenario: Tab moves between Spaces
- **WHEN** a Tab moves from one Space to another
- **THEN** alan repairs the source Space's remembered selected Tab if the moved Tab was remembered there
- **AND** alan preserves the destination Space's remembered selected Tab unless the move follows current selection or explicitly focuses the moved Tab
- **AND** the persisted manifest records the repaired source and destination Space selection outcomes
