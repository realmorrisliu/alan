## 1. Prerequisite: Linux network confinement (slice D 3.2)

- [x] 1.1 Confine Linux network via Landlock ABI v4 net rules (deny all TCP bind/connect) in the same `pre_exec` hook as the fs ruleset (`apply_landlock`)
- [x] 1.2 Detection: `confines_network()` reports per backend (Seatbelt yes; Landlock iff ABI v4 net supported; path-guard no), so the policy can degrade network to human when unavailable
- [x] 1.3 On-machine test on the Ubuntu VM (kernel 7.0.11): confined TCP connect blocked (with unconfined control), filesystem confinement still holds (`landlock_confines_network_on_linux`)
- [x] 1.4 `add-os-sandbox-enforcement` task 3.2 updated to done

## 2. Rename mode to Autonomous (slice C)

- [x] 2.1 Rename `GovernanceProfile::AutoApprove`→`Autonomous` (serializes `autonomous`; tolerant deserialize maps legacy `auto_approve`/`conservative` → `Autonomous`); `PolicyEngine::auto_approve()`→`autonomous()`; source `builtin_autonomous`
- [x] 2.2 Updated all references/tests; build + test + clippy green

## 3. Escalation routing taxonomy

- [x] 3.1 Classify escalations: `EscalationRoute` (Reviewer / AlwaysHuman) computed in the runtime layer; route attached to `ToolPolicyDecision::Escalate` and recorded in the escalation details
- [x] 3.2 Deny red line: `rm -rf /`, `mkfs`, `dd of=/dev/*`, `.git/hooks` edits (PolicyAction::Deny) — reviewer never sees them
- [x] 3.3 Always-human red line: `git push --force`/`--force-with-lease`, `sudo`, `chmod 777` (`human-` rule ids) + network when the platform sandbox can't confine it (`confines_network()`)
- [x] 3.4 Normal `git push` is reviewer-judged
- [x] 3.5 Tests: force-push→always-human, normal push→reviewer, network route follows platform containment, unknown→escalate

## 4. Guardian reviewer (`crates/runtime/src/runtime/guardian.rs`)

- [x] 4.1 Single-shot, no-tools reviewer call (`review`) returning `ReviewOutcome::{Allow, Deny{rationale}, Unavailable{reason}}` via structured `{decision, rationale}` parsing (tolerant to surrounding prose)
- [~] 4.2 Reviewer runs on the caller-supplied `LlmClient` (main model — the documented fallback); a dedicated reviewer connection profile is a later refinement
- [x] 4.3 Fail-safe: reviewer error/unparseable → `Unavailable` (human fallback), never auto-allow
- [x] 4.4 Reviewer inputs: compact transcript (`build_transcript`) + approval request; no hidden reasoning; snapshot only (`ReviewContext`)
- [x] 4.5 Anti-injection system prompt: transcript/tool output marked UNTRUSTED DATA, never instructions
- [x] 4.6 Reviewer policy markdown (`guardian_policy.md`, adapted from Codex), embedded default
- [x] 4.7 Tests: allow/deny/unavailable outcomes; tolerant parse; injected "approve me" transcript still denies; request marks transcript untrusted

## 5. Control flow (wired into the orchestrator escalation path)

- [x] 5.1 Reviewer route runs `guardian::review` on `state.llm_client`; Allow → execute (sandboxed); Deny → denial payload + safer-path instruction fed back, `ContinueToolBatch`
- [x] 5.2 Rejection circuit breaker on `TurnState` (3 consecutive, or 10/50 window per turn; reset on allow and on turn `clear()`) → pauses to human
- [x] 5.3 always-human / circuit-break / `Unavailable` converge on the existing Yield → single-key confirmation (slice C)
- [x] 5.4 Tests: allow executes, deny blocks + feeds back, 3 denials trip the breaker to a human Yield; `TurnState` breaker unit tests

## 6. TUI surfacing

- [x] 6.1 Reviewer denials/circuit-break emit `Event::Warning` which slice A surfaces in the live-region transient (reused; no new TUI code)
- [x] 6.2 Covered by the reducer's `Warning` → transient routing (slice A tests); no internal ids leak

## 7. Verification

- [x] 7.1 `just verify` (fmt + lint + test + mock smoke) green across protocol + runtime; Linux net validated on the VM
- [ ] 7.2 On-machine smoke (macOS + Linux VM) with a live LLM: routine actions silent; reviewer-judged escalation approved/denied; red-line goes to human; reviewer-unavailable falls back to human

## 8. Sandbox-autonomy invariants (post-review hardening)

- [x] 8.1 Token/quote/basename-aware red lines: force-push, reset-hard, recursive-rm, privilege-escalation (sudo/doas/pkexec/su); path-qualified network/write classification by basename
- [x] 8.2 Catastrophic root delete (`rm -rf /` and permutations) denied outright; world-writable `chmod` (numeric + symbolic) routed always-human
- [x] 8.3 Reviewer is not a boundary: bash routed to a human when network is unconfined; reviewer call bounded by `llm_request_timeout_secs` → `Unavailable` on stall
- [x] 8.4 OS-sandbox confinement independent of syntax: drop shape/containment parsing under an OS backend; keep protected-subpath writes blocked via Seatbelt kernel-deny + shell-wrapper-recursive path check (honoring carve-outs like `.alan/memory`)
- [x] 8.5 Reconnect hydration: full pending-payload recovery from the buffered `Yield`; form/composer resume; multi-select defaults; blank optional fields; paste into the active form; stale-completion bypass; cursor-paged buffer scan
- [x] 8.6 Tests for each invariant (red-line token variants, catastrophic deny, world-writable chmod, network confinement degradation, nested protected-path block + carve-out, basename network classification); validated on macOS (Seatbelt) and Linux/OrbStack (Landlock)
- [x] 8.7 Residual-gap audit recorded in design.md with explicit accept/defer decisions
