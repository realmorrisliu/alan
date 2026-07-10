## Context

Fallback memory surfaces (used when the agent did not author a semantic
continuation summary) fill `Current Goal` with the latest user message. After a
substantive task, users often send "y", "ok", or approve a pending request;
today that fragment becomes the recorded goal and the next runtime resumes with
no idea what it was doing. The superseded
`harden-agent-operating-system-contracts` change specified the fix in the old
message-typed world; this change restates it for the current runtime, where
approvals are `requests/<id>/response` writes, not chat messages.

## Goals / Non-Goals

**Goals:**

- Fallback `Current Goal` prefers explicit plan state or the latest substantive
  user task over the literal latest message.
- Control payloads and low-information fragments never displace a substantive
  goal.

**Non-Goals:**

- Changing agent-authored semantic summaries (already preferred by spec).
- Filtering anything out of conversation history or the tape itself.
- Evidence-reference continuity (owned by `Rollout Remains Source Of Truth`
  plus `define-evidence-retention-and-projection`).
- An LLM call to judge salience — the existing no-hidden-summarization-call
  requirement stands; salience is mechanical.

## Decisions

### 1. Salience is a mechanical preference order, not a model judgment

Derivation order: active plan state → latest substantive user request →
(only if nothing better exists) the latest message verbatim. "Substantive" is a
mechanical test (not a control payload; above a minimal information threshold,
e.g. not a bare acknowledgement token). This keeps the fallback path free of
hidden model calls, consistent with the existing requirement that runtime
refresh must not initiate extra summarization requests.

Alternative considered: asking the model to pick the goal during refresh.
Rejected: violates the existing no-hidden-request requirement and adds cost to
every turn end.

### 2. Control payloads are identified by origin, not by content heuristics

Anything that arrived as a `requests/<id>/response` write (approval, selection,
credential, structured input) is control-plane by construction and is excluded
from goal derivation categorically. Content heuristics are only needed for
ordinary chat fragments ("y", "ok"), where a conservative shortness/token test
suffices because the cost of a false positive is merely keeping the previous
goal.

### 3. Suppression keeps the old goal rather than emptying the field

When the latest message is filtered, the surface retains the prior substantive
goal (possibly marked as carried forward). An empty goal is worse than a stale
one for continuation.

## Risks / Trade-offs

- [Risk] A real terse command ("deploy it") is filtered as low-information →
  threshold errs toward acceptance: filter only bare acknowledgement-class
  fragments; imperative verbs pass. The prior goal being kept (not the message
  being lost) bounds the damage — the fragment stays in history.
- [Risk] Plan state is stale and outranks a fresh substantive request → a new
  actionable user request always replaces the goal; plan state outranks only
  filtered fragments.
