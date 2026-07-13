## Why

After extraction, Alan OS Host still temporarily constructs and supervises
internal services and Root Agent directly. A real Service Manager Process is
required so boot, readiness, restart, entry, Host Mount, and Connection
lifecycle live inside Alan OS rather than in a second Host-side manager.

## What Changes

- Add Service Manager as the first system Process and make Alan OS Host start
  only Kernel plus Service Manager.
- Load a minimal, read-only `/lib/boot` tree installed by system packages; units
  declare executable, descriptors/mounts, ordering, restart policy, and
  published handles without arbitrary scripts or a general unit language.
- Determine service readiness exclusively from `/proc` liveness and declared
  `/srv` handle publication; expose manager-owned unit status and `ctl` files.
- Implement bounded restart budgets, exponential backoff, required boot
  failure, post-boot degraded state, and explicit retry.
- Boot Agent Runtime Service and Root Agent from units and keep `/agent/root`
  stable across Root Agent Process replacement.
- Add Local Entry Service to create ordinary Shell Processes from a Login
  Namespace Template and hand their namespaces to authorized local renderers.
- Add Host Mount Service as the authority for requests, grants, hostfs exports,
  revocation, namespace projection, and native sandbox derivation.
- Add Connection Service as owner of profile metadata and callable LLM trees;
  Host adapters retain secrets and native login.
- Remove the temporary fixed boot composition from Alan OS Host completely.

## Capabilities

### New Capabilities

- `service-manager`: Boot Units, ordering, readiness, supervision, status,
  degraded state, and Root Agent boot ownership.
- `local-entry-service`: Shell Process creation and namespace handoff without
  Session identity.
- `host-mount-service`: Host Mount request/grant/export/revocation authority.
- `connection-service`: Service-owned LLM profile metadata and Host credential
  adapter integration.

### Modified Capabilities

- `plan9-kernel-substrate`: Boot the first real system Process and support its
  service supervision responsibilities.
- `agent-root-layout-contract`: Make `/agent/root` a Service-Manager-supervised
  role path backed by a system Agent Definition.
- `provider-connection-contract`: Move profile/default/selection authority to
  Connection Service and keep only secret material behind Host adapters.
- `host-directory-mounts`: Route authorization and live namespace projection
  through Host Mount Service.
- `alan-shell`: Run every interactive Shell as an ordinary Process with its own
  credentials, namespace, descriptors, cwd, and child lineage.

## Impact

Touches Kernel bootstrap, `/proc`, `/srv`, AgentFS root binding, system
executables, boot package data, hostfs, connection/LLMFS integration, sandbox
derivation, local attachment handoff, and failure tests. Depends on
`extract-system-level-alan-os-host`; unblocks macOS attachment and the rewrite
of package management.
