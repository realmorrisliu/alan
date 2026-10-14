# Proposal

## Why
The live prompt reads like a diagnostic log: role qualifiers, idle status and zero queue counts obscure project and model context. The user-approved separator preview makes the boundary clearer but requires the same information hierarchy inside it.

## What Changes
- Keep a blank row and a subtle rule above the flowing live composer.
- Show project name, read-only limitation, and effective model/effort by default; omit idle/zero queue and active/next qualifiers.
- Show running, pending, paused, failed and uncertain states only when applicable; preserve actionable warnings and exact model/queue details through /status.
- Preserve completion anchoring, input routes, scrollback and runtime authority.

## Capabilities
### New Capabilities
None.
### Modified Capabilities
- `rust-inline-tui`: contextual prompt hierarchy and conditional status disclosure.

## Impact
Only terminal presentation, local status disclosure, regression tests and specification artifacts. No model binding, lifecycle, grant, routing or upgrade changes. Includes the existing uncommitted composer boundary preview; installation fix PR #1058 remains separate.
