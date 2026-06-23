## Context

Tool governance is two-stage: `PolicyEngine` returns `allow | escalate | deny`, then a `workspace_path_guard` execution guard enforces workspace containment. Escalations surface as recoverable `Yield` events and there is no session-wide approval cache. The decision audit (`ToolDecisionAudit`) already classifies `capability` (`read|write|network|unknown`), carries the policy `action`, and an optional `reason`. In the TUI, a pending yield is answered by typing into the composer; `request_id` leaks on screen; structured input requires hand-typed JSON.

The product wants a single human-in-the-end mode rather than per-action prompting. Critically, the workspace path guard is **not an OS sandbox**, so auto-approving arbitrary `bash`/network is unsafe until slice D lands a real sandbox.

## Goals / Non-Goals

**Goals:**
- A single auto-approve posture that proceeds on routine work and escalates only high-judgment operations.
- Fast, keyboard-only approval with enough context to decide informedly.
- Safe defaults: never silently widen permissions; never auto-approve what can't be shown to be routine.

**Non-Goals:**
- A mode switcher (default/auto-edit/plan/bypass) — exactly one mode.
- Persistent permission grants written to policy files.
- A session approval cache (the auto posture dissolves the need).
- Sandbox implementation (slice D); this slice ships the conservative pre-sandbox boundary.

## Decisions

### D1. Auto-approve posture keyed on existing capability classification
Change PolicyEngine defaults so the effective decision is: `read` → allow; `write` within the workspace → allow; `network`, write outside the workspace, explicitly destructive/irreversible operations, and `unknown` capability → escalate. Reuse the existing `capability` classification rather than inventing a parallel taxonomy.

Alternatives rejected: (a) keep escalate-frequently — unusable for power users; (b) workspace-trust "auto everything" — unsafe without a sandbox; (c) build the sandbox first — that is slice D and shouldn't block the UX gains here.

### D2. Safe-default escalation
Anything not provably routine escalates. `unknown` capability and unclassifiable operations are treated as needing judgment, not waved through. No decision is remembered across calls in a way that broadens future grants.

### D3. Single-key, context-rich approval
Replace composer-typed answers with a selection/number-key confirmation. The approval surface shows: the operation title and its structured presentation (diff for an edit, command line for bash — from slice B) and the audit `capability` + `reason`. `request_id` and other internal ids are never displayed.

### D4. Structured input as an inline form
Render `StructuredInput` yields as a form: one widget per `StructuredInputQuestion` honoring its kind (text/boolean/number/integer/single-select/multi-select) and options, with keyboard navigation and validation, instead of requiring hand-typed JSON.

### D5. Always-available interrupt
`Esc` issues an interrupt during any running turn (reinforced from slice A). Human-in-the-end is defined as conservative escalation **plus** a responsive stop; both are required for the posture to be safe.

## Risks / Trade-offs

- [Auto-approving in-workspace writes without an OS sandbox can still damage the workspace] → keep destructive/irreversible operations on escalation; the real mitigation (kernel enforcement) arrives in slice D, after which the boundary widens.
- [Capability misclassification could auto-allow something risky] → `unknown` escalates by default; add tests pinning the auto/escalate decision per capability and for known destructive patterns.
- [Users may want "don't ask again"] → intentionally not provided; the auto posture removes most prompts, and persistent grants are out of scope for safety.
- [Approval UX depends on slice B payloads] → when a structured payload is absent, fall back to the flat preview plus audit reason.

## Migration Plan

Behavioral change to runtime defaults; no data migration. Ship with the conservative boundary. Rollback is reverting to the prior posture. Slice D later relaxes D1 to allow sandboxed bash/network.

## Open Questions

- The exact list of "explicitly destructive/irreversible" operations recognized pre-sandbox (e.g. `rm`, `git push`, `git reset --hard`) and where that list lives.
- Whether the approval selection offers a "reject with feedback" path that returns text to the agent.
