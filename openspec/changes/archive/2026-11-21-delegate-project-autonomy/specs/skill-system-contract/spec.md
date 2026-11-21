## ADDED Requirements

### Requirement: Delegated skill calls select explicit Host Mount authority
The `invoke_delegated_skill` Tool SHALL accept bounded explicit `host_mounts`
selections using the existing child Process launch contract: parent-held grant
selector, absolute target namespace path, and read-only or read-write access.
It SHALL forward those selections to Host Mount Service for current ownership,
revocation and equal-or-narrower access validation. Grant identifiers alone SHALL
NOT authorize access. Omitted selections SHALL remain empty; cwd selection SHALL
NOT implicitly project parent grants. Omitted cwd SHALL resolve to the parent
Process cwd before capability classification. The retained invocation evidence
SHALL include the explicit mount selections used for the launch.

#### Scenario: Read-only project delegation
- **WHEN** a parent selects a held read-only project grant and a child target path
- **THEN** the Tool passes that exact selection through the child launch contract
- **AND** Host Mount Service projects only the selected authority after validation
- **AND** the retained invocation records the grant selector, target and access

#### Scenario: Missing project authority
- **WHEN** a delegated call omits cwd and project grant selections
- **THEN** capability requirements name the parent's current directory
- **AND** unrelated service mounts cannot satisfy that directory's read requirement
- **AND** no parent Host Mount is automatically inherited

#### Scenario: Invalid or excessive mount selections
- **WHEN** a delegated call supplies more than sixteen selections, a relative or parent-traversing target, an empty grant selector, or an unsupported access mode
- **THEN** the Tool rejects the invocation before launching a child

#### Scenario: Revoked or widened authority
- **WHEN** a child launch selects a revoked grant, another Process's grant, or access wider than the parent holds
- **THEN** Host Mount Service rejects the projection
- **AND** the Tool cannot override that decision
