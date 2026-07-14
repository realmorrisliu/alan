# Tasks

## 1. Replace the stale contract

- [x] 1.1 Rewrite the proposal and design around Package Service, System Store,
  explicit Host Mount input, `/srv/package`, `/lib/pkg`, and Process references.
- [x] 1.2 Remove every current-change promise of Alan home package stores,
  workspace/AgentRoot/`~/.agents` providers, Host-side resolution, remote fetch,
  and package-helper execution.
- [x] 1.3 Rewrite ADR-0030 so ADR-0052's explicit-source decision is reflected
  in the accepted Quartermaster architecture.
- [x] 1.4 Strict-validate the change and the complete current OpenSpec surface.

## 2. Implement Package Service ownership

- [x] 2.1 Add Package Service domain types for package ids, source snapshots,
  fingerprints, revisions, exports, references, lifecycle state, and results.
- [x] 2.2 Implement bounded snapshot validation and deterministic Skill
  materialization for native `SKILL.md` roots and supported command Markdown.
- [x] 2.3 Implement the service-owned System Store with staged atomic commits,
  restart recovery, integrity validation, and no persisted Host source paths.
- [x] 2.4 Implement `/srv/package/{catalog,status,ctl,result}` with
  commit-on-clunk commands and bounded request results.
- [x] 2.5 Cover install/list/upgrade/uninstall, idempotence, collision,
  retirement, live-reference retention, and preinstalled-package guards.

## 3. Boot and project the service

- [x] 3.1 Add a required Package Service Boot Unit, executable, published
  `/srv/package` handle, readiness checks, supervision, and restart tests.
- [x] 3.2 Bind the channel `services/packages` System Store subtree through the
  Alan OS Host adapter without exposing its backing path to Alan OS records.
- [x] 3.3 Seed first-party Skill trees as deterministic preinstalled packages.
- [x] 3.4 Resolve explicit package references to immutable read-only handles and
  project only those handles beneath `/lib/pkg/<package-id>`.
- [x] 3.5 Prove running Process snapshots retain referenced revisions across
  upgrade/uninstall while new resolution observes the current catalog.

## 4. Cut Agent Runtime over to explicit packages

- [x] 4.1 Add the explicit installed-package reference input to Process launch
  assembly and keep descriptor-passed Skill/Agent Definition roots intact.
- [x] 4.2 Feed Agent Execution Engine only reference-derived installed Skill
  roots and descriptor-derived roots.
- [x] 4.3 Remove direct `builtin_capability_packages()` injection and obsolete
  path-enumeration helpers/tests.
- [x] 4.4 Explicitly reference the preinstalled first-party package set from the
  Root Agent Process boot context.
- [x] 4.5 Add guards proving workspace, AgentRoot, `.agents`, Alan home, and
  unreferenced System Store content are never scanned.

## 5. Add the `q` Process command surface

- [x] 5.1 Bind `/bin/q` and implement its Process image over the Package Service
  file surface.
- [x] 5.2 Complete Alan Shell's generic `/bin` command execution and output
  collection without package-specific builtins.
- [x] 5.3 Implement `q install`, `q list`, `q upgrade`, and `q uninstall` with
  stable human output, structured errors, explicit namespace paths, and no raw
  Host-path output.
- [x] 5.4 Test command execution through `/proc`, malformed input, unavailable
  service, permission failure, and non-zero exit behavior.

## 6. Dogfood and document

- [x] 6.1 Add a synthetic multi-Skill distribution fixture with shared assets,
  command Markdown conversion, and an unsupported runtime capability.
- [x] 6.2 Install the fixture from an explicit `/mnt` projection, inspect its
  catalog and `/lib/pkg` view, resolve its Skills in a new Agent Process, and
  verify the unsupported Skill remains visibly unavailable.
- [x] 6.3 Verify upgrade idempotence, changed-revision publication, retiring
  uninstall, final deletion, restart recovery, and stable/dev isolation.
- [x] 6.4 Update current architecture, Skill, and operator documentation using
  the canonical Package Service, Quartermaster, `q`, and System Store names.
- [x] 6.5 Add current-surface guards rejecting the removed Host-directory and
  Alan-home package vocabulary.

## 7. Final verification

- [x] 7.1 Run focused Package Service, Service Manager, Agent Engine, Shell, OS
  Host, and Alan integration tests.
- [x] 7.2 Run `just fmt`, `just lint`, `just test`, `just check`, and `just build`.
- [x] 7.3 Run strict OpenSpec validation for every current capability/change and
  record dogfood evidence in the implementation PR.

## 8. Remove runtime Package Store Host backing

- [x] 8.1 Amend D6 and capability deltas so immutable package File-Server
  handles/descriptors are the only runtime authority and Host backing grants
  are forbidden.
- [x] 8.2 Load package Skill metadata, sidecars, resources, and child-agent
  exports through the Process namespace/aP path while preserving existing
  capability semantics.
- [x] 8.3 Remove package `HostMountGrant` construction, backing-path
  translation, Tool-authority filtering, and obsolete compatibility tests.
- [x] 8.4 Prove referenced and inherited packages load without Host grants,
  remain read-only, expose only `/lib/pkg` paths, and never create native Tool
  authority.
- [ ] 8.5 Run focused and full verification, strict OpenSpec validation, push
  the implementation, and finish the current-HEAD Codex review loop.
