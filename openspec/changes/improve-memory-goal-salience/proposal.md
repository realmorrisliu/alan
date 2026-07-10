## Why

Generated fallback memory surfaces derive `Current Goal` from the latest user
message, so a one-letter acknowledgement or an approval control displaces the
actual task goal and damages continuation (observed in the superseded
`harden-agent-operating-system-contracts` change, archived 2026-07-10). This is
content-layer logic that survived the namespace-native refactor nearly intact —
only the detection of control payloads changes, since approvals now arrive as
`requests/<id>/response` writes rather than chat messages.

## What Changes

- Derive fallback `Current Goal` from substantive user intent, active plan
  state, or durable task context instead of blindly using the latest user
  message.
- Never treat request-response control payloads (approval answers, selections,
  structured-input responses written to `requests/<id>/response`) as goals.
- Keep the prior substantive goal when the latest message is a short
  low-information follow-up; replace it when the latest message is a new
  actionable request.
- Salience filtering applies only to the generated goal field; conversation
  history and the tape are untouched.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `runtime-memory-surfaces`: fallback `Current Goal` derivation gains salience
  requirements (substantive-intent preference, control-payload exclusion,
  low-information follow-up retention).

## Impact

- Affected runtime modules: memory surface rendering / fallback goal derivation
  in `crates/agent-engine` (memory surface generation).
- Affected tests: one-letter follow-ups, request-response control payloads, new
  substantive requests, plan-state preference.
- Evidence-reference continuity in memory surfaces is NOT in scope here; it is
  covered by the existing `Rollout Remains Source Of Truth` requirement plus the
  `define-evidence-retention-and-projection` change.
