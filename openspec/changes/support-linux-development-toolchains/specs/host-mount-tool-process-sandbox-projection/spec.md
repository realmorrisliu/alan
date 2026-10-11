## ADDED Requirements

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
