## MODIFIED Requirements

### Requirement: macOS is a Connection Service native adapter
Platform Host adapters SHALL own approved native login and secret storage
at the Alan OS Host and Host Command Plane boundary independently of
the desktop product. The retained Alan for macOS adapter is a legacy consumer,
not the durable owner. Its browser/device login and Keychain flows SHALL remain
maintenance-only until their surviving consumers are removed or an explicitly
verified platform adapter supplies equivalent behavior. Desktop removal SHALL
NOT strand required credential functionality or transfer secret authority to
Herdr or a terminal renderer.

Adapters SHALL return only opaque credential references and bounded results
and MUST NOT maintain a second profile/default registry.

#### Scenario: App reconnects after login
- **WHEN** the profile already exists in Connection Service and the retained
  macOS App adapter reconnects
- **THEN** it reads the service status
- **AND** it does not recreate metadata from local preferences

#### Scenario: Desktop credential adapter is removed
- **WHEN** a scoped retirement removes a desktop-hosted credential flow still
  needed by CLI or Alan OS Host consumers
- **THEN** removal is gated on an explicitly verified platform Host adapter
  preserving channel-scoped secret storage and approved login behavior
- **AND** Connection Service remains the metadata owner and Herdr gains no
  credential authority
