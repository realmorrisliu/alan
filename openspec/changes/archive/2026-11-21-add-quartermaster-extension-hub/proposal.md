# Proposal

## Why

Extensions are scattered: Skills ship through the Package Service, Tools are
embedded in the binary, and provider protocols are compiled in. ADR-0061 makes
Quartermaster `q` v1 the single extension hub so users add capability in one
governed place without growing the first-party surface or granting authority by
installation. This is milestone M3; it also gives the demoted Anthropic protocol and
future providers a home.

## What Changes

- Package kinds `skill`, `mcp`, `tool` and `provider` share one install, catalog,
  lifecycle and projection path owned by the Package Service.
- `mcp` packages run their servers as ordinary governed Processes; `tool` packages
  install reusable executables into the alan9 command namespace; `provider`
  packages register protocol adapters bound to Connection profiles.
- Installation never grants authority, credentials or mounts.
- `q` remains the user surface; no Host command plane or second package authority.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `package-management-contract`: One extension hub with four package kinds and no installation-time authority.

## Impact

Package Service remains the owner of durable package state in its System Store
subtree. Agent Runtime governance, Host Mount Service grants and Connection Service
profiles are unchanged owners. Depends on the M1 provider set (ADR-0061). Only the
`skill` kind is shipped today; this change claims no other kind until its tasks
pass.
