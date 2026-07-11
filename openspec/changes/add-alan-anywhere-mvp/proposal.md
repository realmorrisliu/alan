## Why

Alan Anywhere should let a user enter Alan on an owned Mac from iPhone without
having to understand network topology. The product plane needs account identity,
device enrollment, availability, short-lived entry authorization, and a native
mobile experience above the file-native `remote-access-service` contract.

## What Changes

- Add Alan account sign-in and explicit owned-device enrollment.
- Show product-facing device availability without exposing transport or routing
  infrastructure.
- Issue short-lived, device-bound Remote Entry Tickets after user intent and
  destination authorization are verified.
- Enter a general Remote Entry Process and discover files, Processes, Agent
  Processes, and apps only after handoff.
- Preserve continuity through service-owned attachment leases and stream
  offsets.
- Keep Alan Cloud limited to account, device directory, presence, ticket, and
  byte-delivery coordination; destination-host Process and policy authority do
  not move to the cloud.
- Build the initial iPhone experience around device choice, entry progress,
  terminal/file interaction, pending requests, and explicit disconnection.

## Capabilities

### New Capabilities

- `alan-anywhere`: Product requirements for accounts, device enrollment,
  availability, Remote Entry Tickets, iPhone entry, continuity, and cloud trust
  boundaries.

### Modified Capabilities

None.

## Dependencies

- `remote-access-service`, owned by `remove-daemon-era-contracts`, defines the
  OS entry Process, handoff, lease, revocation, and remote context semantics.
- Alan for macOS integration with local Alan OS remains a separate design; this
  change assumes only the accepted Remote Access Service host boundary.

## Impact

- Alan Cloud gains account/device/presence/ticket product services.
- Alan for macOS gains enrollment, availability, and Remote Access Service host
  integration in later implementation slices.
- Alan for iPhone gains the first remote-entry client.
- No transport implementation is selected by this product contract.
