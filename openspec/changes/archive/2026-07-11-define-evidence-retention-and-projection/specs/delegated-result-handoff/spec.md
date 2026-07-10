## MODIFIED Requirements

### Requirement: Completed Child Output Fidelity
The system SHALL preserve completed delegated child output as full inline text or an inspectable output reference instead of silently replacing it with an unlabeled short preview. Output references SHALL be namespace paths readable in the parent's namespace, never raw host filesystem paths.

#### Scenario: Child returns short output
- **WHEN** a completed delegated child returns output that fits the parent tape budget
- **THEN** the delegated result includes the full `output_text` and does not mark it as truncated

#### Scenario: Child returns long output
- **WHEN** a completed delegated child returns output that exceeds the parent tape budget
- **THEN** the delegated result includes a bounded preview, an `output_ref` namespace path resolvable in the parent's namespace for the full text, and truncation metadata that states what was omitted

#### Scenario: Parent resolves an output reference
- **WHEN** the parent runtime resolves a delegated `output_ref`
- **THEN** it walks the namespace path like any other file read, with no dedicated artifact-read API and no reliance on raw child workspace paths
- **AND** a missing or retention-expired reference returns a structured error preserving the original preview and child-run metadata

### Requirement: Delegated Result Shape
The delegated result payload SHALL distinguish summary, preview, full output, child-run reference, structured output, and truncation metadata. Raw host rollout paths MAY appear only as optional debug metadata, never as the resolution mechanism for `output_ref`.

#### Scenario: Completed child result is persisted
- **WHEN** a completed child result is recorded in the parent tape or rollout
- **THEN** the result contains `status`, `summary`, `child_run`, and either `output_text` or a namespace-path `output_ref`

#### Scenario: Summary is shortened
- **WHEN** the result summary is shortened for tape compactness
- **THEN** the result includes `summary_preview` and truncation metadata so the parent can tell the summary was intentionally shortened

#### Scenario: Structured output is large
- **WHEN** structured output exceeds the inline budget
- **THEN** the result preserves critical keys such as `status` and `summary`, includes a namespace-path structured output reference or bounded structured preview, and records explicit truncation metadata
