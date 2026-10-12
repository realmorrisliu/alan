## ADDED Requirements

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
- **AND** quoted output remains ordinary content; opaque program-internal mutation is not qualified by this bounded check.


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
