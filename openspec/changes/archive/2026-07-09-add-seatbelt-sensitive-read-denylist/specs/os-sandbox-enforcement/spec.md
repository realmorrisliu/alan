## ADDED Requirements

### Requirement: macOS sensitive-read denylist
Default sandbox specs SHALL include a sensitive-read denylist for common
home-directory secret, credential, keychain, and browser-profile locations.
When the active backend is macOS Seatbelt, those paths SHALL be projected into
the generated Seatbelt profile as read-deny rules. Backends that cannot express
broad reads with selected deny paths SHALL NOT claim sensitive-read denylist
enforcement.

#### Scenario: Default sandbox spec includes sensitive paths
- **WHEN** a sandbox spec is seeded from a workspace root on a host with a known user home directory
- **THEN** the spec includes read-deny entries for Alan home stores, common credential stores, macOS keychains, and browser profile directories

#### Scenario: Seatbelt profile denies sensitive reads
- **WHEN** a macOS Seatbelt profile is generated from a sandbox spec with read-deny entries
- **THEN** the profile contains `deny file-read*` rules for those read-deny entries

#### Scenario: Host mount projection preserves read denies
- **WHEN** the `alan` composition root projects host mount declarations into a sandbox spec
- **THEN** the resulting spec keeps the default sensitive-read denylist while adding read-write host mount roots

#### Scenario: Linux does not over-claim read-deny enforcement
- **WHEN** the Linux Landlock backend receives a sandbox spec with read-deny entries
- **THEN** write and network confinement remain active where supported
- **AND** sensitive-read denylist enforcement is not reported as provided by Landlock
