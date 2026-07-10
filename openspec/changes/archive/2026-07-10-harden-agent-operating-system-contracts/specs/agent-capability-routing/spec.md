## ADDED Requirements

### Requirement: Task Capability Classification
The runtime SHALL classify the material capabilities required by a task before launching delegated work when the task depends on a specific workspace, external service, network source, shell command, browser surface, side effect, or evidence artifact.

#### Scenario: GitHub issue review requires GitHub access
- **WHEN** a user asks alan to inspect and review a GitHub issue in another repository
- **THEN** the runtime records that the task requires target-workspace read access and GitHub or network access before choosing a delegated target

#### Scenario: Task only needs local workspace inspection
- **WHEN** a user asks alan to inspect local files in another workspace without external state
- **THEN** the runtime MAY classify a read-only workspace inspection target as eligible if that target advertises matching workspace-read capability

### Requirement: Delegation Eligibility
The runtime SHALL launch a delegated child target only when the target's advertised capabilities satisfy the classified task requirements or when the delegated task is explicitly narrowed to a supported fallback.

#### Scenario: Child target lacks required capability
- **WHEN** a delegated target lacks GitHub, network, shell, browser, write, or other required capability for the classified task
- **THEN** the runtime does not launch that child for the original task and records a capability-mismatch decision with the missing capabilities

#### Scenario: Fallback task is explicitly narrowed
- **WHEN** the original task requires GitHub access but the parent narrows a child task to local repository inspection only
- **THEN** the child task description MUST state that GitHub is not required and the parent remains responsible for obtaining or acknowledging the missing GitHub issue content

### Requirement: Capability Mismatch Recovery
The runtime SHALL provide a visible recovery decision when no eligible delegated target can satisfy the task requirements.

#### Scenario: Direct parent tool can satisfy missing capability
- **WHEN** no delegated target can inspect GitHub but the parent runtime has an authorized GitHub-capable tool path
- **THEN** alan may use the parent tool path and records that the parent recovered from delegated capability mismatch

#### Scenario: No available capability can satisfy task
- **WHEN** neither parent tools nor delegated targets can satisfy a required capability
- **THEN** alan asks for the missing input or returns a limitation-focused answer instead of substituting unrelated local context

### Requirement: Capability Decision Observability
The runtime SHALL expose capability-routing decisions in rollout or event metadata so the execution path can be audited after the turn.

#### Scenario: Delegation is accepted
- **WHEN** alan launches a delegated child after capability matching
- **THEN** the child-run metadata includes the required capabilities, selected target capabilities, and the decision reason

#### Scenario: Delegation is rejected
- **WHEN** alan declines a delegated target because of capability mismatch
- **THEN** the session timeline or rollout records the rejected target, missing capabilities, and selected recovery path
