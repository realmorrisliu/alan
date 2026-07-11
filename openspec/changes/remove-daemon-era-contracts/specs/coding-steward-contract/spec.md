## REMOVED Requirements

### Requirement: Coding handle profiles are explicit
**Reason**: The existing handle profile carries parent Session identity.
**Migration**: Use parent Agent Process path, bounded repo descriptors, worker executable, policy, and rollout/checkpoint evidence.

### Requirement: Coding execution is recoverable and fail-safe
**Reason**: Recovery is coupled to a Session-recovery path.
**Migration**: Recover from Process state, worker/steward files, checkpoints, rollouts, and durable repo evidence.

## ADDED Requirements

### Requirement: Coding handles identify parent and worker Processes
Coding steward handles SHALL identify the parent Agent Process, worker Agent Process or executable,
bounded repository descriptors, namespace/policy inputs, and rollout/checkpoint evidence. They
SHALL use those concrete owners as their complete identity and authority boundary.

#### Scenario: A steward delegates repository work
- **WHEN** the steward spawns a bounded repo worker
- **THEN** the handle links the worker to the parent Agent Process and delegated repo resources
- **AND** Process, namespace, policy, and evidence files are sufficient to inspect and govern it

### Requirement: Coding recovery is file and Process based
Coding execution SHALL recover from authoritative Process state, steward/worker files, durable
checkpoints, rollouts, and repository evidence. It SHALL fail closed when those owners cannot prove
continuity.

#### Scenario: A worker disappears during coding work
- **WHEN** the worker Process exits or becomes unavailable before handoff
- **THEN** the steward reconstructs status from Process and durable evidence
- **AND** it resumes work only when those owners prove a safe continuation point
