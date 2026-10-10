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
