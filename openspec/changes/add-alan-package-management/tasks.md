# add-alan-package-management — tasks

## 1. Package store and lifecycle model (library, in the agent-engine skills module space)

- [ ] 1.1 Define the data model: distribution package, store backing under the channel alan home (`~/.alan/pkg/`, dev `~/.alan-dev/pkg/`), provenance record, materialization manifest with content hashes, operation report types
- [ ] 1.2 Implement source fetch: git URL clone / local directory copy into the store, commit resolution, tolerating non-git sources
- [ ] 1.3 Implement materializable-skill scanning by convention (command-style `*.md`, portable `*/SKILL.md`) with include/exclude flags

## 2. Materialization primitives

- [ ] 2.1 Implement command-`.md` conversion: derive `name`/`description` frontmatter, inject the versioned adapter preamble, preserve the body verbatim
- [ ] 2.2 Implement the known foreign-vocabulary table (converter data, versioned) and the scan that emits `capabilities.required_tools` plus unknown-token report entries
- [ ] 2.3 Emit canonical `/lib/pkg/<name>/...` namespace paths in the adapter preamble for upstream-relative helper references (e.g. repo-root `tools/*.py`); no host paths in generated content
- [ ] 2.4 Implement portable-package adoption: validate via existing loader rules, copy without content edits
- [ ] 2.5 Write owning-package `provenance` blocks into materialized `package.yaml` sidecars
- [ ] 2.6 Implement skill-id collision detection against manifest ownership: warn-and-skip default, explicit force to overwrite
- [ ] 2.7 Implement duplicate-source precedence: same skill id from command file and portable package → convert the command file, skip the portable duplicate, record the choice in the report

## 3. Lifecycle operations

- [ ] 3.1 Implement install: fetch + materialize + provenance/manifest write, atomic enough that a failed install leaves no partial skills
- [ ] 3.2 Implement upgrade: no-op on unchanged commit+converter, re-materialize on change, manifest-hash divergence warn/skip/force
- [ ] 3.3 Implement uninstall: delete exactly manifest-listed files plus the store entry; preserve and report diverged files unless forced
- [ ] 3.4 Project the store read-only at `/lib/pkg` via the host-directory mount machinery on install/boot; verify Agent Processes read package content through the namespace
- [ ] 3.5 Implement execution-backend resolution of `/lib/pkg/...` paths to store backing for spawned helper processes (deterministic prefix mapping via the mount table), with the narrow guard exception for runtime-resolved store paths; verify direct agent references to the backing stay denied
- [ ] 3.6 Implement list: packages with provenance and materialized-skill summary including unsatisfied required tools

## 4. CLI surface

- [ ] 4.1 Add the Quartermaster `q` command family (install/list/upgrade/uninstall), hosted by the alan CLI in v0, with `--dest` skill-source selection (global public default) and `--force`
- [ ] 4.2 Render the operation report (materialized/updated/skipped with reasons, required tools, missing host capabilities, unknown vocabulary, collisions)
- [ ] 4.3 Wire missing-`required_tools` detection at install time through `skill_availability_issues` so the report and existing inspection surfaces agree

## 5. Tests (per rust-test-placement-contract, synthetic fixtures only)

- [ ] 5.1 Build a synthetic fixture repo mimicking the external shapes: command `.md` with `$ARGUMENTS` + foreign vocabulary, bare portable `SKILL.md` package, shared repo-root helper script referenced by both
- [ ] 5.2 Cover conversion output: frontmatter, preamble placement, verbatim body, `required_tools` emission, unknown-token reporting, `/lib/pkg` helper addressing
- [ ] 5.3 Cover install: store layout, provenance (git and non-git sources), manifest, materialized sidecar provenance, collision warn/skip/force
- [ ] 5.4 Cover upgrade: unchanged no-op, upstream-change re-materialization, local-modification warn/skip/force
- [ ] 5.5 Cover uninstall exactness (manifest-only deletion, diverged-file preservation) and list output
- [ ] 5.6 Cover honest failure: materialized skill with unsatisfied `required_tools` produces availability issues visible through inspection

## 6. Docs, glossary, and spec sync

- [ ] 6.1 Add CONTEXT.md glossary entries: Quartermaster, distribution package, package store, materialization, adapter preamble, package provenance
- [ ] 6.2 Update `docs/skill_authoring.md` / `docs/skills_and_tools.md` operator guidance for the Quartermaster (`q`) surface (non-normative pointers)
- [ ] 6.3 Reverse-sweep `skill-system-contract` for contradictions (one-skill-per-package rule, sidecar stable-keys framing, discovery tolerance, install-channel source resolution)

## 7. Dogfooding run (manual, outside CI)

- [ ] 7.1 Run `q install ~/Developer/github.com/xbtlin/ai-berkshire`; verify the 19 skills materialize, `tools/financial_rigor.py` runs via `/lib/pkg/ai-berkshire/...`, and the report flags `web_search` and Team-orchestration gaps
- [ ] 7.2 Execute a materialized layer-1 skill end-to-end in alan; verify the missing `web_search` capability surfaces visibly instead of silently degrading
- [ ] 7.3 Exercise upgrade after an upstream `git pull` and uninstall; verify manifest exactness
- [ ] 7.4 Record run results and any new gap findings in design.md (Gap findings section) as seeds for the web-access and multi-agent follow-up changes

## 8. Verification

- [ ] 8.1 `just verify` (fmt + lint + test + mock smoke) passes
