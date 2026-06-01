## ADDED Requirements

### Requirement: Quick Terminal boundary refactor has staged verification
The Apple client SHALL verify the Quick Terminal boundary refactor in stages so
the implementation can first prove stable launch safety and compilation, then
add focused behavior coverage.

#### Scenario: First implementation slice is verified
- **WHEN** the dedicated Quick Terminal presentation boundary is implemented
- **THEN** verification includes the focused Apple build needed to prove the
  refactor compiles
- **AND** verification proves stable-channel launch does not trap on Quick
  Terminal Peak setup
- **AND** verification does not operate the Alan Dev app

#### Scenario: Full behavior verification follows
- **WHEN** the follow-up verification slice is performed
- **THEN** tests cover Quick Terminal presentation state transitions for show,
  hide, close, attach, focus, and promotion
- **AND** an AppKit harness or equivalent focused test verifies panel collection
  behavior, visibility, and focus ordering
- **AND** runtime attach/focus sequencing tests prove early focus requests do
  not race host view registration
- **AND** stable-channel Quick Terminal behavior verification still avoids
  touching Alan Dev
