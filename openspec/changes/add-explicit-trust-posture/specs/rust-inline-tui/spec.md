# Spec Delta

## ADDED Requirements

### Requirement: Approval posture and execution isolation are distinguishable
The terminal SHALL distinguish governed versus fully trusted posture and actual confined versus Host-account execution. Activation SHALL show both choices through a trusted-user control; compact presentation MAY use a short indicator with details in status, but MUST NOT hide a broad Host-account execution state or imply unavailable confinement.

#### Scenario: Narrow trusted terminal
- **WHEN** the user runs fully trusted Host-account execution in a narrow pane
- **THEN** a visible indicator and accessible details identify broad native execution without displacing input

#### Scenario: Confinement unavailable
- **WHEN** the selected backend cannot provide its promised boundary
- **THEN** the UI reports the limitation rather than displaying an ordinary protected state
