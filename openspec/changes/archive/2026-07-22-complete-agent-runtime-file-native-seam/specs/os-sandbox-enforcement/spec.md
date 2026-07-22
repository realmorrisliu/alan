## MODIFIED Requirements

### Requirement: Confinement input is a projected SandboxSpec
The Host adapter SHALL derive each Tool Process `SandboxSpec` from the explicit
service-owned Host Mount grants delegated to that launch, together with its
network policy and executable needs. The spec SHALL be attributable to that
Tool Process and SHALL contain the complete native inputs needed by the selected
OS sandbox backend. Agent Execution Engine, Alan Kernel, and composition roots
MUST NOT reconstruct native Host roots from Process namespace paths,
descriptors, grant IDs, or declaration lists.

#### Scenario: A Tool Process receives explicit-mount-only confinement
- **WHEN** a Tool Process is spawned with one writable Host Mount and no network authority
- **THEN** the selected OS sandbox backend receives a matching `SandboxSpec`
- **AND** the Host adapter maps only that delegated grant to native confinement

#### Scenario: Virtual namespace mounts grant no native authority
- **WHEN** a Tool Process receives only virtual Alan OS mounts and no delegated
  Host Mount grant
- **THEN** no composition root or backend infers a native Host root from those
  namespace paths

### Requirement: macOS sensitive-read denylist
Default sandbox specs SHALL include a sensitive-read denylist for common
home-directory secret, credential, keychain, and browser-profile locations.
When the active backend is macOS Seatbelt, those paths SHALL be projected into
the generated Seatbelt profile as read-deny rules. Backends that cannot express
broad reads with selected deny paths SHALL NOT claim sensitive-read denylist
enforcement.

#### Scenario: Default sandbox spec includes sensitive paths
- **WHEN** a sandbox spec is assembled for a Process on a host with a known user home directory
- **THEN** the spec includes read-deny entries for Alan Host Stores, common credential stores, macOS keychains, and browser profile directories

#### Scenario: Seatbelt profile denies sensitive reads
- **WHEN** a macOS Seatbelt profile is generated from a sandbox spec with read-deny entries
- **THEN** the profile contains `deny file-read*` rules for those read-deny entries

#### Scenario: Host adapter projection preserves read denies
- **WHEN** the Host adapter derives a sandbox spec from delegated read-write
  Host Mount grants
- **THEN** the resulting spec keeps the default sensitive-read denylist while
  adding only those grants' native writable roots

#### Scenario: Linux does not over-claim read-deny enforcement
- **WHEN** the Linux Landlock backend receives a sandbox spec with read-deny entries
- **THEN** write and network confinement remain active where supported
- **AND** sensitive-read denylist enforcement is not reported as provided by Landlock
