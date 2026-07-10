## MODIFIED Requirements

### Requirement: Failed Child Handoff Metadata
The system SHALL include child-run metadata and latest progress information when a child fails, pauses, is cancelled, is terminated, or times out. References to the child's persisted state SHALL be namespace paths (the child's home tree or the parent-side action record), not raw host filesystem rollout paths.

#### Scenario: Child times out
- **WHEN** a delegated child reaches idle timeout
- **THEN** the delegated result includes `error_kind`, `error_message`, child-run reference, a namespace-path reference to the child's persisted state when available, latest heartbeat/progress metadata, and terminal status `timed_out`

#### Scenario: Child is explicitly terminated
- **WHEN** a delegated child is terminated by operator or parent request
- **THEN** the delegated result distinguishes `terminated` from `timed_out` and includes termination actor, reason, and mode when available
