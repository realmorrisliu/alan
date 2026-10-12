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
- **AND** an inspected ordinary Cargo subcommand's default PATH rustc lookup is also revalidated

#### Scenario: Provider lookup context cannot be preserved
- **WHEN** a Rust command uses an unproved executable alias, relative executable path, or wrapper changing the inspected lookup environment or cwd
- **THEN** the unsupported invocation refuses before effects
- **AND** the default-lookup mixed compiler guard does not imply qualification of Cargo compiler configuration or RUSTC/wrapper overrides
