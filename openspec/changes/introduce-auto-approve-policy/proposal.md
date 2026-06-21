## Why

The agent currently surfaces escalations as recoverable yields with no session-wide approval memory, and the TUI makes the user type words ("approve"/"reject") into the composer to answer them. For a power user driving the agent through many file operations, this is unusable — it interrupts constantly and demands typing for yes/no. The product goal is a single **human-in-the-end** posture (like Codex/Claude Code auto modes): the agent proceeds automatically on routine work and stops only for the operations that genuinely need human judgment.

This is **slice C of the four-slice TUI parity program**. It builds on slice B (`add-structured-tool-rendering`) so approval prompts can show the diff/command being approved, and it is later loosened by slice D (`add-os-sandbox-enforcement`), which makes auto-approving bash/network safe.

> **Extended by `add-autonomous-review-mode`:** the single mode is renamed `AutoApprove` → `Autonomous`, and escalations are routed to a reviewer agent (with a deterministic red line bypassing it to the human) instead of always pausing for a human. The deterministic policy from this change becomes the classifier feeding that reviewer. See that change for the reviewer contract.

## What Changes

- **One auto-approve mode** (no mode switcher): replace the escalate-frequently posture with an auto-approve-by-default posture driven by the existing capability classification.
- **Conservative escalation boundary (pre-sandbox):** auto-allow `read` and in-workspace `write`; escalate `network`, out-of-workspace writes, explicitly destructive/irreversible operations, and `unknown` capability. This boundary is the interim until slice D enables a real sandbox.
- **Safe-by-construction:** escalation is the default for anything not provably routine; there is no silent widening of permissions.
- **Single-key approval UX:** confirmations are answered by selection/number keys, not free text. The prompt shows what is being approved (the structured diff/command from slice B) plus the policy `capability` and `reason` from the decision audit.
- **Structured input as a real form:** structured-input yields render as an inline form (fields + selection) instead of requiring hand-typed JSON.
- **Always-available interrupt:** `Esc` interrupts a running turn at any time; informed human-in-the-end requires both a conservative boundary and a responsive stop.
- Internal identifiers (`request_id`) never appear on screen (reinforces slice A).

## Capabilities

### New Capabilities
- `auto-approve-policy`: the human-in-the-end policy posture — the auto/escalate boundary by capability, safe-default escalation, and the approval interaction contract (single-key confirmation, contextual approval content, structured-input forms, always-available interrupt).

### Modified Capabilities
- (none — the new posture is specified as its own capability; it changes runtime defaults rather than another spec's stated requirements)

## Impact

- Code: `crates/runtime` (PolicyEngine defaults/posture, escalation classification), `crates/tui` (approval/yield rendering and key handling, structured-input form).
- Behavior: most routine operations stop prompting; risky operations still escalate. No persistent permission grants are written.
- Depends on slice B for showing diff/command context in approvals; the boundary widens in slice D.
