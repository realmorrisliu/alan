# os-sandbox-enforcement Specification

## Purpose
Defines pluggable macOS and Linux Tool sandbox enforcement, safe degradation,
projected confinement inputs, sensitive-read controls, and reified namespace
behavior independent of command-text heuristics.

## Requirements

### Requirement: Pluggable OS sandbox backends
Tool execution SHALL be constrained by a selectable sandbox backend behind a common abstraction, with macOS and Linux backends and a degraded path-guard fallback.

#### Scenario: macOS selects the Seatbelt backend
- **WHEN** the agent runs on macOS with Seatbelt available
- **THEN** tool execution is confined by the Seatbelt backend

#### Scenario: Linux selects an OS backend when available
- **WHEN** the agent runs on Linux with the required kernel capabilities
- **THEN** tool execution is confined by the Linux backend (filesystem and network controls)

#### Scenario: Fallback to the path guard
- **WHEN** no OS sandbox backend is available
- **THEN** backend selection falls back to the namespace and Host Mount path guard

### Requirement: Kernel-enforced confinement independent of command syntax
When an OS sandbox backend is active, the operating system SHALL enforce
filesystem writes to the writable roots of the active `SandboxSpec` and control
network access regardless of how a command is written. Exact writable Host
Mount grants SHALL become writable roots; no ambient Host directory SHALL seed
the sandbox. Read-only Host Mounts SHALL NOT grant native-subprocess write
authority.

#### Scenario: Internal-write command is confined
- **WHEN** a command writes outside the active `SandboxSpec` writable roots
  through program-internal logic without an explicit path operand
- **THEN** the OS sandbox blocks the out-of-writable-roots write
- **AND** confinement does not depend on parsing the command string

#### Scenario: Network is controlled by the sandbox
- **WHEN** a confined command attempts network access that the active policy does
  not permit
- **THEN** the OS sandbox prevents it

#### Scenario: Declared writable host mount permits native writes there
- **WHEN** a host directory is declared with write access and projected into the
  active `SandboxSpec`
- **THEN** a native subprocess confined by the OS sandbox can write below that
  declared host path
- **AND** writes outside all writable roots remain blocked

#### Scenario: Declared read-only host mount does not permit native writes
- **WHEN** a host directory is declared read-only
- **THEN** that host path is not included in the active `SandboxSpec` writable
  roots
- **AND** a native subprocess is not granted write authority there by the
  sandbox projection

### Requirement: Safe degradation when no sandbox is available
When no enforcing sandbox backend is available, the system SHALL NOT auto-approve bash or network operations and SHALL escalate them instead. Sandbox-unavailable SHALL NOT be treated as sandbox-disabled-and-allow.

#### Scenario: No backend means escalate
- **WHEN** the host has no available OS sandbox backend
- **THEN** bash and network operations are escalated for human approval rather than auto-approved

#### Scenario: Degraded state is detectable
- **WHEN** the sandbox backend is unavailable or degraded
- **THEN** the active backend/degraded state is reported (e.g. via the decision audit or backend name)

### Requirement: Heuristic command parsing is not the enforcement mechanism
With an OS sandbox backend active, the bash command-string heuristic SHALL NOT be the line of defense for confinement and SHALL be reducible to advisory pre-flight or removed.

#### Scenario: Enforcement comes from the kernel
- **WHEN** a tool runs under an active OS sandbox backend
- **THEN** confinement correctness does not rely on the bash command-shape parser

### Requirement: Linux network confinement via seccomp
On Linux, the OS sandbox backend SHALL confine network access (in addition to Landlock filesystem confinement) so that filesystem and network are both contained, matching macOS Seatbelt. This removes the platform asymmetry so that network escalations can be reviewer-judged uniformly rather than always requiring a human on Linux.

#### Scenario: Linux confines network alongside the filesystem
- **WHEN** a tool runs under the active Linux sandbox backend
- **THEN** disallowed network access is prevented by the OS (via seccomp or equivalent), as filesystem writes are by Landlock

#### Scenario: Network escalation is reviewer-eligible on both platforms
- **WHEN** an operation requests network access and an OS backend that confines network is active
- **THEN** the escalation is eligible for reviewer judgment rather than being forced to a human due to a missing network backstop

#### Scenario: Missing network confinement falls back to the human
- **WHEN** no backend that confines network is available on the host
- **THEN** network operations are surfaced to the human rather than reviewer-judged or auto-allowed

