## ADDED Requirements

### Requirement: Linux development execution preserves selected tools
For a supported Linux environment, confined native execution SHALL preserve the
selected shell/git/Rust executable identity, Rust version and PATH search order.
Required executable and runtime inputs SHALL be projected read-only. Unsupported
or unsafe inputs MUST produce an explicit unavailable reason under existing
safe-degradation rules, never silent replacement or wider ambient access.

#### Scenario: Selected executables are outside system command directories
- **WHEN** an installed supported toolchain resides outside the default system PATH directories
- **THEN** confined execution uses the selected executable and its required runtime files
- **AND** the user's whole home, unrelated tool data and credentials are not exposed

#### Scenario: Two supported PATH entries provide the same command
- **WHEN** the selected PATH orders two visible executable providers
- **THEN** confined execution resolves the same first provider in that order
- **AND** trusted namespace setup helpers remain independent of user PATH

#### Scenario: Selected runtime is absent or unsafe
- **WHEN** a selected tool or required runtime path is missing, relative, escaping, changed or cannot be projected safely
- **THEN** execution reports that unsupported input before user effects
- **AND** it does not substitute another installed tool or weaken confinement

#### Scenario: A supported installed Rustup toolchain is selected
- **WHEN** a literal Rust invocation selects an installed standard runtime through a command selector, original environment override, directory override or delegated project toolchain file
- **THEN** confined execution preserves that selection precedence and exact runtime
- **AND** private Rustup settings expose neither unrelated overrides nor the Host management tree
- **AND** a missing requested runtime, component or target refuses before user effects

#### Scenario: Rust selection uses a literal command-local override or wrapper
- **WHEN** a literal Rust invocation uses command-local `RUSTUP_TOOLCHAIN` assignments or a transparent wrapper admitted by the existing command-shape rules
- **THEN** inspection resolves the effective Rust command and validates its selected runtime before any user effects
- **AND** a command selector takes precedence over the last command-local override, which takes precedence over the inherited override and project selection
- **AND** environment-clearing wrappers or command-local PATH/Rustup-home and persistent selection changes that cannot preserve the inspected view refuse explicitly before effects

#### Scenario: Inspected Rust selection changes before admission
- **WHEN** selection metadata, component metadata or an inspected executable changes after the Host adapter constructs its environment
- **THEN** runner preparation rechecks the inspected content and path containment before user effects
- **AND** an escaping component-metadata parent or helper alias cannot gain access outside the inspected runtime

#### Scenario: A runtime or delegated tree contains nested mounts
- **WHEN** a supported runtime or delegated tree includes nested mounted content
- **THEN** native projection preserves the visible mounted subtree
- **AND** read-only access protects every descendant mount, including escaped paths
- **AND** any failed required child protection prevents user command execution

### Requirement: Development cache writes use private owning scratch
Linux confined development SHALL use private writable home/cache scratch within
its existing execution lifetime. It MUST NOT inherit personal Cargo credentials,
write installed runtime content or gain ambient writable home access. Project
build output SHALL remain within delegated writable authority or private scratch.
Scratch SHALL remain owned while descendants use it and be released afterward.

#### Scenario: Cargo needs cache and build output
- **WHEN** Cargo builds a supported mounted project
- **THEN** cache writes use private scratch and output uses authorized project space or private scratch
- **AND** personal credentials, unrelated home files and read-only runtime remain unchanged

#### Scenario: Read-only project is built
- **WHEN** a project is delegated read-only and a supported build uses private output
- **THEN** compilation and tests can complete without modifying project files
- **AND** attempted source mutation remains denied

#### Scenario: Delegated files already exist below scratch
- **WHEN** an explicit Host mount is a descendant of the private scratch mount
- **THEN** private environment directories avoid that mount and preserve its contents and access
- **AND** a subsequent command receives fresh home and cache directories

#### Scenario: A supported tool needs native Process discovery
- **WHEN** the runtime reads `/proc/self/exe` in a confined command
- **THEN** native procfs describes only the command's private PID namespace and is read-only
- **AND** no outside-root setup Process remains visible through procfs at user admission
- **AND** a conflicting native `/proc` projection refuses before user effects

#### Scenario: A cancelled command has descendants
- **WHEN** a development command is cancelled or times out with active descendants
- **THEN** existing Process cancellation stops their later effects
- **AND** scratch lifetime and cleanup cannot leave a surviving writer or reclaim files still in use

### Requirement: Development dependencies retain explicit mount authority
Local dependencies outside a delegated project SHALL require their own live
Host Mount authority. Runtime projection MUST NOT infer that authority from a
manifest, PATH, prior successful build or cached output. Revoked, missing or
escaping dependencies SHALL fail without unauthorized reads/writes or replay;
network dependency retrieval remains subject to existing network approval.

#### Scenario: A project uses an explicitly mounted local dependency
- **WHEN** its dependency path resolves inside a separately delegated read-only mount
- **THEN** the supported build reads that dependency with its granted access
- **AND** the dependency cannot be modified by the build

#### Scenario: Dependency authorization is absent or revoked
- **WHEN** a dependency is outside live delegated authority or its mount is revoked
- **THEN** the next build cannot access the dependency through an ambient path or stale grant
- **AND** retained output is not reported as a new successful execution

#### Scenario: A dependency symlink escapes its mount
- **WHEN** a declared local dependency resolves beyond its authorized backing
- **THEN** execution refuses or the sandbox denies the escaped access
- **AND** no extra Host root is inferred from the manifest or symlink

### Requirement: Linux development qualification reports actual execution
Qualification SHALL distinguish environment inventory, backend readiness,
confined execution and real-model development. A supported Linux development
result MUST include actual dependency compilation, tests and git diff under the
selected backend, with artifact and authority assertions. Capability skips,
fallbacks and corrected retries SHALL remain explicit and never count as passes
for an unexercised enforcing backend.

#### Scenario: A native test returns early after a failed capability probe
- **WHEN** readiness prevents the required enforcing backend from running
- **THEN** qualification records an unsupported or failed slot
- **AND** a zero-exit early-return test is not counted as actual confined execution

#### Scenario: Supported shell git and Rust environment is qualified
- **WHEN** the reproducible fixture completes dependency build/test and git diff
- **THEN** evidence records source, binary, selected tools/version, environment, backend, commands and exact artifact effects
- **AND** read isolation, read-only access, network denial, revocation and descendant cancellation have independently checked outcomes
