# host-mount-tool-process-sandbox-projection Specification

## Purpose
Defines how Host adapters derive per-Tool-Process native sandbox authority from
explicitly delegated service-owned Host Mount handles without exposing raw Host
backing or retaining engine-owned sandbox roots.

## Requirements

### Requirement: Explicit read-write Host Mounts derive per-Tool-Process authority
The Host adapter SHALL derive native writable authority from the same
service-owned grant when alan9 starts a native Tool Process with an explicitly
delegated read-write Host Mount. Agent Execution Engine
MUST NOT store the raw Host backing, add native paths to an engine-owned sandbox
list, or apply sandbox authority from a grant ID alone.

#### Scenario: Approved read-write grant is passed to a Tool Process
- **WHEN** a Tool Process launch explicitly includes a read-write Host Mount
  handle mounted at `/mnt/project`
- **THEN** the Host adapter adds the grant's native backing to the OS sandbox
  with writable access
- **AND** Tool code continues to address the tree through the alan9 namespace

#### Scenario: Duplicate grant delegation is idempotent
- **WHEN** the same read-write grant handle is included more than once in one
  Tool Process launch
- **THEN** the Host adapter emits one effective native sandbox authorization

### Requirement: Read-only Host Mounts do not grant native write authority
The Host adapter SHALL NOT derive native writable authority from a read-only
Host Mount grant. The approved grant MAY remain readable through its mounted
file-server handle according to the Tool Process namespace and Host sandbox
policy.

#### Scenario: Approved read-only grant is passed to a Tool Process
- **WHEN** a Tool Process launch explicitly includes a read-only Host Mount
  handle
- **THEN** the Host adapter does not add its native backing as a writable root
- **AND** no engine-owned sandbox state is mutated

### Requirement: Host adapters enforce delegated Tool Process sandbox authority
Host adapter containment checks and OS sandbox profile generation SHALL include
the native backing of every explicitly delegated read-write Host Mount grant
and SHALL reject paths outside the Tool Process's delegated writable authority.
Those native roots SHALL remain Host-adapter implementation data and MUST NOT be
serialized into Agent Machine, AgentFS, rollout/checkpoint, or Tool result
records.

#### Scenario: Tool can access an approved writable grant
- **WHEN** the Tool Process has an explicitly delegated read-write Host Mount
  and executes below its alan9 mount
- **THEN** Host containment checks and the OS sandbox permit the corresponding
  native access
- **AND** a path outside every delegated writable grant remains rejected

#### Scenario: Bash uses an approved writable grant as cwd
- **WHEN** bash starts with cwd below an explicitly delegated read-write Host
  Mount
- **THEN** the Host adapter resolves the grant internally and permits the native
  cwd
- **AND** bash launch records do not expose the raw Host path to Agent Execution
  Engine

#### Scenario: Bash preserves cwd across multiple delegated mounts
- **WHEN** a Tool Process cwd is below one of multiple explicitly delegated Host
  Mounts
- **THEN** Host authority reconciliation resolves the cwd through that covering
  mount independent of grant iteration order
- **AND** every other delegated mount remains available with its effective access

#### Scenario: Process requests overlapping Host Mount projections
- **WHEN** a Process already holds an active Host Mount projection and another
  grant would mount at a strict parent or child namespace path
- **THEN** Host Mount Service rejects the overlapping projection before native
  Tool sandbox authority is constructed
- **AND** the existing projection remains unchanged

### Requirement: Native shell guidance distinguishes namespace and shell operands
Model-visible native Bash Tool guidance SHALL distinguish Host-resolved Process
cwd and file-Tool namespace paths from operands embedded in unchanged shell
command text. It SHALL describe cwd-relative operands for mounted project files
and file Tools for absolute Alan namespace paths. A namespace-absolute shell
operand refusal MUST NOT be described as proof that the selected project lacks
authorization. Guidance MUST NOT expose private Host backing, add authority or
claim that arbitrary namespace paths are automatically rewritten for a shell.

#### Scenario: Model receives Bash guidance for a mounted project
- **WHEN** a real Agent receives the builtin Bash definition and command schema
- **THEN** both explain the supported cwd-relative shell path behavior
- **AND** the guidance distinguishes a namespace operand limitation from missing project authority

#### Scenario: Qualification follows a guidance correction
- **WHEN** Tool guidance changes after a failed task
- **THEN** the original failed attempt remains retained
- **AND** a fresh candidate runs the unchanged frozen task before claiming a qualified completion or resolved model-usage finding

