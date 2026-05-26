## ADDED Requirements

### Requirement: Destructive close commands report confirmation requirements
The macOS shell control plane SHALL route destructive PaneSlot and Tab close
commands through the same active-work guard used by interactive close paths and
MUST NOT silently apply a guarded close mutation when terminal work is active.

#### Scenario: Control close pane requires confirmation
- **WHEN** a control client requests close for a PaneSlot whose terminal content has active work
- **THEN** the response reports `applied: false` with a stable `requires_confirmation` result
- **AND** the response identifies the guarded PaneSlot or terminal ContentInstance when available
- **AND** shell state and terminal runtime state remain unchanged

#### Scenario: Control close tab requires confirmation
- **WHEN** a control client requests close for a Tab containing at least one terminal ContentInstance with active work
- **THEN** the response reports `applied: false` with a stable `requires_confirmation` result
- **AND** no terminal ContentInstance in that Tab is finalized by the rejected command

#### Scenario: Control close idle target succeeds
- **WHEN** a control client requests close for a PaneSlot or Tab whose terminal content is idle, exited, or absent
- **THEN** Alan may apply the existing close mutation and return an authoritative result

#### Scenario: Force close is not implicit
- **WHEN** a control client sends an existing close command without an explicit future force-close contract
- **THEN** Alan treats the command as non-forcing and applies confirmation-required semantics for active terminal work
