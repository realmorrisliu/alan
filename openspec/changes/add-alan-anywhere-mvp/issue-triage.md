## OpenSpec

- Tracking issue: `#349`
- Change: `add-alan-anywhere-mvp`
- Proposal: `openspec/changes/add-alan-anywhere-mvp/proposal.md`
- Design: `openspec/changes/add-alan-anywhere-mvp/design.md`
- Requirements:
  - `openspec/changes/add-alan-anywhere-mvp/specs/alan-anywhere/spec.md`
- Tasks: `openspec/changes/add-alan-anywhere-mvp/tasks.md`

## Summary

Define and implement Alan Anywhere MVP: a signed-in Mac automatically
becomes remotely connectable, and a signed-in iPhone using the same account can
discover that Mac, enter through Remote Access Service, interact with the
returned remote namespace, stream file records, write process and agent files,
and recover through lease reattachment without requiring public IPs, router
setup, VPNs, tunnels, SSH, daemon URLs, or port forwarding.

The user-facing product framing is:

> alan, anywhere you need to continue.

Not:

> Remote desktop, LAN tunnel, or network configuration.

## Scope

- Account-bound Mac/iPhone device enrollment.
- Automatic Mac remote availability over outbound encrypted relay.
- iPhone owned-device discovery and remote entry.
- Realtime remote stream delivery plus lease reattach, stream-offset recovery,
  and file rereads after gaps.
- Remote namespace interaction for messages, interrupts, and pending responses.
- Device-bound, short-lived remote entry tickets and revocation.
- Mac-authoritative namespace, process, governance, tool execution, and stream
  ordering.

## Non-goals

- Remote desktop or screen sharing.
- P2P hole punching.
- LAN discovery.
- Multi-user collaboration.
- Enterprise networking/MDM policy.
- Cloud-side agent/tool execution.

## Issue Cleanup

- Close `#9` as superseded by `#349`, this OpenSpec-backed Alan Anywhere MVP issue.
  The lower-level Agent Node / Relay / Client architecture remains the
  transport foundation, but this issue becomes the product contract.
- Keep `#75` open as the iOS task-manager/product IA follow-up. It should
  depend on this MVP rather than replace it.
- Leave `#305` unchanged; it is unrelated to remote access.
- Leave closed phase issues `#32`, `#33`, `#34`, and `#35` closed; their
  completed direct/relay/multi-node/reliability work becomes prior foundation.

## Verification

- `openspec validate add-alan-anywhere-mvp --type change --strict --json`
- `openspec validate --all --strict --json`
- `git diff --check`
