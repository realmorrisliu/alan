# evidence-retention-and-projection Specification

## Purpose
Define how oversized tool and delegated-child outputs are projected into the
tape (bounded preview plus namespace-path reference plus truncation metadata),
how referenced evidence stays readable after the producing process exits under
the storing file server's retention, and how redaction is marked distinctly
from truncation in durable evidence.
## Requirements
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

#### Scenario: Original ranges fit the prompt-facing budget
- **WHEN** ReadFile follows its concrete own-Agent Action output path using byte_offset and byte_limit
- **THEN** each read acquires at most 4096 original bytes and reports total_bytes and the next UTF-8-safe byte offset
- **AND** worst-case JSON escaping still leaves the ordinary Tool result below the inline budget
- **AND** Host files retain line offset/limit behavior; mixed range modes and unsupported paths do not silently fall back to Host access

#### Scenario: An old range refers to expired evidence
- **WHEN** the storing service replaces an original output with its structured expiry record
- **THEN** reading an obsolete byte offset returns that expiry record truthfully
- **AND** it does not report successful acquisition of the previous original range

### Requirement: Referenced Evidence Outlives The Producing Process
Content referenced from a durable tape SHALL remain resolvable after the
producing tool or child process exits, for at least as long as the citing
tape's own retention, under the retention policy of the storing file server.
An expired reference SHALL resolve to a structured retention-expiry error, not
to silently missing or shifted content.

#### Scenario: Tool process exits before inspection
- **WHEN** a consumer resolves an `actions/<id>/output` reference after the tool
  process has exited
- **THEN** the full output content is returned from the action record's backing
  storage

#### Scenario: Retention expires a reference
- **WHEN** a reference's backing content has been garbage-collected past
  retention while the citing tape record remains
- **THEN** resolution returns a structured expiry error identifying the
  reference and retention cause

#### Scenario: Streams referenced by offset do not shift
- **WHEN** a durable tape references a stream by offset
- **THEN** the stream's retained prefix is never rewritten in place, so the
  offset either resolves to the original content or reports expiry

### Requirement: Durable Evidence Is Redacted With Marked Spans
The runtime SHALL redact secret material before durable evidence persistence
and SHALL mark redacted spans with an explicit marker and reason class,
distinguishable from size truncation.

#### Scenario: Secret appears in tool output
- **WHEN** tool output containing secret material is persisted durably
- **THEN** the persisted content carries a redaction marker in place of the
  secret, with a reason class

#### Scenario: Auditor distinguishes redaction from truncation
- **WHEN** an auditor inspects a persisted evidence record with both size
  truncation and redaction
- **THEN** truncation metadata points to recoverable full content while
  redaction markers state the spans are not recoverable
