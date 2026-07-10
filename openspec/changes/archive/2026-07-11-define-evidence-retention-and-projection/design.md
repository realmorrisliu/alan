## Context

The superseded `harden-agent-operating-system-contracts` change proposed an
evidence-artifact store with artifact ids, digests, and an authorized artifact
reader API. Two months of substrate work made most of that machinery redundant
or wrong:

- `io/output` is contractually append-only and complete; "clipping of the
  newest output is a renderer concern, never a gap in the stream's data"
  (`agent-file-layout-contract`).
- `actions/<id>/` records a tool effect's output and result by referencing the
  tool process rather than duplicating it.
- `content-addressed-knowledge` provides dedup, tamper-evident history,
  retention/GC owned by the storing file server, and the rule that a hash is
  not a capability — retrieval is gated by namespace reachability.
- An "authorized artifact reader" would be a side-channel authorization API,
  which the iron law forbids; authorization is mount visibility.

What is genuinely unfinished: tape records that stand in for oversized outputs
do not carry a usable reference to the full content; a child's or tool's output
can become unreachable after process exit; and nothing marks redaction in
durable evidence.

## Goals / Non-Goals

**Goals:**

- One projection shape for oversized tool and child outputs: bounded preview +
  namespace-path reference + truncation metadata.
- Evidence referenced from a tape stays readable after the producing process
  exits, under the storing file server's retention.
- Redaction happens before durable persistence and is marked as redaction.

**Non-Goals:**

- A new artifact store, artifact-id scheme, or artifact read API
  (content-addressed-knowledge is the backing store; files are the surface).
- Answer-level provenance summaries linking final answers to evidence (dropped;
  the tape already interleaves answers with the referenced records, and a
  dedicated summary can be proposed later if tape-reading proves insufficient).
- Changing prompt-facing truncation budgets themselves.
- Cross-host/remote evidence access (belongs to `define-remote-access-service`).

## Decisions

### 1. Evidence references are namespace paths with offsets

A tape record that previews an oversized output references the full content by
namespace path — the action's output file (`actions/<id>/output`) for tool
effects, the child's `io/output` (optionally with offset/length) for delegated
work. Readers resolve the reference by walking their namespace; a reader
without the mount simply cannot resolve it, which is the authorization model.

Alternative considered: content-hash references. Rejected as the primary key:
a hash is deliberately not a capability and not walkable; hashes may appear as
integrity metadata alongside the path, not instead of it.

### 2. Post-exit readability is a retention obligation on the storing server

Action records and agent homes are backed by storage-owning file servers; the
contract requires that content referenced from a durable tape remains
resolvable for as long as that tape's retention keeps the tape itself. This
rides `content-addressed-knowledge` reachability GC: a live tape root keeps its
referenced evidence blocks reachable, so the invariant is "evidence lives at
least as long as the tape that cites it" — not "forever".

Alternative considered: copy full outputs into the tape. Rejected: reintroduces
unbounded tape growth, which the projection budget exists to prevent.

### 3. Redaction is marked, not silent

Redaction runs before durable persistence (same seam as today's rollout
redaction rules) and replaces spans with an explicit marker carrying a reason
class. Truncation metadata and redaction markers are distinct: an auditor must
be able to tell "cut for size, full content at this path" from "removed for
secrecy, not recoverable".

### 4. Delegated output is retained in a parent-side action record

Oversized delegated output is redacted and copied once into the delegated
invocation's parent-side `actions/<id>/output` file. The tape `output_ref`
points at that path, with offset `0` and the emitted length. The child session
id and raw rollout path remain optional debug metadata only; they are never the
resolution mechanism.

This resolves the earlier open question in favor of the parent-side action
record. The parent can assert the path is walkable before emitting it, the
record survives child-home cleanup, and tool and delegated evidence use the
same retention/redaction surface. Pointing directly at child `io/output` remains
valid for future namespace arrangements where that child tree is already
mounted and retained, but the current launcher does not emit such a reference.

Offsets carry an explicit length in the first implementation. This makes the
retained evidence range stable and avoids read-to-end ambiguity if a referenced
stream later receives additional bytes.

## Risks / Trade-offs

- [Risk] Offset-based references go stale if a stream is compacted → streams
  referenced by durable tapes are append-only by contract; retention removes
  whole objects, not prefixes, so an offset either resolves or the whole
  reference reports "expired by retention" — never silently shifted content.
- [Risk] Retention pinning bloats storage for chatty children → dedup and
  reachability GC bound this; retention policy stays with the storing server
  and can cap by age/size with the reference then resolving to a structured
  expiry error.
- [Risk] Parent namespaces may not mount child homes today → the projection
  must only emit references the parent can actually resolve; tests assert
  resolvability at emission time, and unresolvable cases keep inline previews
  as the complete record with truncation marked.

## Resolved Questions

- Child output references point at the parent-side delegated action record.
- First-version references carry both offset and length.
