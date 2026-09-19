## MODIFIED Requirements

### Requirement: macOS is a Host Mount native adapter
Platform Host adapters at the Alan OS Host and Host Command Plane boundary
SHALL supply native directory authorization and bounded hostfs exports to Host
Mount Service independently of the desktop product. The retained Alan for
macOS request presenter is a legacy consumer, not the durable grant or platform
adapter owner. Its native flows SHALL remain maintenance-only until their
surviving consumers are removed or an explicitly verified platform adapter
supplies equivalent behavior.

Raw Host OS paths and security-scoped handles SHALL remain in the platform
adapter and MUST NOT appear in Agent-visible grant files. Host Mount Service
SHALL remain the sole grant registry and projection/revocation owner.

#### Scenario: Agent requests a read-only directory
- **WHEN** the user approves it through an authorized macOS platform adapter
- **THEN** Host Mount Service receives a read-only export result
- **AND** the Agent sees only its Alan OS mount path and grant metadata

#### Scenario: Desktop mount presenter is removed
- **WHEN** a scoped retirement removes a desktop-native authorization flow
  still needed by a surviving consumer
- **THEN** removal is gated on a verified platform Host adapter preserving
  explicit consent, read-only scope, confinement and revocation
- **AND** neither terminal cwd nor Herdr pane identity grants directory access