### Requirement: Confinement input is a projected SandboxSpec
The Host adapter SHALL derive each Tool Process `SandboxSpec` from the explicit
service-owned Host Mount grants delegated to that launch, together with its
network policy and executable needs. The spec SHALL be attributable to that
Tool Process and SHALL contain the complete native inputs needed by the selected
OS sandbox backend. Agent Execution Engine, alan9 Kernel, and composition roots
MUST NOT reconstruct native Host roots from Process namespace paths,
descriptors, grant IDs, or declaration lists.

#### Scenario: A Tool Process receives explicit-mount-only confinement
- **WHEN** a Tool Process is spawned with one writable Host Mount and no network authority
- **THEN** the selected OS sandbox backend receives a matching `SandboxSpec`
- **AND** the Host adapter maps only that delegated grant to native confinement

#### Scenario: Virtual namespace mounts grant no native authority
- **WHEN** a Tool Process receives only virtual alan9 mounts and no delegated
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

### Requirement: Linux reified namespace backend provides full read isolation
The Linux reified namespace backend SHALL run native subprocesses inside a
reified filesystem view derived from host-backed alan9 mount declarations when
the host has the required namespace capabilities. In this mode, undeclared host
paths SHALL be absent from the subprocess view by default, providing full read
isolation through filesystem reification rather than command parsing or a
sensitive-read denylist.

#### Scenario: Reified backend exposes declared host mounts at namespace paths
- **WHEN** a Linux runtime starts a native subprocess with a declared host-backed mount `/mnt/project`
- **THEN** the subprocess sees the mounted content at `/mnt/project`
- **AND** the subprocess does not need to use the original host path to access that mount

#### Scenario: Undeclared host paths are absent
- **WHEN** a command running under the reified backend attempts to read an undeclared host path such as the user's home secret directory
- **THEN** that path is absent or unreachable from the subprocess filesystem view
- **AND** read isolation does not depend on the command-shape parser

#### Scenario: Virtual alan9 mounts are not exposed as native paths
- **WHEN** the alan9 namespace contains virtual mounts such as `/agent`, `/srv`, `/proc`, or `/mnt/llm`
- **THEN** the reified native subprocess view does not expose those mounts as host filesystem paths
- **AND** only host-backed declarations contribute native bind mounts

### Requirement: Reification preserves mount access and execution substrate boundaries
The reified Linux backend SHALL distinguish declared host mounts from execution
substrate. Declared read-write host mounts SHALL be writable where mounted;
declared read-only host mounts SHALL be readable but not writable; execution
substrate needed to launch commands SHALL be mounted read-only and SHALL NOT
grant access to user data outside declared mounts.

#### Scenario: Read-write host mount permits mutation at the reified path
- **WHEN** a declared host mount has read-write access
- **THEN** a native subprocess running under the reified backend can write under the corresponding reified namespace path
- **AND** writes outside declared writable mounts are rejected

#### Scenario: Read-only host mount rejects mutation at the reified path
- **WHEN** a declared host mount has read-only access
- **THEN** a native subprocess running under the reified backend can read under the corresponding reified namespace path
- **AND** write attempts under that path are rejected

#### Scenario: Execution substrate does not expose user data
- **WHEN** the reified backend mounts system paths needed to execute `/bin/sh` and common tools
- **THEN** those system paths are mounted only as execution substrate
- **AND** user home directories and secret stores are not exposed unless explicitly declared

### Requirement: Linux reification degrades safely
The Linux reified namespace backend SHALL be selected only when the host can
create the required user namespace, mount namespace, bind mounts, read-only
mounts, and network confinement. If any required capability is unavailable, the
runtime SHALL fall back to the existing Linux projection backend or path guard
according to current safe-degradation rules and SHALL report why reification is
unavailable.

#### Scenario: Missing namespace capability falls back
- **WHEN** the host cannot create an unprivileged user or mount namespace
- **THEN** the Linux reified backend is not selected
- **AND** backend reporting includes the missing capability reason
- **AND** the runtime continues with Landlock or path-guard fallback behavior

#### Scenario: Missing network confinement is degraded
- **WHEN** the filesystem view can be reified but network confinement is unavailable for a network-denied command
- **THEN** the backend reports degraded network confinement
- **AND** policy routes network-capable operations to a human, denies them, or
  falls back to a backend with network confinement
- **AND** the autonomous reviewer cannot approve network-capable execution
  without an OS network-confinement backstop

