## Context

Alan currently has three conflicting architectural strata. The accepted Alan OS model says that
Process, Namespace, files, descriptors, `/proc`, `/agent`, `/srv`, mounted File-Server Services, and
Service Manager are the system boundary. Canonical OpenSpec still requires daemon HTTP/WebSocket
routes, Session identity, relay and reconnect behavior, and daemon-backed macOS surfaces. The code
implements both models at once.

This change removes the obsolete normative stratum before any new Alan for macOS attachment design
is chosen. It is the first half of a stacked cleanup program; `remove-daemon-era-implementation`
must already be implementation-complete and review-green before this change merges. OpenSpec
archive history remains immutable and non-normative.

## Goals / Non-Goals

**Goals:**

- Leave one canonical Alan OS model with no supported or transitional daemon/session API boundary.
- Delete whole obsolete capabilities instead of retaining tombstone requirements.
- Redistribute still-valid behavior to Process, Agent Machine, AgentFS, Memory Store, provider,
  Skill, policy, namespace, and renderer owners.
- Remove Session as a domain object without creating an equivalent replacement center object.
- Make the contracts change and implementation change independently reviewable but operationally
  atomic.
- Preserve immutable OpenSpec archive history as provenance only.

**Non-Goals:**

- No design for how Alan for macOS boots, discovers, attaches to, or communicates with Alan OS.
- No compatibility period, deprecation endpoint, legacy route adapter, data migrator, or feature
  replacement.
- No rewrite of archived OpenSpec change text.
- No attempt to remove legitimate uses of the general word "session" for an Apple/XPC/terminal or
  third-party protocol concept unrelated to the retired Alan Agent Session.

## Decisions

### 1. Remove contracts before replacement design

`daemon-api-contract` and `remote-control-contract` are deleted, not frozen as current
compatibility contracts. `runtime-core-contract` is also deleted because its organizing object is
the app-server Session; retaining a reduced shell would keep the wrong owner alive. Valid
requirements are copied into the existing capabilities that already own their semantics.

Alternative considered: mark the three capabilities deprecated until macOS migration. Rejected
because a canonical deprecated contract still authorizes maintenance and new consumers.

### 2. Session decomposes into existing owners

No Thread, Conversation, Run, or renamed Session capability is introduced. The ownership map is:

| Former Session concern | Canonical owner |
| --- | --- |
| lifecycle and identity | Process / Agent Process |
| tape, transition state, checkpoints | Agent Machine |
| user-input transition | turn |
| input/output/control | `/agent/<pid>` and `/proc/<pid>` files |
| durable execution evidence | rollout/checkpoint files |
| working continuity | Agent-Process-local Working Memory |
| cross-process continuity | Episodic Memory, handoff, and other Memory Stores |
| renderer hydration and live updates | AgentFS snapshots and offset-readable streams |

The Event/Op alphabet may remain where the Agent Execution Engine still uses it, but it is no
longer specified as a client/server Session protocol.

### 3. Remove entire obsolete capabilities and modify every live dependent contract

The proposal's capability list is the exhaustive contract migration inventory. A canonical spec
that currently names the daemon as a consumer, transport, authority, compatibility path, data
source, test category, or future owner receives a delta. Session-shaped requirements outside those
files are also migrated when they refer to the retired Agent Session rather than a legitimate
third-party or terminal concept.

Alternative considered: rely on a repository-wide word replacement. Rejected because `session`
and `daemon` also appear in legitimate Apple LaunchDaemon, terminal, authentication, and transport
contexts; ownership must be judged semantically.

### 4. Fold already-resolved successor contracts

The accepted content of `define-alan-app-service-integration` and
`define-remote-access-service` is copied into this change as new canonical capabilities. Their
original active changes are then retired as superseded without applying the daemon-era capability
deltas they carried independently. This produces one coherent archive point and avoids leaving
contract-only changes open after the reset.

### 5. Archive history is immutable but excluded from authority and guards

`openspec/changes/archive/` may contain daemon-era language and examples. Current docs, agent
instructions, active changes, canonical specs, code guards, and help surfaces may not cite archived
content as current authority. Semantic deletion checks explicitly exclude the archive tree rather
than rewriting history to achieve a misleading string-zero result.

### 6. The two changes are a stacked atomic program

The contracts and implementation changes remain separate because they have different review
questions: ownership correctness versus executable deletion. The implementation change must be
complete and green before the contracts change merges; the contracts change merges first, the
implementation change rebases and follows immediately. There is no supported release or prolonged
mainline state between them.

### 7. Alan for macOS attachment remains deliberately undecided

The contract reset may state what Alan for macOS is not authoritative for, but it does not select
an embedded runtime, child process, Service Manager lifecycle, Unix socket, FFI, XPC, aP transport,
or any other attachment mechanism. Removal of the daemon-backed Console and Settings consumers is
owned by the implementation change without replacement.

## Risks / Trade-offs

- [A valid invariant is lost with `runtime-core-contract`] → Map every requirement before removal
  and require an explicit destination or an explicit obsolete classification in the contract audit.
- [The large delta set becomes inconsistent] → Use one ownership matrix, validate every modified
  capability strictly, and run a semantic scan across all non-archived planning surfaces.
- [Main briefly has contracts ahead of implementation] → Require both PRs review-green and merge
  them consecutively as a stack.
- [Historical archive searches look contradictory] → Keep archive provenance but document and test
  that it is non-normative and excluded from current-source searches.
- [The reset accidentally designs macOS attachment] → Reject any task or requirement that chooses a
  new transport, lifecycle, or client API.

## Migration Plan

1. Complete the full capability ownership matrix and delta specs.
2. Fold the two resolved successor contracts and mark their original active changes superseded.
3. Update current ADRs, glossary, agent instructions, docs, help contracts, and active changes.
4. Validate every delta and the entire OpenSpec tree; review all non-archived daemon/session hits.
5. Finish and review `remove-daemon-era-implementation` before merging this change.
6. Merge contracts, immediately rebase and merge implementation, then archive both in order.

Rollback before merge is ordinary source rollback. After both changes and the destructive state
cleanup land, code can be restored from version control but deleted local daemon/session data has
no recovery promise.

## Open Questions

None. Alan for macOS attachment is intentionally a later design question, not an unresolved part of
this change.
