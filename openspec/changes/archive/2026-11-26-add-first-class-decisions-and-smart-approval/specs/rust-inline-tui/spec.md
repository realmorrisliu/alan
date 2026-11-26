# Spec Delta

## MODIFIED Requirements

### Requirement: Approval posture and execution isolation are distinguishable
The terminal SHALL distinguish governed (`rw·gov` / `ro·gov`), confined smart
approval (`rw·smart` / `ro·smart`), confined trusted (`rw·trust` / `ro·trust`),
and unsandboxed Host-account (`! unsandboxed`) execution, with pure-ASCII
fallbacks (`rw/gov`, `rw/smart`, `rw/trust`) when dumb-terminal rendering is
active. Activation SHALL show choices through a trusted-user control; compact
Row 1 presentation MAY use the short posture indicator while `^T` status
details disclose the active System 1 evaluator profile, its calibration kind
(`calibrated` or `uncalibrated`), and smart-approval receipts, and MUST NOT
hide a broad Host-account execution state or imply unavailable confinement.

#### Scenario: ConfinedSmart posture renders distinctly in status bar and status panel
- **WHEN** the user runs under `ConfinedSmart` on a writable project mount
- **THEN** Row 1 displays `rw·smart` (or `rw/smart` in dumb-terminal mode)
  rather than `rw·gov` or `rw·trust`
- **AND** opening `^T` status discloses the active decision evaluator profile
  and whether its calibration is `calibrated` or `uncalibrated`

#### Scenario: Narrow trusted terminal
- **WHEN** the user runs fully trusted Host-account execution in a narrow pane
- **THEN** a visible indicator (`! unsandboxed`) and accessible details
  identify broad native execution without displacing input

#### Scenario: Confinement unavailable
- **WHEN** the selected backend cannot provide its promised boundary
- **THEN** the UI reports the limitation rather than displaying an ordinary
  protected state
