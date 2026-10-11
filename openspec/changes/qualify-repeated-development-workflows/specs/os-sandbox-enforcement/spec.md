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
