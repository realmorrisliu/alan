## Purpose

Define how one Agent Machine selects deterministic, typed evaluation and
generation work while preserving ordinary Process authority and durable evidence.

## ADDED Requirements

### Requirement: Cognitive modes do not define Process or permission types
Agent Machine SHALL select deterministic, evaluation or generation operations
according to explicit input, available capabilities and bounded policy. It
SHALL NOT require a child Process for every decision or grant permissions
because a model is designated System 1 or System 2.

#### Scenario: Deterministic task completes
- **WHEN** existing deterministic rules can complete authorized work
- **THEN** the Machine completes without a model call
- **AND** no final natural-language generation is required

#### Scenario: Isolation is needed
- **WHEN** a step requires an independently bounded lifecycle or authority
- **THEN** the existing Process launch path supplies that boundary
- **AND** the child's capabilities do not exceed its authorized launch context

### Requirement: Evaluation is typed advice from an available Connection
A Machine SHALL use only reachable Connections supporting the requested
operation. Evaluation SHALL return a versioned typed result or a typed failure,
not assistant prose. Candidate capabilities SHALL be resolved for that Process
before evaluation, including Skill availability and implicit-exposure rules.

#### Scenario: Evaluation selects a candidate
- **WHEN** a valid result names a member of the submitted candidate set
- **THEN** the Machine records the selected candidate and result provenance
- **AND** the choice does not grant authority to execute it

#### Scenario: Candidate is outside the submitted set
- **WHEN** evaluation names an unavailable or unknown candidate
- **THEN** the Machine rejects that selection
- **AND** it does not resolve a global catalog entry to bypass the candidate view

### Requirement: Fallback and interruption are bounded
The Machine SHALL preserve explicit input intent and bound evaluation retries
and generation fallback by its declared budget. Unavailable or uncertain
evaluation SHALL NOT authorize an effect.

#### Scenario: Evaluation has no match
- **WHEN** evaluation returns no-match and generation is available within budget
- **THEN** the Machine may request generation with recorded fallback provenance
- **AND** generation receives no additional authority from that escalation

#### Scenario: Budget is exhausted or user cancels
- **WHEN** no allowed attempt remains or cancellation is accepted
- **THEN** no further model or effect dispatch starts for that work
- **AND** a typed terminal or waiting outcome explains the boundary

### Requirement: Decisions use existing Machine and execution evidence owners
Machine state SHALL own pending decisions and wait state, AgentFS SHALL project
that state, and durable rollout/checkpoint evidence SHALL retain recoverable
operation identity, input reference, model/schema version, result and effect
linkage. Model-input projections SHALL NOT be mistaken for complete checkpoints.

#### Scenario: Work completes without prose
- **WHEN** the authorized task produces only a structured result
- **THEN** work can complete without manufacturing an assistant apology
- **AND** Process exit is governed separately from work completion

#### Scenario: External effect outcome is unknown after restart
- **WHEN** recovery sees a dispatched effect with unknown completion
- **THEN** normal effect reconciliation or confirmation is required
- **AND** replaying a decision does not re-execute that effect

### Requirement: Agent effects retain existing authorization
Every Agent-originated effect SHALL pass existing governance and capability
checks regardless of whether code, evaluation or generation proposed it.
Ordinary human Shell commands SHALL NOT acquire a universal model-review gate.

#### Scenario: High confidence requests unavailable authority
- **WHEN** a model result recommends an action requiring absent rights or mounts
- **THEN** the action remains unavailable or follows explicit grant approval
- **AND** confidence does not expand authority
