## REMOVED Requirements

### Requirement: Canonical Endpoint Registry
**Reason**: Alan daemon HTTP/WebSocket endpoints are removed as a product and runtime boundary.
**Migration**: None. Clients use the canonical Process and mounted file surfaces defined by their owning capabilities.

### Requirement: Shared URL Construction
**Reason**: Session response URLs disappear with the daemon API.
**Migration**: None.

### Requirement: Remote Access Scope Metadata
**Reason**: Remote authority belongs to Remote Access Service, credentials, namespaces, and leases rather than endpoint metadata.
**Migration**: Use `remote-access-service`.

### Requirement: Relay Policy Metadata
**Reason**: The daemon relay and its forwarding policy are removed.
**Migration**: None; future remote byte transport remains subordinate to `remote-access-service`.

### Requirement: Generated Client Endpoint Helpers
**Reason**: No shipped daemon endpoint client remains.
**Migration**: None.

### Requirement: Protocol And Payload Drift Checks
**Reason**: Daemon payload drift is no longer a live compatibility concern.
**Migration**: Keep schema and file-surface checks with the capabilities that own the surviving Agent Execution Engine alphabet and file records.

### Requirement: Public Route Compatibility
**Reason**: Public daemon routes are deliberately removed without a compatibility window.
**Migration**: None.

### Requirement: Session route semantics match runtime protocol mapping
**Reason**: Session routes and their runtime protocol mapping are removed together.
**Migration**: Use Agent Process IO, control, Agent Machine, request, action, rollout, and checkpoint files.

### Requirement: Raw Route String Guardrail
**Reason**: There are no canonical daemon route strings to preserve.
**Migration**: Replace the route allowlist with the daemon-era absence guard owned by `documentation-governance`.

### Requirement: Rust TUI client preserves session API compatibility
**Reason**: The Rust TUI is file-backed and SHALL NOT preserve a Session API compatibility path.
**Migration**: Use `rust-inline-tui`, `alan-renderer-host-contract`, and `agent-runtime-ui-file-surfaces`.
