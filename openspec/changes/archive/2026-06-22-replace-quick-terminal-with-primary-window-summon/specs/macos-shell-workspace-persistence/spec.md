## ADDED Requirements

### Requirement: Legacy Quick Terminal Restore Data Is Discarded
The macOS shell workspace manifest loader SHALL tolerate old `quick_terminal`
restore records while discarding them during materialization and omitting them
from future manifest writes.

#### Scenario: Manifest records visible quick terminal
- **WHEN** Alan materializes shell state from a workspace manifest whose
  `quick_terminal` record has visible presentation
- **THEN** Alan discards the quick-terminal record
- **AND** the app does not create a hidden or visible quick-terminal slot
- **AND** the app does not create a terminal runtime, tab, pane, or detached panel
  from that record

#### Scenario: Manifest records hidden quick terminal
- **WHEN** Alan materializes shell state from a workspace manifest whose
  `quick_terminal` record has hidden presentation
- **THEN** Alan discards the quick-terminal record
- **AND** Alan restores only normal Spaces, Tabs, PaneSlots, ContentInstances,
  selected Space, and selected Tab from the manifest

#### Scenario: Manifest is written after legacy quick terminal data is read
- **WHEN** Alan writes a workspace manifest after reading a manifest that
  contained `quick_terminal`
- **THEN** the new manifest omits `quick_terminal`
- **AND** no quick-terminal transcript snapshot, last working directory, or
  presentation state is preserved

## REMOVED Requirements

### Requirement: Quick Terminal launch restore is presentation-hidden
**Reason**: Quick Terminal launch restore is removed. Old quick-terminal
records are discarded instead of restored as hidden presentation state.

**Migration**: Decode old `quick_terminal` records only for load tolerance, then
omit them from materialized shell state and all future manifest writes.

#### Scenario: Manifest records visible quick terminal
- **WHEN** Alan materializes shell state from a workspace manifest whose
  quick-terminal record has visible presentation
- **THEN** Alan discards that record rather than restoring a hidden
  quick-terminal slot

#### Scenario: Manifest records hidden quick terminal
- **WHEN** Alan materializes shell state from a workspace manifest whose
  quick-terminal record has hidden presentation
- **THEN** Alan discards that record rather than preserving hidden
  quick-terminal state
