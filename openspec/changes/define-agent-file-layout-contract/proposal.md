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

- Define the generic process file layout (`io/`, `status`, `ctl`) that every
  process exposes, and the agent superset (`requests/`, `actions/`, `machine/`,
  `context/`).
- Define control as text commands written to `ctl`, so new control actions need
  no new files.
- Define `/agent` as a view over `/proc` (a union/bind of agent-conforming
  process directories plus stable aliases such as `/agent/root`).
- Define the LLM as a typed stream the process consumes, with effects governed
  by the process's namespace, not by the provider.
- Define the request as a view assembled from namespace files, tape compaction
  as a view over `machine/tape`, and an agent's tools as the `/bin` visible in
  its namespace.
- Define requests and responses as files, and the durable agent identity (home
  tree) that makes a restarted agent resume.

## Capabilities

### New Capabilities

- `agent-file-layout-contract`: the OS-level file-layout convention for agents —
  process layout, agent superset, `ctl` control, `/agent` as a `/proc` view, the
  LLM-stream consumer model, namespace-assembled requests, tools-as-`/bin`,
  request/response files, and the durable home tree.

### Modified Capabilities

- None.

## Impact

- Affected architecture: any agent runtime is a user-space file server that
  conforms to this contract; conformance — not a kernel flag — is what makes its
  processes operable as agents.
- Affected planning: LLM providers, memory, tools, and skills are separate
  user-space file servers referenced by this contract but specified elsewhere.
- Affected ADRs: implements ADR-0024 D1, D2, D4, and the agent-facing parts of
  D6, D7, and D8. Depends on `define-plan9-kernel-substrate`.
