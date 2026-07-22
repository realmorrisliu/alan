## REMOVED Requirements

### Requirement: Host mount declarations project into SandboxSpec
**Reason**: This requirement assigned native sandbox-root derivation to Alan OS
from Process launch-context records. Host adapters now exclusively derive
per-Tool-Process native authority from explicitly delegated service-owned Host
Mount grants.

**Migration**: Use `host-mount-tool-process-sandbox-projection`; namespace
declarations remain Alan OS-visible authority and never reveal or reconstruct
native Host backing.
