## REMOVED Requirements

### Requirement: Apple client source layout mirrors architecture ownership

**Reason**: The source-layout contract still treats Console views and protocol models as live Apple client owners.

**Migration**: Describe only the active macOS shell, terminal, model, controller, service, and support owners.

### Requirement: API clients and event reducers are not embedded in views

**Reason**: The API clients, Session event reducers, and Console projections owned only the retired daemon-backed Apple surface.

**Migration**: Delete those consumers; no replacement Apple-to-Alan OS boundary is selected by this change.

### Requirement: Mobile and legacy console surfaces are isolated from the primary macOS shell

**Reason**: The current Xcode product target is macOS-only and the legacy Console/mobile sources are obsolete rather than supported secondary surfaces.

**Migration**: Delete the obsolete sources and project membership.

### Requirement: Architecture warning debt is reduced by focused slices

**Reason**: One of the required warning slices preserves a migration owner for the deleted Console/mobile tree.

**Migration**: Keep focused debt reduction for active macOS owners only.

## ADDED Requirements

### Requirement: Active Apple source layout mirrors macOS shell ownership

The native Apple client SHALL organize the active macOS product by app startup, shell views, terminal hosts, models, controllers, services, adapters, and support code under `clients/apple/alan-macos`.

#### Scenario: Source tree is inspected

- **WHEN** a developer inspects the active Apple source root and Xcode target membership
- **THEN** every product source belongs to an active macOS owner
- **AND** no source group exists without an active product owner and focused validation boundary

#### Scenario: Architecture docs describe the tree

- **WHEN** Apple architecture documentation lists source owners
- **THEN** the list matches the active Xcode project and filesystem layout
- **AND** it does not preserve a future attachment owner that has not been designed

### Requirement: Deleted Apple compatibility consumers have no replacement stub

The active macOS target SHALL NOT contain an unavailable placeholder, mock service, disabled control, or alternate data source for a deleted compatibility consumer.

#### Scenario: Obsolete consumer removal is reviewed

- **WHEN** the cleanup removes a source group with no active macOS product owner
- **THEN** their source files and project references are deleted
- **AND** unrelated terminal, workspace, update, helper, and shell-control features continue through their existing owners

### Requirement: Architecture warning debt is reduced through active-owner slices

The Apple client SHALL reduce maintainability warnings through focused, behavior-preserving slices that name an active owner and its verification commands.

#### Scenario: Focused slice resolves a warning

- **WHEN** a refactor removes warnings from `check-architecture-maintainability.sh`
- **THEN** the architecture debt ledger and expected warning count are updated in the same change
- **AND** focused checks protect any active terminal, shell-controller, service, or adapter behavior moved by the slice
