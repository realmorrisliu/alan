> **Re-scoped by ADR-0024/0025; tasks reset.** Alan Agent is the optional
> workspace-app *client* that reads agent files (`status`, `io/`, `requests/`,
> `actions/`, `machine/`, `context/`, `children/`, `events`) and writes `ctl` —
> not a "projection" of objects/buffers/views/evidence/artifacts (that V1
> ontology is retired, and its code was removed in 70c4c02e). The boxes below are
> unchecked: no implementation exists in the tree, and the file-client model has
> not been built. The tasks are rewritten to the file-client model.

## 1. Workspace client over agent files

- [ ] 1.1 Render an agent's conversation by reading `/agent/<pid>/io/output` and
  tailing `events`; submit input by writing `io/input`.
- [ ] 1.2 List and inspect agents by walking `/agent` (a view over `/proc`).
- [ ] 1.3 Review and answer requests by reading `requests/<id>/` and writing the
  response file; review actions by reading `actions/<id>/`.
- [ ] 1.4 Control agents (interrupt, compact, rollback) by writing `ctl`.

## 2. Boundaries

- [ ] 2.1 Depend only on aP (`alan-ap`) like any client; hold no private session
  or projection state (ADR-0025 client layer).
- [ ] 2.2 Read memory/evidence as files (Memory Stores, action records); do not
  reintroduce object/buffer/view/evidence/artifact kernel surfaces.

## 3. Verification

- [ ] 3.1 Tests for the workspace client against an in-memory aP agent surface.
- [ ] 3.2 Run `just verify`.
- [ ] 3.3 Run `openspec validate migrate-alan-agent-to-agent-workspace --strict`.
