# Spec Delta

## ADDED Requirements

### Requirement: Project recovery selects evidence and revalidates execution authority
Cross-invocation project recovery SHALL require explicit user selection and use existing durable recovery owners to create invocation-local execution. It SHALL validate current grants and retained budget consumption, preserve paused or unresolved work and never automatically replay completed effects. Root identity, trust activation and authority SHALL NOT be restored from a summary or old Process reference.

#### Scenario: Completed effect exists
- **WHEN** a user resumes a task containing a completed write
- **THEN** the prior write remains evidence and is not executed again

#### Scenario: Grant was revoked
- **WHEN** the chosen history references revoked project authority
- **THEN** recovery does not revive that authority and pauses work needing a new grant

#### Scenario: Summary is read
- **WHEN** an Agent reads a continuity summary without explicit recovery selection
- **THEN** no prior pending operation is launched solely from that summary
