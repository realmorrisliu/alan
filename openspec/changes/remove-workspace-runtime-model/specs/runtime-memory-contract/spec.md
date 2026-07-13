## ADDED Requirements

### Requirement: Memory authority is not a workspace directory
Runtime memory SHALL be accessed through explicit Memory Store file trees or
descriptors and persisted by their owning service backing. Agent Process boot,
recall, flush, and promotion MUST NOT infer `<host-dir>/.alan` memory paths.

#### Scenario: Agent receives a Memory Store
- **WHEN** an Agent Process is launched with a Memory Store descriptor
- **THEN** recall and writes use that tree
- **AND** Host cwd contributes no implicit memory authority
