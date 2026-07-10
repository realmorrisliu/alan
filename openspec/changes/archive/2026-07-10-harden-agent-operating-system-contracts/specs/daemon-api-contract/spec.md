## ADDED Requirements

### Requirement: Evidence Artifact Endpoint Metadata
The daemon endpoint registry SHALL cover evidence-artifact read surfaces introduced for runtime-owned tool and delegated-child evidence.

#### Scenario: Evidence endpoint is registered
- **WHEN** the daemon exposes an endpoint that reads, previews, lists, or describes evidence artifacts
- **THEN** the canonical endpoint registry includes method, route pattern, scope metadata, relay policy, and response URL metadata when applicable

#### Scenario: Evidence read uses session scope
- **WHEN** a client requests evidence for a session, run, tool call, or child run
- **THEN** remote-control and relay layers derive authorization from endpoint metadata and runtime ownership rather than raw path prefixes

### Requirement: Run Lifecycle Endpoint Metadata
The daemon endpoint registry SHALL cover user-visible run lifecycle, approval, resume, and child-progress surfaces.

#### Scenario: Run state includes resumed execution
- **WHEN** an approval checkpoint is approved and execution resumes
- **THEN** session/read/list or run-state surfaces expose a running or resuming state before terminal completion

#### Scenario: Child progress is readable through daemon APIs
- **WHEN** a parent session has active delegated child work
- **THEN** daemon read/list surfaces expose child lifecycle status and progress metadata through registered endpoints or registered response fields
