## 1. Contract Review

- [ ] 1.1 Review the interaction-model contract against ADR-0039, ADR-0044,
  ADR-0045, ADR-0046/0047, ADR-0048/0049, and ADR-0050 to confirm no
  system-level decision is contradicted or duplicated.
- [ ] 1.2 Review overlap with `macos-shell-ui-ux-conformance` (visual
  treatment) and `agent-runtime-ui-file-surfaces` (runtime-owned UI files) and
  confirm this change owns only interaction structure, modes, and vocabulary.
- [ ] 1.3 Update the Alan product glossary (AGENTS.md canonical names) with
  Alan Interaction Model, disclosure layers (Intent/Work/Files), interaction
  modes (conversation/background-servant/event-driven), review surface, and
  permission-as-grant terminology.

## 2. macOS Interaction Skeleton

- [ ] 2.1 Introduce the workspace entry view in Alan for macOS — active
  agents, recent work, installed services — as the default app view, with the
  shell demoted to an explicit tab type; keep the change presentation-only
  with no new runtime authority.
- [ ] 2.2 Add the review surface (results inbox) that lists completed agent
  work with evidence links, shared by dispatched and event-driven outcomes;
  data sourced from mounted execution-record files, never renderer-copied
  state.
- [ ] 2.3 Add the Permissions surface listing active host grants by label,
  scope, and access, with revocation; wire grant creation to drag-in, file
  picker, and agent-request approval sheet through Host Mount Service.
- [ ] 2.4 Render existing agent Work-layer affordances (conversation, plan
  card, approval sheet, Stop via `/proc/<pid>/ctl`) from their file
  surfaces and remove any raw-file or OS-vocabulary presentation from the
  default UI.
- [ ] 2.5 Add the Files-layer entry point ("view as files" inspector) that
  exposes the raw namespace as an explicit mode without becoming a second
  authority.

## 3. Conformance

- [ ] 3.1 Add a vocabulary-rule check (lint or review checklist) covering
  default-UI copy in `clients/apple` for quarantined OS terms.
- [ ] 3.2 Add renderer-host conformance tests proving Work-layer gestures
  become file writes and `ctl` commands with no renderer-local state
  mutation.
- [ ] 3.3 Verify the workspace entry, review surface, and Permissions surface
  render exclusively from mounted file state per ADR-0046.

## 4. Verification And Archive Readiness

- [ ] 4.1 Run `just quality` and the macOS focused tests
  (`just apple-shell-focused-tests`, `just apple-shell-ui-smoke`) with the new
  surfaces covered.
- [ ] 4.2 PR review confirms the change stays UX-only: no kernel, aP,
  AgentFS, or runtime event machinery is introduced.
- [ ] 4.3 Sync delta specs into `openspec/specs/` (new
  `alan-interaction-model`, updated `alan-renderer-host-contract`, updated
  `macos-shell-workspace-persistence`) and move the change to
  `openspec/changes/archive/YYYY-MM-DD-define-alan-interaction-model/`.
