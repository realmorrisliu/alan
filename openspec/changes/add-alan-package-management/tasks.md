# add-alan-package-management — tasks

## 1. Quartermaster resolution authority (retire multi-source enumeration)

- [ ] 1.1 Define the Q resolution interface: given an agent context, return its resolved skill capability set from registered providers
- [ ] 1.2 Define the three provider kinds (pre-installed / local-source / distribution) and a provider registry the resolver reads
- [ ] 1.3 Reseed built-in first-party skills as pre-installed Q packages (idempotent seed into the store on first run); keep the skill package contract unchanged
- [ ] 1.4 Register `AgentRoot` / workspace / user `.agents/skills/` as local-source providers at their existing locations (no copy into the global store), preserving channel scoping
- [ ] 1.5 Replace both discovery bypasses: retire `package_dirs_for_roots` (`agent_definition.rs`) and the `builtin_capability_packages()` append in `ResolvedCapabilityView::from_package_dirs`; feed the view only from Q resolution
- [ ] 1.6 Regression: existing built-in, agent-root, and workspace skills still resolve and expose identically through Q, with no direct built-in injection remaining (no behavior change beyond source)

## 2. Package store and lifecycle model (library, in the agent-engine skills module space)

- [ ] 2.1 Define the data model: distribution package with a channel-unique package id (normalized source basename or explicit `--name`), store backing under the channel alan home (`~/.alan/pkg/`, dev `~/.alan-dev/pkg/`), provider registry entry, provenance record, materialization manifest with content hashes and selected skill roots, operation report types
- [ ] 2.2 Implement source fetch: git URL clone / local directory copy into the store; compute the source revision token (git commit, or content fingerprint of the source tree for non-git sources)
- [ ] 2.3 Implement materializable-skill scanning by convention (command-style `*.md`, portable `*/SKILL.md`) with include/exclude flags

## 3. Materialization primitives

- [ ] 3.1 Implement command-`.md` conversion: derive `name`/`description` frontmatter, inject the versioned adapter preamble, preserve the body verbatim
- [ ] 3.2 Implement the known foreign-vocabulary table (converter data, versioned) and the scan that emits `capabilities.required_tools` for real tool equivalents, typed `runtime_capability` dependencies for unsupported Alan surfaces, and unknown-token report entries
- [ ] 3.3 Emit canonical `/lib/pkg/<package-id>/...` namespace paths in the adapter preamble for upstream-relative helper references (e.g. repo-root `tools/*.py`); no host paths in generated content
- [ ] 3.4 Implement portable-package adoption: validate via existing loader rules, register in place without content edits
- [ ] 3.5 Store the store entry in two layers (`source/` verbatim checkout, `materialized/` generated packages + manifest); project the merged content view while resolving only manifest-selected skill roots
- [ ] 3.6 Implement skill-id collision detection against provider ownership: warn-and-skip even with `--force`; never transfer another package's ownership implicitly
- [ ] 3.7 Implement duplicate-source precedence: same skill id from command file and portable package → convert the command file, omit the portable duplicate from manifest-selected skill roots, record the choice in the report

## 4. Lifecycle operations and projection

- [ ] 4.1 Implement install: fetch + materialize + provider/provenance/manifest write, atomic enough that a failed install leaves no partial skills
- [ ] 4.2 Implement upgrade: no-op only on unchanged source revision token (git commit, or re-computed content fingerprint for non-git local sources) + converter; re-materialize on change; manifest-hash divergence warn/skip/force
- [ ] 4.3 Implement uninstall: check divergence before removing the store entry — relocate diverged files out of the entry (never delete with it) unless forced — then delete manifest-listed files, the store entry, and provider registration
- [ ] 4.4 Project the store read-only at `/lib/pkg` via the host-directory mount machinery on install/boot; verify Agent Processes read package content through the namespace
- [ ] 4.5 Implement execution-backend resolution of `/lib/pkg/...` paths to store backing for spawned helper processes (deterministic prefix mapping via the mount table), with the narrow guard exception for runtime-resolved store paths; verify direct agent references to the backing stay denied
- [ ] 4.6 Implement list: installed packages with provenance and resolved-skill summary including unsatisfied required tools

## 5. CLI surface

- [ ] 5.1 Add the Quartermaster `q` command family (install/list/upgrade/uninstall), hosted by the alan CLI in slice 1, with `--name` for package-id disambiguation and `--force` for package-owned divergence only
- [ ] 5.2 Render the operation report (materialized/updated/skipped with reasons, required tools, missing host capabilities, unknown vocabulary, collisions)
- [ ] 5.3 Wire missing tool and runtime-capability dependency detection at install time through `skill_availability_issues` so the report and existing inspection surfaces agree

## 6. Tests (per rust-test-placement-contract, synthetic fixtures only)

- [ ] 6.1 Cover Q resolution: built-in pre-installed + agent-root local-source + distribution all resolve through one authority; no independent directory scan remains
- [ ] 6.2 Build a synthetic fixture repo: command `.md` with `$ARGUMENTS` + foreign vocabulary, bare portable `SKILL.md`, shared repo-root helper referenced by both
- [ ] 6.3 Cover conversion output: frontmatter, preamble placement, verbatim body, tool-vs-runtime-capability dependency emission, PATH cannot satisfy unsupported surfaces, unknown-token reporting, `/lib/pkg` helper addressing
- [ ] 6.4 Cover install: package-id derivation and collision rejection, store layout (source/ + materialized/), provenance (git and non-git sources), manifest-selected skill roots, cross-owner collision rejection even with force, duplicate-source precedence
- [ ] 6.5 Cover upgrade: unchanged no-op, upstream-change re-materialization, local-modification warn/skip/force
- [ ] 6.6 Cover uninstall exactness (manifest-only deletion, diverged-file preservation) and list output
- [ ] 6.7 Cover honest failure: resolved skill with an unsatisfied runtime-capability dependency produces availability issues visible through inspection

## 7. Docs, glossary, and spec sync

- [ ] 7.1 Add CONTEXT.md glossary entries: Quartermaster, distribution package, package store, skill provider, materialization, adapter preamble, package provenance
- [ ] 7.2 Update `docs/skill_authoring.md` / `docs/skills_and_tools.md` for the Q resolution model and the `q` surface (non-normative pointers)
- [ ] 7.3 Reverse-sweep `skill-system-contract` for contradictions with the four MODIFYed requirements — especially requirements that assume the old multi-source discovery (exposure resolution, prompt catalog, availability gates, management surfaces)

## 8. Dogfooding run (manual, outside CI)

- [ ] 8.1 Run `q install ~/Developer/github.com/xbtlin/ai-berkshire`; verify the 19 skills resolve through Q, `tools/financial_rigor.py` runs via `/lib/pkg/ai-berkshire/...`, and the report flags unsatisfied `web_access` and `multi_agent_orchestration` runtime capabilities
- [ ] 8.2 Execute a resolved layer-1 skill end-to-end in alan; verify the missing `web_access` runtime capability surfaces visibly instead of silently degrading
- [ ] 8.3 Exercise upgrade after an upstream `git pull` and uninstall; verify manifest exactness
- [ ] 8.4 Record run results and any new gap findings in design.md (Gap findings section) as seeds for the web-access and multi-agent follow-up changes

## 9. Verification

- [ ] 9.1 `just verify` (fmt + lint + test + mock smoke) passes
