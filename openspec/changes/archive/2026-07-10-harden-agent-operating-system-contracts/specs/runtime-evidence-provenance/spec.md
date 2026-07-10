## ADDED Requirements

### Requirement: Durable Evidence Artifacts
The runtime SHALL preserve answer-supporting tool and delegated-child outputs as durable evidence artifacts when those outputs exceed prompt-facing or rollout-inline budgets.

#### Scenario: Long tool output supports final answer
- **WHEN** a tool returns output that exceeds inline durable rollout limits and the subsequent answer relies on that output
- **THEN** the runtime stores a redacted evidence artifact with artifact id, source tool call id, digest, original size metadata, truncation metadata, and retrieval scope

#### Scenario: Long child output supports final answer
- **WHEN** a delegated child returns output that exceeds the parent inline budget
- **THEN** the runtime stores or references the full redacted child output as an evidence artifact associated with the parent session and child run

### Requirement: Prompt Projection Does Not Replace Evidence
The runtime SHALL separate model-facing truncation from durable evidence preservation.

#### Scenario: Prompt receives preview
- **WHEN** a tool or child output is too large for the parent model context
- **THEN** the model-facing payload includes a bounded preview and evidence reference, while the durable evidence artifact remains available for authorized inspection

#### Scenario: Rollout stores truncated value
- **WHEN** rollout persistence stores a truncated preview for safety or size
- **THEN** the rollout also records evidence metadata sufficient to verify that full redacted evidence was captured or that capture was intentionally unavailable

### Requirement: Evidence Access Authorization
The runtime SHALL authorize evidence reads by runtime ownership and policy instead of relying on raw filesystem path access.

#### Scenario: Parent reads child evidence
- **WHEN** a parent session requests evidence produced by one of its child runs
- **THEN** the runtime validates the parent-child relationship and returns bounded content or a structured denial

#### Scenario: Unrelated session requests evidence
- **WHEN** a session requests evidence that is not owned by the session, its child run, or an authorized workspace scope
- **THEN** the runtime denies the request without revealing the artifact contents or raw storage path

### Requirement: Answer Evidence Summary
The runtime SHALL make final-answer provenance inspectable for turns that rely on tool or child evidence.

#### Scenario: Final answer uses evidence
- **WHEN** alan produces a final answer after reading tool, GitHub, child, or local workspace evidence
- **THEN** the rollout or session metadata records a bounded evidence summary linking the answer to the relevant tool calls, child runs, or evidence artifacts

#### Scenario: Evidence is incomplete
- **WHEN** alan produces an answer after an evidence source failed, was unavailable, or was only partially inspected
- **THEN** the evidence summary records the limitation so a reviewer can distinguish verified facts from fallback assumptions
