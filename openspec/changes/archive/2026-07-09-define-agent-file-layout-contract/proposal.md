## Why

ADR-0024 / D3 removes `Agent Process` as a kernel type: the kernel knows only
`Process`. Uniform operation of agents must therefore come from a *published
file-layout convention*, not from a kernel category — exactly as Plan 9's `/net`
gives uniform networking through a directory convention plus `ctl` files rather
than a "connection" kernel type.

This change defines that convention as an OS-level contract that any agent
runtime (file server) must satisfy so that shells, tools, other agents, and
auditors can operate every agent the same way. It sits above
`define-plan9-kernel-substrate` and depends on nothing the kernel does not
already provide.

## What Changes

- Define the generic process file layout — the full `/proc/<pid>` substrate
  layout (identity/parentage/credentials/namespace/exit state) plus the `io/`,
  `status`, `ctl` IO/control subset — that every process exposes, and the agent
  superset (`requests/`, `actions/`, `machine/`,
  `context/`, `children/`, and the top-level aggregate `events` stream),
  overlaid at `/agent/<pid>` while `/proc/<pid>` stays generic.
- Define control as text commands written to `ctl`, so new control actions need
  no new files.
- Define `/agent` as an overlay over `/proc` (for each agent-conforming process,
  union the kernel `/proc/<pid>` generic files with the runtime's agent surfaces,
  plus stable aliases such as `/agent/root`).
- Define the LLM as a typed stream the process consumes, with effects governed
  by the process's namespace, not by the provider.
- Define the request as a view assembled from namespace files, tape compaction
  as a view over `machine/tape`, and an agent's tools as the `/bin` visible in
  its namespace.
- Define the referenced external file-server boundaries for LLM provider access,
  Memory Stores, Tools, and Skills, without taking ownership of their detailed
  protocols.
- Define requests and responses as files, and the durable agent identity (home
  tree) that makes a restarted agent resume.
- Define the namespace's **access discipline** — consolidated here as the single
  authoritative home: `ctl` scoped to one lifecycle-bearing object; `machine/tape`
  and `events` append-only with an exclusive-write lease held by the generating
  engine; write authority keyed to the acting actor with extension by interpose
  (the iron law); external writers gated on a protocol-layer tape lease; and an
  in-band self-describing namespace.

## Capabilities

### New Capabilities

- `agent-file-layout-contract`: the OS-level file-layout convention for agents —
  process layout, agent superset, `ctl` control, `/agent` as a `/proc` view, the
  LLM-stream consumer model, namespace-assembled requests, tools-as-`/bin`,
  referenced LLM/memory/tool/skill file-server boundaries, request/response
  files, the durable home tree, and the namespace access discipline (ctl
  scoping, the generation tape lease, actor-keyed authority + interpose, the
  external-writer protocol-layer prerequisite, self-description).

### Modified Capabilities

- None.

## Supersession

This contract is the single authoritative home for the agent file-layout surface
and its access discipline. Two overlapping efforts are folded in here:

- `add-external-namespace-writers` (an unmerged draft) is **retired**; its
  `external-namespace-write-authority` requirement — external writers need the
  `machine/tape` lease at the aP protocol layer, not just an agent-file-server
  check — is folded into this contract.
- `refactor-engine-namespace-native`'s access-discipline design (its D7–D12 notes
  on `ctl`/answer roles, the tape lease, actor permission, and interpose) is
  **moved here**; that change keeps only the engine-implementation requirements
  (environment-as-namespace, generation/tools/state as file ops, M2) and now
  references this contract for the file surface. Its earlier D7 shape — answering
  via `requests/<id>/ctl` and lifecycle verbs on `machine/ctl` — is **not** adopted:
  this contract keeps answering as a `requests/<id>/response` write committed on
  clunk, generic lifecycle control on the kernel `/proc/<pid>/ctl`, and
  `machine/ctl` for agent-runtime tape/checkpoint commands.

## Impact

- Affected architecture: any agent runtime is a user-space file server that
  conforms to this contract; conformance — not a kernel flag — is what makes its
  processes operable as agents.
- Affected planning: LLM providers, memory, tools, and skills are separate
  user-space file servers referenced here at their mount/descriptor boundaries
  but specified in detail by their owning OpenSpec capabilities.
- Affected ADRs: implements ADR-0024 D1, D2, D4, and the agent-facing parts of
  D6, D7, and D8. Depends on `define-plan9-kernel-substrate`.
