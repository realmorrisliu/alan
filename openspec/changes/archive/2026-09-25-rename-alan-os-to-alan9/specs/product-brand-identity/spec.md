## ADDED Requirements

### Requirement: The agent operating system is named alan9
Alan SHALL use `alan9` as the canonical lowercase name for its agent operating
system, formerly called `Alan OS`. `Alan` SHALL remain the complete product
brand. System component prose SHALL use `alan9 Kernel` and `alan9 Host` for the
existing Kernel and system Host. Alan Shell and service role names SHALL retain
their existing meanings. Explanations of the system name SHALL distinguish its
Plan 9 inspiration from protocol compatibility: alan9 uses aP and does not
claim Plan 9 or 9P compatibility or a standalone bootable OS.

#### Scenario: Product and system are introduced
- **WHEN** active documentation introduces the product and its system
- **THEN** it names the product Alan and the system alan9
- **AND** lowercase alan9 is valid system branding without weakening Alan's
  capitalized product-brand rule

#### Scenario: System Host and terminal host are described
- **WHEN** documentation describes system lifetime and terminal presentation
- **THEN** it names the system authority alan9 Host
- **AND** it distinguishes that component from an external terminal host such as Herdr

#### Scenario: The system name is explained
- **WHEN** documentation explains the name alan9
- **THEN** it describes Plan 9 inspiration and the system's own aP protocol
- **AND** it does not promise 9P compatibility or a bootable replacement operating system

### Requirement: System naming preserves identifiers and historical evidence
Adoption of the alan9 system name SHALL preserve existing executable and crate
names, code identifiers, protocol names, namespace and storage paths, install
channels and OpenSpec capability IDs. Active explanatory prose SHALL adopt the
new system vocabulary. Historical ADRs, archived changes, literal identifiers,
quotations and explicit migration explanations SHALL retain their original
spelling where needed. Naming adoption MUST NOT alter Process authority,
input-routing semantics, credentials, persisted formats or service ownership.

#### Scenario: An existing installation uses the new documentation
- **WHEN** a user follows documentation after the system naming update
- **THEN** `alan` and its existing commands and store locations remain valid
- **AND** no new command alias or data migration is required

#### Scenario: A retained identifier contains the former system name
- **WHEN** a link or literal identifier uses `alan-os-host-lifecycle` or an existing Rust name
- **THEN** the identifier remains unchanged and its surrounding current prose uses alan9

#### Scenario: Historical material is inspected
- **WHEN** a reader opens an older ADR or archived OpenSpec change
- **THEN** its original terminology remains intact
- **AND** that terminology does not change its disposition or implementation status
