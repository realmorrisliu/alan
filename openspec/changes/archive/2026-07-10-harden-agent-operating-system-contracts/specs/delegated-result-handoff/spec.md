## ADDED Requirements

### Requirement: Authorized Delegated Output References
Delegated `output_ref` values SHALL resolve through a runtime-authorized artifact reader instead of requiring the parent to read a raw child workspace path.

#### Scenario: Parent inspects long child output
- **WHEN** a delegated result includes `output_ref` for long child output
- **THEN** the parent runtime can request that output through an authorized runtime artifact surface tied to the parent session and child run

#### Scenario: Parent lacks filesystem access to child workspace
- **WHEN** the parent workspace guard would reject direct reads of the child rollout path
- **THEN** the authorized artifact reader still returns permitted child output content or a structured policy denial without asking the parent to bypass workspace isolation

#### Scenario: Output reference cannot be resolved
- **WHEN** an `output_ref` points to missing, expired, or denied child output
- **THEN** the delegated result inspection path returns a structured error that preserves the original preview, child-run metadata, and failure reason

### Requirement: Delegated Result Evidence Metadata
Delegated result handoffs SHALL identify whether inline content is complete, preview-only, or backed by a durable evidence artifact.

#### Scenario: Child output is preview-only
- **WHEN** delegated child output is too long to inline in the parent result
- **THEN** the result marks the inline text as preview-only and includes evidence metadata with original size, digest when available, and inspection reference

#### Scenario: Child output is fully inline
- **WHEN** delegated child output fits inline budgets and is included as `output_text`
- **THEN** the result marks truncation as absent so the parent can treat the inline content as complete
