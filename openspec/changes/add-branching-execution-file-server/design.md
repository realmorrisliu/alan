## Context

`alan-knowledge` already stores tape/memory/context as content-addressed blocks
and Merkle checkpoint roots. It can fork a root by referencing the base root and
appending only divergent blocks. ADR-0027 names the payoff: speculative /
branching execution over cheap forks.

This change adds the missing file boundary before adding a scheduler. A mounted
`branchfs` gives agents, tools, or future schedulers one file-shaped place to
create candidate branches, record scores, select a winner, and observe branch
lifecycle events.

## Goals / Non-Goals

**Goals:**

- Add `alan-branchfs` as a user-space aP file-server crate.
- Serve `ctl`, `branches/`, `selected`, and `events`.
- Use `alan-knowledge` checkpoint roots for branch state and cheap fork sharing.
- Require fork commands to name an existing visible branch id, not a bare content
  hash.
- Record branch lifecycle as JSON-line events on a retained blocking-read stream.

**Non-Goals:**

- Running automatic model calls or tools for each branch.
- Implementing tree-search strategy, ranking heuristics, or budget policy.
- Replacing daemon session fork, child-agent spawning, or AgentFS checkpoint
  commands.
- Persisting branchfs state across process restarts.

## Decisions

1. **Expose a small branch tree, not a scheduler.**

   The server root lists `ctl`, `branches`, `selected`, and `events`.
   `branches/<id>` is an inspectable JSON document with the base branch id,
   current root hash, status, optional score, and optional summary. This proves
   the file contract and leaves scheduling policy for a later mounted service.

2. **`ctl` accepts one JSON command document committed on clunk.**

   Commands are structured because branch creation needs multiple fields:
   `{"op":"fork","id":"candidate-a","from":"base","delta":"..."}`,
   `{"op":"score","id":"candidate-a","score":0.82,"summary":"..."}`,
   `{"op":"select","id":"candidate-a"}`, and
   `{"op":"discard","id":"candidate-a"}`. Commit-on-clunk matches the other
   Alan OS document-style control surfaces.

3. **Forks name visible branch ids, not content hashes.**

   A content hash is not authority. `branchfs` can be initialized with a visible
   base branch through bootstrap code, and later forks must name an existing
   `branches/<id>` entry. The server then uses the internal `alan-knowledge`
   store to fork from that branch's root. Knowing a root hash string is never
   enough to create a branch.

4. **Selection is explicit and inspectable.**

   Writing `select` to `ctl` updates `selected` and marks the selected branch.
   No branch is selected implicitly by score. This keeps judgment policy outside
   the file server while making the decision durable enough for future clients to
   tail or read.

5. **Discard hides the branch but keeps lifecycle evidence.**

   Discarded branches are removed from `branches/` so clients stop considering
   them. The discard record remains in `events`. Knowledge-store GC remains the
   backing store's job; this change does not add retention policy.

## Risks / Trade-offs

- [Risk] This can look weaker than "real" speculative execution because no model
  work is scheduled. -> Mitigation: the file boundary is the load-bearing piece;
  schedulers can later drive it with ordinary file operations.
- [Risk] JSON control documents are less shell-like than single-word ctl verbs.
  -> Mitigation: branch operations need typed fields; keeping them in one
  commit-on-clunk document avoids partial multi-file transactions.
- [Risk] Branch state is in-memory. -> Mitigation: v1 is a headless contract
  slice; persistence follows once the scheduler and durable home integration are
  specified.

## Migration Plan

1. Add `alan-branchfs` and focused tests.
2. Keep it unmounted by default.
3. Later mount it under a scheduler or Agent Runtime Service namespace when the
   speculative-execution policy exists.