### Requirement: Native diagnostic projection preserves public path boundaries
The native Host adapter SHALL project captured Tool stdout, stderr and error text
through one shared output projection. Known mounted backing paths SHALL use their
existing namespace paths with complete path-component boundaries. For ordinary
native diagnostics whose relative paths normalize outside known mounts, private
backing ancestors SHALL use an explicit unmapped marker while retaining useful
diagnostic suffixes. This presentation SHALL NOT infer a grant, invent a usable
namespace for an unavailable resource, or alter native execution authority.

#### Scenario: A known mount shares a spelling prefix with an unmounted sibling
- **WHEN** native output names a mounted project and a sibling whose name starts with the same project spelling
- **THEN** the mounted path uses its namespace path and the sibling does not acquire a fabricated project namespace
- **AND** private backing ancestors in the sibling diagnostic use an unmapped marker

#### Scenario: A missing or revoked dependency normalizes outside the project
- **WHEN** an actual native Tool command reports a relative dependency outside its selected project and the dependency is missing or revoked
- **THEN** the failure remains explicit and no cached build is reported as successful
- **AND** captured diagnostics conceal the private backing ancestor rather than exposing native project location

#### Scenario: Projection replacements overlap another native prefix
- **WHEN** a valid namespace replacement shares spelling with a native ancestor or another mount
- **THEN** the result projects the original text once without rewriting the already projected namespace
- **AND** the most specific known mount wins over unmapped ancestor fallback

#### Scenario: Literal output remains readable
- **WHEN** captured text contains ordinary role lookalikes, comparisons, Markdown quotes, Unicode, color sequences or file URI paths
- **THEN** ordinary literal text retains its content and known native diagnostic paths use the same namespace mapping

#### Scenario: A mounted spelling appears inside ordinary text or a URL
- **WHEN** a mounted root or its ancestor appears inside a network URL or a token with no leading path boundary
- **THEN** projection preserves that literal text rather than inventing a namespace path or unmapped marker
- **AND** root, non-root and ancestor projection share the same leading boundary while retaining existing trailing component checks
- **AND** trailing SGR color controls preserve the preceding visible boundary, so colored absolute paths and file URI paths still project while colored token suffixes remain unchanged

### Requirement: Native shell reads explicitly delegated read-only dependencies
For supported enforcing native execution, the Host adapter SHALL retain the cwd
selected grant first and SHALL additionally project live read-only Host Mounts
explicitly held by the same Process. Only the selected grant SHALL contribute
native writable authority. Other writable grants and grants held only by another
Process MUST NOT gain native access through this projection. The Host adapter
SHALL preserve each grant's effective access, current reconciliation and native
path containment without command rewriting or inferred common-parent authority.
This is a proposed extension to ADR-0058's delivered single-grant slice; adoption
requires review and user merge of this delivery.

#### Scenario: A project builds against its separately granted read-only dependency
- **WHEN** the Process holds the selected project and a live separate read-only dependency grant
- **THEN** the enforcing native build can read that dependency through supported relative or absolute native manifest paths and complete its tests
- **AND** native writes to the dependency, reads/writes in another writable project and projection of another Process's grant remain denied

#### Scenario: An external dependency grant is revoked after successful compilation
- **WHEN** the Process revokes its read-only dependency grant and requests the next build with retained successful output
- **THEN** launch reconciles current authority and cannot read the revoked dependency
- **AND** the cached earlier success does not become a new successful execution

#### Scenario: A read-only dependency source escapes through a symlink
- **WHEN** a live read-only dependency resolves a source outside its granted backing
- **THEN** structured access rejects the escaped path and enforcing native compilation fails without consuming the outside source
- **AND** no extra root or writable authority is inferred from the dependency manifest or symlink

#### Scenario: A stale or unknown cwd cannot select another writable grant
- **WHEN** a non-root requested cwd is outside every live same-Process Host Mount
- **THEN** Host reconciliation refuses and requires an explicit directory selection
- **AND** an unrelated live writable grant is not silently substituted for that cwd

#### Scenario: An approved backing root is retargeted or unavailable
- **WHEN** a live grant's approved canonical backing root is missing, is no longer a directory, resolves to a different root or names a replacement directory instead of the retained file-tree root before Tool adapter construction
- **THEN** the native Host adapter refuses projection before deriving sandbox authority
- **AND** the failure identifies the public namespace without exposing private backing paths or granting the new target
