## 1. Contract Review

- [ ] 1.1 Review the interaction-model contract against ADR-0039, ADR-0044,
  ADR-0045, ADR-0046/0047, ADR-0048/0049, and ADR-0050 to confirm no
  system-level decision is contradicted or duplicated.
- [ ] 1.2 Review overlap with `macos-shell-ui-ux-conformance` (visual
  treatment), `agent-runtime-ui-file-surfaces` (live runtime-owned UI files),
  and `expose-agent-rollout-history` (durable Rollout discovery), and
  confirm this change owns only interaction structure, modes, and vocabulary.
- [ ] 1.3 Update the Alan product glossary (AGENTS.md canonical names) with
  Alan Interaction Model, disclosure layers (Intent/Work/Files), interaction
  modes (conversation/background-servant/event-driven), review surface, and
  permission-as-grant terminology.

## 2. macOS Interaction Surfaces

- [ ] 2.1 Set `runtime_overrides.durability_required` in the `/proc/clone`
  document for background dispatch; snapshot discoverable Rollout IDs before
  spawn and acknowledge only a new Rollout whose first-record metadata matches
  the returned PID.
- [ ] 2.2 Add the review surface (results inbox) that lists completed
  Rollout-backed work with evidence links, shared by dispatched and
  event-driven outcomes; start only after `expose-agent-rollout-history`
  lands, source data from its mounted files and retained rollout/checkpoint
  evidence, and never add renderer-copied state.
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
- [ ] 3.3 Verify the review and Permissions surfaces render exclusively from
  mounted file state per ADR-0046.

## 4. Verification And Archive Readiness

- [ ] 4.1 Run `just quality` and the macOS focused tests
  (`just apple-shell-focused-tests`, `just apple-shell-ui-smoke`) with the new
  surfaces covered.
- [ ] 4.2 PR review confirms the implementation stays renderer-side: no
  kernel, aP, Rollout history schema, AgentFS, or runtime event machinery is
  introduced.
- [ ] 4.3 Sync delta specs into `openspec/specs/` (new
  `alan-interaction-model` and updated `alan-renderer-host-contract`) and move
  the change to
  `openspec/changes/archive/YYYY-MM-DD-define-alan-interaction-model/`.