#### Scenario: Backend audit names the active path
- **WHEN** a native subprocess is evaluated or executed
- **THEN** the decision audit identifies whether the active Linux path is `linux_reified_namespace`, `landlock`, or `host_mount_path_guard`
- **AND** the audit distinguishes reified namespace paths from projected host paths

### Requirement: Native shell preflight distinguishes supported read-only manifest inputs
With an enforcing OS backend, native shell preflight SHALL validate the manifest
input of direct Cargo build, check, test and run commands against explicit readable
authority, independently of the command's overall Write capability. This applies
to separated or inline `--manifest-path` before Cargo's argument terminator. It
MUST NOT add writable authority, rewrite command text, alter policy classification,
allow a read-only cwd for a Write command or bypass sensitive/protected path checks.
Unknown operand roles and a backend without OS enforcement SHALL retain existing
conservative command-capability validation. macOS shared temporary-directory
write allowances MUST NOT override explicit read-only roots; explicit writable
descendant roots SHALL retain only their existing writable authority.

#### Scenario: A Write-classified build reads a separately granted manifest
- **WHEN** an OS-confined Cargo build/check/test/run uses an explicit readable manifest and writable cwd/output
- **THEN** preflight permits the manifest read in separated or inline option form
- **AND** compound commands keep the same per-operand boundary and original command text

#### Scenario: Cargo selectors and global options precede the subcommand
- **WHEN** a supported Cargo invocation selects an installed toolchain or places global options before build/check/test/run
- **THEN** manifest operand classification and Rustup inspection use the same effective-subcommand parser
- **AND** global option values are consumed as values rather than mistaken for subcommands
- **AND** original command text, policy classification, output checks and the argument terminator remain unchanged

#### Scenario: Outputs and unrelated arguments retain writable checks
- **WHEN** a command targets a read-only path for output, redirection, another command or arguments after Cargo's terminator
- **THEN** the manifest input role does not authorize those writes or operands
- **AND** unknown Cargo subcommands and unrelated utilities receive no manifest input exception

#### Scenario: Manifest authority disappears or escapes
- **WHEN** the manifest is outside current readable grants, resolves through an escaping symlink or matches a sensitive-read deny
- **THEN** preflight rejects the input before execution
- **AND** a previously successful read does not confer authority on the next command

#### Scenario: No enforcing backend is available
- **WHEN** execution has only a path-guard backend and a Write-classified command references a read-only manifest
- **THEN** preflight retains its conservative rejection
- **AND** no read-only input exception weakens OS-unavailable degradation or approvals

#### Scenario: Read-only source resides in a shared temporary directory
- **WHEN** an OS-confined program receives a read-only source below a generally writable temporary directory
- **THEN** it can read the source but native program-internal writes to that root are denied
- **AND** explicitly writable scratch remains writable, including a writable child of a read-only ancestor

#### Scenario: Rustup directory selection survives project namespace remapping
- **WHEN** an inspected Cargo/Rust command uses a granted project whose Host path differs from its native namespace path
- **THEN** Host directory-override and toolchain-file proximity determine the effective selector before projection
- **AND** each inspected directory's effective selection is pinned at all validated equivalent namespace projections, including fixed shell cd outcomes
- **AND** explicit selectors retain priority, only selected runtimes are mounted, and unprojectable/conflicting selections refuse before effects
- **AND** Host configuration, delegated rights and selection metadata revalidation remain unchanged; pins do not qualify rewriting selection metadata within the same compound command.

#### Scenario: An alias-only project retains its canonical Rustup override
- **WHEN** the inspected cwd and sole Host mount source are aliases while Rustup settings key the override to the exact canonical target
- **THEN** canonical inspection cwd/Host roots retain that override before selection, with no extra canonical child mount needed
- **AND** remapped and identity execution agree with Host selection while original plan inputs still undergo path/authority validation.

#### Scenario: Earlier ordinary executable precedes a Rustup proxy
- **WHEN** an ordinary executable is the first regular executable allowed by current PATH lookup, or the command names its absolute path
- **THEN** a later proxy does not cause unrelated Rustup metadata inspection or runtime projection
- **AND** the original PATH and actual provider spelling remain unchanged; discovery grants no extra mount authority

#### Scenario: Inspected provider changes before launch
- **WHEN** executable availability, contents, canonical provider or validated proxy classification changes after inspection
- **THEN** revalidation refuses before command effects instead of falling through to another provider
- **AND** a compiler-using ordinary Cargo subcommand's default PATH rustc lookup is also revalidated

