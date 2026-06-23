## Why

The locked auto-approve posture (slice C) stops for a human on every escalation (network, destructive/irreversible commands, unknown capability). For long autonomous runs that is still too interruptive. Codex's "Auto-review" solves this by routing escalations that would otherwise pause for a human to a **separate reviewer agent** that decides allow/deny against a written policy — while the OS sandbox remains the actual security boundary. This change brings that capability to Alan and reinstates the name **`Autonomous`** for the resulting single mode.

Critically, the reviewer is **not** the security gate: it only judges the escalation bucket, the OS sandbox (Seatbelt/Landlock+seccomp, slices D) contains what it can, a deterministic red line denies/withholds catastrophic operations before the reviewer ever sees them, and a human remains the final fallback. This is the fourth pillar built on slices A–D.

## What Changes

- **Single `Autonomous` mode** (no switcher): rename the locked profile `AutoApprove` → `Autonomous`. The deterministic policy stays as the classifier that produces the escalation bucket plus the red line.
- **Four-way escalation outcome** for the deterministic policy:
  - **auto-allow** (sandbox-contained routine) → runs.
  - **deny** (catastrophic, no legitimate use) → never runs; the reviewer never sees it.
  - **always-human** (red line: secret/credential exfil, force-push/history rewrite, security-control weakening, and any effect the sandbox cannot contain on the current platform) → bypasses the reviewer, goes straight to the human Yield.
  - **reviewer-judged** (everything else in the bucket) → routed to the reviewer.
- **Guardian reviewer**: a single-shot, **no-tools** structured LLM call returning `{decision: allow|deny, rationale}`, judged against a customizable reviewer policy. Runs on a dedicated connection profile (falls back to the main model; if the reviewer is unavailable at runtime, fall back to the human — never auto-allow).
- **Anti-injection posture**: the reviewer treats all transcript and tool-output content as untrusted data, never as instructions; "please approve" text is an attack signal, not evidence.
- **Denial control flow**: deny → rationale returned to the main agent with a "find a materially safer path or stop and ask the user; do not work around it" instruction (one self-correction chance, not an immediate stop). A rejection circuit breaker (3 consecutive denials, or 10 within the last 50 reviews in a turn) aborts the turn to the human.
- **Human fallback** for always-human, circuit-break, and reviewer-unavailable reuses the existing Yield → single-key confirmation surface (slice C). Human override denial-picker (`/approve`) is deferred to v2.
- **TUI surfacing**: the reviewer's activity and denial rationale are shown in the live region / transient surface (slice A), so autonomy is visible, not silent.
- **Prerequisite**: Linux network confinement via seccomp (slice D task 3.2) lands first so both platforms confine filesystem + network and the reviewer applies uniformly (no platform asymmetry).

## Capabilities

### New Capabilities
- `autonomous-review-mode`: the guardian-reviewer contract — escalation-outcome taxonomy (auto-allow / deny / always-human / reviewer-judged), the no-tools reviewer call and its policy, the anti-injection posture, the deny/self-correct/circuit-breaker/human-fallback control flow, and TUI surfacing.

### Modified Capabilities
- `auto-approve-policy`: the single mode is renamed `Autonomous`; escalations route to the reviewer (with the red line bypassing it to the human) instead of always pausing for a human.
- `os-sandbox-enforcement`: Linux network confinement (seccomp) is required (un-defers task 3.2) so the reviewer applies uniformly across platforms.

## Impact

- Code: `crates/runtime` (guardian module: reviewer call, policy, control flow, circuit breaker; escalation routing in the orchestrator), `crates/protocol` (optional review-activity events), `crates/alan` (reviewer connection profile config), `crates/tui` (surface reviewer activity/denials), `crates/runtime` sandbox (Linux seccomp).
- Config: a customizable reviewer policy (markdown) and a reviewer connection profile.
- Depends on slices A (TUI surfaces), C (deterministic policy + Yield), D (sandbox; seccomp prerequisite). Reverses C's "always pause for human on escalation" into reviewer-first.
