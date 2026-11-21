# Design

## Context

Canonical auto-approve-policy currently requires a single Autonomous posture and
forbids persistent permission grants. Guardian exists but production hardcodes the
default policy and main generation Connection. See ADR-0060 and the interview.

## Goals / Non-Goals

Reduce redundant interruptions within explicit delegated capabilities. Do not infer
filesystem authority from conversation, memory, cwd, a profile name or reviewer
output. Do not implement full-trust bypass or automatic input routing here.

## Decisions

Keep three axes separate: delegated scope, approval posture, actual isolation.
Retain serialized autonomous for the governed default; reject malformed/retired
aliases rather than inventing compatibility modes. Read-only is a scope preset,
not a third reasoning engine. A later dependent change adds fully trusted posture.

Host Mount Service owns saved file authorization and its live grant derivation;
its Host adapter binds canonical native identity without exposing private paths.
Agent Runtime owns operator-authored action delegation in its existing durable
policy subtree. Reference file-grant identities instead of duplicating path rights.
No globally addressable Project runtime object is introduced. Saved grants are
revocable operator state, never model memory or automatically discovered writable
repository config. Trusted CLI/file-control adapters authenticate user origin;
Agent credentials cannot create standing delegation through AgentFS writes.

A delegation binds resource identity, allowed actions, relevant destinations,
provenance and revocation revision. Project execution includes arbitrary code
inside the effective sandbox, not a promise that build/test command names are safe.
Dependency access is bounded to explicitly selected network/resource scope;
installation scripts gain no extra authority. If the backend cannot enforce the
chosen scope, report that fact and request another explicit user decision; never
silently broaden network or Host paths. Ordinary push can be delegated to exact
repository/remote/ref scope; force-push and deletion require distinct delegation.
Do not grant credentials merely by authorizing a publication action.

Each effect admission validates current scope and revocation state. Existing
mount handles, Process lifecycle and cancellation own enforcement. Parent launches
pass explicit equal/narrower capabilities; inheriting posture is not inheriting
all mounts. Race tests cover queued operations, mode changes, concurrent clients,
child launch and revocation before execution. Already completed effects remain
facts and are never silently replayed.

Guardian is a no-tools evaluation of a normalized action and verified delegation,
with untrusted transcript separated from authoritative user controls. A dedicated
Connection is configurable; generation-based structured results are acceptable
when that Connection does not expose typed evaluation. Do not require Jev or a
new Process. Freeze active reviewer identity, policy and grant revision per check;
reject stale decisions. A bounded fallback to the admitted main Connection uses
one aggregate deadline/call budget, not a fresh budget for every retry.

A denial requests a materially safer alternative, retaining the existing bounded
circuit breaker. Uncertainty/unavailability after fallback creates one correlated
pending decision. Independent admitted actions may proceed only where dependency
and ordering semantics prove they do not consume its result or require its rights;
no new concurrent scheduler or blind reordering is implied. If independence cannot
be proved, preserve the queue ordering. Clarification is distinct from authority.

Under ADR-0061's OS-boundary rule, a separate labeled Guardian shadow-corpus
classifier gate is obsolete and pruned: inside an active OS sandbox, routine
contained work skips Guardian entirely, uncontainable or out-of-grant effects hit
deterministic red-line and authority gates first, Guardian never grants
authority, and failed or uncertain reviews fail closed to one pending human
decision.

## Risks / Trade-offs

- Saved scope becomes stale → bind identity and revision; revoke projections and recheck at admission.
- Small evaluator misses intent → deterministic authority bounds, OS-boundary containment, and fail-closed human fallback.
- Reviewer outage blocks progress → bounded fallback, one request and independent work where valid.
- Same-account arbitrary code tampers with controls → enforce in confined mode; explicit Host-account successor documents the absence of this guarantee.

## Migration Plan

No implicit policy widening. Existing autonomous configurations retain their
current behavior until the new implementation is delivered. Enable saved scope
only from explicit user actions; invalidate stale reviewer decisions on policy or
mode changes. Rollback ignores new saved delegation and pauses affected pending
work rather than automatically executing it under old defaults.

## OS-boundary rule (ADR-0061)

Guardian reviews are reserved for actions whose effects escape or cannot be contained
by the active confinement. Deterministically delegated and contained routine work
needs no model review, which reduces permission noise without widening scope. The
reviewer remains a no-tools evidence source and never a security boundary or grant.
