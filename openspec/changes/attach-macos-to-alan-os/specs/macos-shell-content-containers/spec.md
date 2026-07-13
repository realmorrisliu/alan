## ADDED Requirements

### Requirement: Agent content references an Alan OS Process
Alan for macOS SHALL support an Agent ContentInstance whose domain payload is an
Agent Attachment reference and whose placement remains owned by Space, Tab,
PaneSlot, and window presentation state.

#### Scenario: Agent content moves between Panes
- **WHEN** the user moves the ContentInstance
- **THEN** its Process Reference remains unchanged
- **AND** Alan OS Process lifecycle receives no mutation

### Requirement: Multiple Agent content views may share a Process
Alan for macOS SHALL allow more than one ContentInstance to attach to the same
Process Reference. Each SHALL own renderer offsets and neither SHALL infer
exclusive Process ownership.

#### Scenario: One duplicate view closes
- **WHEN** another view remains attached
- **THEN** the Process and other view remain unaffected
