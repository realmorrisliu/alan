# Spec Delta

## MODIFIED Requirements

### Requirement: Mount authorization cannot be auto-allowed
Every valid mount request SHALL cross Host Mount Service and a Host-adapter authorization boundary. A previously explicitly saved user authorization MAY satisfy this boundary only after the owning service revalidates identity, scope and revocation. Agent policy or reviewer allow SHALL only permit request creation, never create or broaden a grant. Pending service requests SHALL preserve existing correlated wait and terminal-state semantics.

#### Scenario: Valid saved delegation
- **WHEN** the Host adapter and service revalidate an exact saved scope for a request
- **THEN** the request can settle without another user prompt and records that authorization provenance

#### Scenario: Policy allow has no grant
- **WHEN** an Agent policy or reviewer returns allow without valid saved or new user authorization
- **THEN** only a pending request is created and access is not granted

#### Scenario: Agent spoofs approval
- **WHEN** an Agent writes an approve-like value to its own files
- **THEN** service grant authority remains unchanged
