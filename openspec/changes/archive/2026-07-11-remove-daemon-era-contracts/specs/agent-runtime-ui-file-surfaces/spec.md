## MODIFIED Requirements

### Requirement: Agent runtime projects renderer-visible UI state under `machine/ui`
Alan SHALL expose renderer-visible runtime UI state through a runtime-owned `machine/ui/` subtree
in the Agent Process overlay. The subtree SHALL provide readable snapshot files for current
activity, plan, thinking, and latest notice state plus a watchable `machine/ui/events` Stream for
ordered live updates.

#### Scenario: A renderer host hydrates current UI state
- **WHEN** a renderer host attaches to `/agent/<pid>`
- **THEN** it reads `machine/ui/` snapshot files for current activity, plan, renderer-visible
  thinking, and notice state
- **AND** hydration requires no second runtime state object or transport-owned history
