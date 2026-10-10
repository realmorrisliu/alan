## MODIFIED Requirements

### Requirement: Oversized Outputs Project As Preview Plus Namespace Reference
The runtime SHALL project any tool or delegated-child output that exceeds the
prompt-facing budget as a tape record carrying a bounded preview, a
namespace-path reference to the full content, and truncation metadata stating
what was omitted. References
SHALL be namespace paths (such as `actions/<id>/output` or a child's
`io/output`, optionally with offset/length), NOT raw host filesystem paths and
NOT identifiers of a separate artifact-read API.

#### Scenario: Long tool output is projected
- **WHEN** a tool effect produces output exceeding the prompt-facing budget
- **THEN** the tape record contains a bounded preview, a namespace path to the
  action's full output, and truncation metadata

#### Scenario: Reference is resolvable by the reader that receives it
- **WHEN** the runtime emits a projection reference into a tape
- **THEN** the reference resolves in the namespace of the agent whose tape it is
- **AND** if no resolvable path exists, the record keeps the inline preview as
  the declared-complete record with the omission explicitly marked

#### Scenario: Real Agent reads its own retained Action output
- **WHEN** a real Agent follows a published reference to its own retained Action output after the producer exits
- **THEN** ordinary Tool execution can acquire bounded original ranges through that Agent's namespace descriptors within the prompt budget
- **AND** it does not treat AgentFS evidence as a Host project path or require another Host grant or producer execution

#### Scenario: Evidence read cannot widen Process authority
- **WHEN** a read requests another Agent owner, traversal/control paths, unavailable evidence or an expired range
- **THEN** it refuses unsupported authority or returns truthful missing/expiry evidence without a Host fallback
- **AND** no evidence read creates write authority or exposes private backing paths