#### Scenario: Provider lookup context cannot be preserved
- **WHEN** a Rust command uses an unproved executable alias, relative executable path, or wrapper changing the inspected lookup environment or cwd
- **THEN** the unsupported invocation refuses before effects
- **AND** the default-lookup mixed compiler guard does not imply qualification of Cargo compiler configuration or RUSTC/wrapper overrides

#### Scenario: Cargo selects a compiler through configuration
- **WHEN** a compiler-using Cargo or direct Cargo frontend invocation supplies environment, inline, extra-file or visible ancestor compiler/wrapper overrides or a selected alias
- **THEN** unsupported compiler/context selections refuse before effects rather than snapshotting an unrelated PATH rustc
- **AND** configuration inspection uses only existing native projections, checks every fixed cwd outcome and rejects uncertain or parent-traversing directory selection
- **AND** validated configuration bytes and their digest come from the same read; present and absent inputs are rechecked before launch.

#### Scenario: A proven compiler-free Cargo command has no ordinary rustc
- **WHEN** ordinary Cargo runs a proven compiler-free builtin or metadata --no-deps while only a later Rustup proxy rustc exists
- **THEN** absence of an ordinary compiler does not reject the command or project an unrelated Rustup runtime
- **AND** plain metadata/clean and custom help/aliases retain conservative compiler/configuration inspection, including help targets after the argument delimiter.

#### Scenario: A compound command visibly rewrites selected Cargo configuration
- **WHEN** the same shell request contains a literal configuration redirection, file write or ancestor replacement followed by compiler-using Cargo
- **THEN** the bounded preflight rejects it before effects and requires separate configuration-edit and execution requests
- **AND** quoted output remains ordinary content; an opaque preceding program or earlier Rust work requires a separate execution request even when no literal writer is visible.

#### Scenario: Shell lookup no longer resolves the inspected executable
- **WHEN** a Linux Rust request defines a shell function, changes aliases/hash lookup or hides Rust execution in an unqualified nested shell
- **THEN** inspection rejects it before effects rather than recording a PATH provider that the shell does not execute
- **AND** quoted source, comments and literal here-document contents containing function-like text remain ordinary content; actual command/process substitutions are an unsupported execution context, including expansions in unquoted here-documents.

#### Scenario: A compound command can create a later working directory
- **WHEN** an inspected Linux cd target is absent or not a directory before execution
- **THEN** subsequent cwd-dependent Cargo inspection treats it as uncertain and refuses before effects
- **AND** earlier mkdir or configuration writes cannot make an uninspected Cargo cwd appear during the same request.

#### Scenario: Ordinary Cargo launches auxiliary frontends
- **WHEN** an ordinary Cargo invocation in the Linux namespace can launch documentation, default doctest, formatter, Clippy or a custom Cargo frontend
- **THEN** supported command-specific PATH helper decisions are frozen and revalidated, and a later unqualified proxy refuses without projecting an unrelated runtime
- **AND** helper replacement/loss refuses before launch, compiler-free queries keep their narrower requirements, and the fresh private Cargo home is not a qualification of in-request helper mutations.

#### Scenario: Known frontend queries preserve frontend and configuration binding
- **WHEN** Linux ordinary Cargo dispatches fmt or Clippy help, or Clippy version
- **THEN** only the proven compiler-free frontend must be bound and revalidated
- **AND** selected configuration aliases and compiler overrides remain inspected
- **AND** custom commands, fmt version and forwarded payload retain conservative child requirements

#### Scenario: Root Cargo queries retain dispatch-aware boundaries
- **WHEN** Cargo handles a global query before a trailing command
- **THEN** only actual Cargo/runtime selection is bound, without unused frontend or target requirements
- **AND** root help returns before alias expansion while named/frontend help keeps its checks
- **AND** version/list selected aliases are inspected and unsafe default-alias combinations refuse before effects

#### Scenario: Mixed Cargo short options preserve arity and directory inspection
- **WHEN** Cargo short options combine a root query with a value-taking option
- **THEN** only actual h/V flags are queries; Z/C consume attached or separate values, and real h retains its immediate-help precedence
- **AND** directory-changing C in visible prefix or subcommand arguments retains inspection refusal, while consumed values and -- payload do not create flags
- **AND** selected aliases and frontend arguments retain their existing checks

