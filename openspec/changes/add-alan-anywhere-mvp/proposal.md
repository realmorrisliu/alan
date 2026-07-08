## Why

alan Anywhere should let a signed-in user enter their own Alan OS from another
owned device without learning VPN, tunnels, public IPs, router configuration,
SSH, port forwarding, daemon URLs, or relay nodes. The product path is
OS-native remote attachment: product account/device infrastructure gets the
client to `Remote Access Service`, then Alan OS hands back a real
`Remote Entry Process` namespace.

## What Changes

- Add alan Anywhere as an account-bound, zero-configuration way to continue
  Alan OS from a user's own Mac on iPhone.
- Have alan Desktop automatically register and advertise the Mac as an online,
  trusted execution device after account login.
- Have alan iPhone automatically discover the user's online Macs, connect to a
  selected Mac, enter through `Remote Access Service`, interact with the
  resulting `Remote Entry Process` namespace, and recover through explicit lease
  reattachment after reconnect.
- Introduce product-level device availability instead of exposing relay nodes,
  tunnel URLs, daemon URLs, public IPs, or router concepts; work discovery after
  attach belongs to the returned remote namespace.
- Preserve the invariant that process execution, agent execution, tool
  execution, governance checks, namespace access, and stream ordering remain
  authoritative on the user's Mac.
- Add device binding, remote entry tickets, revocation, and encrypted transport
  requirements for alan Anywhere access.
- Fold the current open remote-control architecture issue into this product
  contract while keeping the iOS task-manager issue as a follow-up UI framing
  track.

## Capabilities

### New Capabilities

- `alan-anywhere`: Defines alan account-bound device discovery, automatic Mac
  availability, iPhone remote entry, namespace-backed interaction, realtime
  stream flow, reconnect recovery, and security boundaries for the MVP.

### Modified Capabilities

- `remote-control-contract`: frozen as legacy by `define-remote-access-service`
  (ADR-0028 D11); this change deletes its earlier `daemon-api-contract` delta
  and MUST NOT extend any daemon API compatibility contract.

### Dependencies

- `remote-access-service` (owned by `define-remote-access-service`): the OS
  entry contract this product change consumes — Remote Access Service,
  bootstrap tree, handoff, `Remote Entry Process`, leases, revocation. This
  change owns only the product plane: accounts, device enrollment, presence,
  relay brokerage, tickets, and the iPhone experience.

## Impact

- alan Desktop/macOS account login, device enrollment, Keychain-backed device
  credentials, and automatic outbound relay connection.
- alan iPhone account login, device discovery, remote entry selection, and
  namespace-backed interaction after handoff.
- alan Cloud/App Server account, device registry, presence, relay broker, token
  issuance, revocation, and audit surfaces.
- Daemon/HTTP/WebSocket remote compatibility is not a migration surface. The
  remote path is `Remote Product Control Plane` plus Alan OS
  `Remote Access Service` and aP/file-surface interaction.
- Existing GitHub issue tracking for remote access: close or supersede `#9`
  with this OpenSpec-backed product issue; keep `#75` open as iOS IA follow-up
  unless it is rewritten to depend on this change.