#### Scenario: Opaque programs precede an inspected Rust invocation
- **WHEN** a Linux request runs an unmodeled program or Rust work that can execute project code before another inspected Rust invocation
- **THEN** admission refuses the request before effects, including possible changes in private Cargo/Rustup homes, and requires separate requests
- **AND** literal cwd selection, command-local selection, bounded read-only text producers and genuine system queries retain their supported subset; dynamic control flow, asynchronous execution and loader overrides refuse

#### Scenario: A transparent wrapper is not the actual standard provider
- **WHEN** a request names a transparent wrapper or a supported system query whose actual namespace provider is unverified, missing or replaced
- **THEN** admission or prelaunch revalidation refuses before effects using the actual projected identity, without supplementing unmounted paths from the Host
- **AND** native system cross-root aliases require unchanged system projections and an actually projected target; Host-only alias equality does not qualify a remapped native route

#### Scenario: A bounded read-only pipeline supplies a Rustc invocation
- **WHEN** a literal supported read-only producer supplies the last Rustc command in a pipeline, without earlier opaque or project-code execution
- **THEN** inspection preserves the existing invocation and verified provider bindings
- **AND** a concurrent writer after Rustc, dynamic producers or nested cwd/environment controls refuse before effects

#### Scenario: A Rust-named project query precedes inspected execution
- **WHEN** a command uses an ordinary project-owned Rust-named executable with query-looking arguments before later inspected Rust execution
- **THEN** the adapter requires separate execution requests before any program effects, including when the project or individual program is granted read-only
- **AND** frozen identity/content or a project proxy-shaped directory alone MUST NOT count as proof of read-only query semantics
- **AND** only actual native standard providers or validated Rustup proxies outside explicitly granted Host trees retain the bounded query-prelude exemption
- **AND** a Rustup proxy with hard links outside its validated proxy directory receives no query-prelude exemption, including links reachable through directory or individual-file grants; symbolic aliases do not count as additional hard links
- **AND** query-prelude link isolation is revalidated before execution, so an external hard link introduced after discovery refuses before command effects without restricting standalone execution
- **AND** forwarded rustup run calls require a separate request before later inspected Rust execution; standalone calls keep existing authority

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

#### Scenario: Other Rust runtimes are installed
- **WHEN** a command selects one or more supported Rust runtimes from the installed catalog
- **THEN** only those selected runtimes enter the read-only execution substrate and retained runtime validation evidence
- **AND** unrelated installed runtime roots remain unavailable to child code
- **AND** a command without any Rust runtime selection projects no installed Rust runtime
- **AND** startup PATH checks and commands without a runtime selection do not validate or project unrelated custom runtime entries
- **AND** selecting an unsupported runtime alias still refuses before effects

#### Scenario: Rustup dispatches a direct installed proxy command
- **WHEN** a literal `rustup run` selects an installed standard runtime and a direct supported Rust proxy command
- **THEN** inspection validates that runtime and the actual command's required executables and helpers before effects
- **AND** nested selectors, arbitrary child dispatch and unmodeled Rustup management commands refuse before effects rather than changing the inspected selection

#### Scenario: A Rust invocation requests literal compilation targets
- **WHEN** an admitted Cargo, rustc, rustdoc or Clippy invocation requests literal `--target` values, including equals syntax, repeated Cargo targets or compiler arguments forwarded by Cargo
- **THEN** inspection checks every requested target against the selected runtime's installed standard-library components before user effects
- **AND** the Cargo host-tuple alias preserves the installed host target while absent targets and unsupported custom specifications refuse before effects
- **AND** target-like arguments forwarded to a Cargo-run application or test executable remain ordinary content rather than compilation targets
- **AND** bounded direct Rustup-run dispatch uses the same target checks and selected manifest revalidation

#### Scenario: A native script changes directory before a Rust invocation
- **WHEN** a compound native script uses literal directory changes before invoking a standard Rustup proxy
- **THEN** admission checks every bounded possible directory selection inside delegated project authority, retaining both success and failure paths rather than reusing only the initial cwd
- **AND** a missing or non-directory target retains the failed branch's cwd while existing delegated targets remain inspectable for subsequent changes
- **AND** all potentially selected runtimes, components, helpers and project selection metadata are validated and rechecked before user effects
- **AND** an uninspectable, escaping or excessive directory state requires explicit command, command-local or inherited Rust selection, or refuses before effects without inferring another grant

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
