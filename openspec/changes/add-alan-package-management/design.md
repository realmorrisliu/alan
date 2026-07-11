# add-alan-package-management — design

## Context

ai-berkshire (`github.com/xbtlin/ai-berkshire`) is the first real external
workload pointed at alan's skill system. It ships the same content in two
forms:

- `skills/*.md` — canonical Claude Code command-style single files: body text
  with `$ARGUMENTS`, no portable frontmatter, references to Claude-only
  surfaces (`WebSearch`, `TeamCreate`/`TaskCreate`, background subagents,
  permission whitelists).
- `codex-skills/<name>/SKILL.md` — generated portable packages with
  `name`/`description` frontmatter plus a hand-written "Codex adapter note"
  preamble mapping Claude-only vocabulary onto Codex capabilities.

Critically, all 19 skills invoke shared repo-root helpers
(`python3 tools/financial_rigor.py ...`). The repository, not the individual
skill, is the unit of distribution.

Current alan state (verified in-code):

- The loader (`crates/agent-engine/src/skills/loader.rs`) accepts any
  `SKILL.md` with `name` + `description` frontmatter, so the generated packages
  are already discoverable if hand-copied. Command-style single files are not.
- `capabilities.required_tools` exists in frontmatter and flows into
  `skill_availability_issues` (`skills/types.rs`), so missing-capability
  detection needs no new runtime mechanism.
- There is no `$ARGUMENTS` substitution in the engine; alan's model is implicit
  invocation / force-select, not slash-commands-with-args.
- skill-system-contract fixes one exported skill per skill package and already
  speaks install vocabulary (install sources, install channels).
- The execution guard's sensitive-read denylist covers the channel alan home
  directories (`~/.alan`, `~/.alan-dev`; `sandbox.rs`
  `sensitive_read_denylist_for_home`) because they hold `host.toml` and the
  secret store. Under canonical `/lib/pkg` addressing this is a feature, not a
  blocker: agents reach package content only through the projection and the
  exec resolver, and any host path that leaks into content fails fast at the
  guard.
- The two upstream source forms overlap completely: all 19 command-style
  skills also exist as generated `codex-skills/*/SKILL.md` packages (whose
  hand-written preamble targets *Codex* vocabulary, not alan's). Naive
  convention-scan would materialize every skill twice under colliding ids.
- The upstream package's documented worst failure (issue #58) is a background
  agent silently losing web access and producing training-data pseudo-research.
  This contract treats prevention of that failure class as first-order.

An earlier draft of this change scoped the surface to per-skill import
(`alan skill install`). It could only warn about the shared `tools/*.py`
dependency, because the distribution unit was wrong. This design supersedes it
with a package layer; the per-skill conversion/adoption primitives survive
unchanged as materialization rules.

## Goals / Non-Goals

**Goals:**

- A supported, repeatable way to adopt an external repository of skill content
  as one **distribution package**, preserving upstream as the source of truth.
- Conversion of Claude Code command-style `.md` files into directory-backed
  alan skill packages during materialization.
- Shared in-repo helpers remain resolvable after install via the store's
  canonical `/lib/pkg` projection in the Alan OS namespace.
- Honest failure: foreign capability requirements become machine-readable
  declarations surfacing through existing availability reporting.
- Exact lifecycle: provenance and a materialization manifest make upgrade
  idempotent and uninstall complete.

**Non-Goals:**

- Registry, semantic version resolution, dependency graphs, lockfiles,
  signing. A package's version is its source commit.
- Web-search capability (follow-up change; see Gap findings).
- Multi-agent orchestration parity with Claude Code Team tools (follow-up).
- Emulating foreign tools or rewriting prompt semantics.
- Vendoring ai-berkshire content into this repository.
- Codex prompt formats (`codex-prompts/`) — marginal value over command `.md`.
- Changing skill discovery or the one-skill-per-package rule.
- Per-agent package visibility via namespace binds (different agents seeing
  different `/lib/pkg` contents) — v0 projects the whole store for all agents;
  the per-agent story is a later slice on the ADR-0029 roadmap.

## Decisions

### D1: A distribution-package layer above skill packages, not inside them

**Distribution package** (the Quartermaster `q` unit) ⊇ N **skill packages** (the
existing contract's unit). Materialization emits ordinary single-skill packages
into existing skill sources, so discovery, loading, exposure, and the
one-skill-per-package rule are untouched. Alternative rejected: multi-skill
skill packages — that reopens a rule skill-system-contract deliberately closed
and forces loader changes for one workload.

### D2: The unit of distribution is the source tree pinned to a commit

`q install <git-url|path>` places a checkout in the package store. The store's
**canonical address is `/lib/pkg/<name>` in the Alan OS namespace** — the
alan-kernel (aP) namespace, a host-agnostic userspace construct, not a host-OS
mount namespace — projected read-only through the existing
host-directory-mounts machinery. Its host backing lives in the
per-install-channel alan home (`~/.alan/pkg/<name>/` stable,
`~/.alan-dev/pkg/` dev): alan-owned state belongs under alan-controlled
directories, and channel isolation is inherited for free. Backing is an
implementation detail that never appears in contracts or generated content —
self-enforced by the alan home's presence on the execution guard's
sensitive-read denylist: only the runtime's projection and exec resolver
reach the backing, so a host path leaking into agent-visible content fails at
the guard instead of silently working (see Context). "Version" = source commit; upgrade =
fetch + re-materialize. Alternatives rejected: per-skill import (wrong unit —
severs shared helpers; superseded draft) and registry-based packaging (no
registry exists and v0 needs none).

### D3: Materialization reuses the conversion/adoption primitives

From the store, alan materializes skill packages into the chosen skill source
(default: the channel-selected global public source — stable
`~/.agents/skills/<skill-id>`, dev `~/.agents-dev/skills/<skill-id>` — per
skill-system-contract's install-channel isolation):

- **Convert**: command-style `.md` without portable frontmatter → generated
  package with derived `name`/`description` frontmatter, versioned adapter
  preamble, body preserved byte-for-byte.
- **Adopt**: directory with valid `SKILL.md` → validate with existing loader
  rules, copy without content edits.

Skill-id collisions with existing unowned packages warn and skip, never
overwrite.

When one package yields the same skill id from both source forms (ai-berkshire
ships all 19 skills both ways), **conversion from the canonical command file
wins** and the duplicate portable package is skipped with a report entry. The
counterintuitive-looking choice is deliberate: the portable duplicates carry a
preamble hand-written for *Codex* surfaces, while conversion applies alan's
own adapter preamble — the canonical body plus our adapter beats someone
else's adapter. The include/exclude scan flags remain the escape hatch.

### D4: Adapter preamble + `required_tools`, never mechanical rewrite

The source body stays verbatim. The injected preamble (one standard, versioned
block after frontmatter):

- defines `$ARGUMENTS` as the user's current request;
- maps known foreign tool vocabulary to alan surfaces where an equivalent
  exists;
- explicitly declares vocabulary with no alan equivalent (web search, Team
  orchestration) **unavailable**, instructing the skill to state the limitation
  instead of improvising;
- resolves upstream-relative helper invocations (e.g. `tools/*.py`) to the
  package's canonical namespace path under `/lib/pkg/<name>/` — never to a
  host path.

Known vocabulary is converter data, versioned with the converter. Unknown
tool-like tokens are never silently mapped; they go to the install report.
Alternatives rejected: mechanical token rewriting (prompt surgery, untraceable
upstream diffs) and verbatim import (reproduces the silent-degradation
failure). Mirrors the adapter-note pattern upstream already validated.

### D5: Missing capabilities fail honestly through existing machinery

Conversion emits `capabilities.required_tools` for recognized vocabulary (e.g.
`web_search`). Missing tools surface as `skill_availability_issues`, which the
runtime already computes; the contract requires visibility at install time
(report) and at exposure time (existing inspection surfaces). No new
enforcement mechanism.

### D6: The `/lib/pkg` projection is the fix for shared helpers

Materialized skills reference in-package helpers at the canonical namespace
path (`/lib/pkg/<name>/tools/...`), which is stable for the life of the
installation and identical on every host. Agent Processes see only the Alan OS
file system; the store projection is how package content enters it. Helper
*execution* (spawning an interpreter on a store file) is the one place the
runtime must bridge: the execution backend resolves `/lib/pkg/<name>/...`
through the mount table to the store backing when spawning host processes — a
deterministic prefix resolution, since the store is one declared
host-directory mount. The guard gets exactly one narrow exception for this:
paths the runtime itself resolved through the store mount are permitted for
spawn, while agent-authored commands referencing the backing directly remain
denied by the sensitive-read denylist — which is what keeps `/lib/pkg` the
only working address. That resolution is runtime machinery, invisible to
contracts and content. Uninstall removes the store entry *and* the
materialized skills together (manifest-driven), so no skill outlives the
helpers it points at. Helpers are not copied per-skill (duplication) and not
left pointing at the user's original clone (breaks when the clone moves).

### D7: Provenance and manifest make the lifecycle exact

The store keeps, per package: provenance (source repository, commit, source
path, converter version) and a manifest of every materialized file with a
content hash. Each materialized skill package's `package.yaml` carries a
`provenance` block naming the owning distribution package. Semantics:

- **upgrade**, unchanged commit + converter → no-op; changed → re-materialize;
- materialized files diverging from manifest hashes (local edits) → warn and
  skip unless `--force`;
- **uninstall** → delete exactly the manifest's files plus the store entry;
- **list** → packages with provenance and materialized-skill summary.

### D8: The surface is Quartermaster's `q` command family, implemented in Rust

Identity and naming are settled by
[ADR-0029](../../../docs/adr/0029-quartermaster-package-management.md):
package management is an Alan OS organ named **Quartermaster**, command `q`
("Q equips agents"), verbs deliberately boring (`q install|list|upgrade|
uninstall`). In v0, before the shell command namespace lands, the family is
hosted by the alan CLI; materialization logic is library code in the existing
skills module space so tests drive it without the CLI, per the skill-authoring
tooling preference.

### D9: This change is slice v0 of the ADR-0029 roadmap

ADR-0029 carries the long arc (further package types, permissions wired to
policy, per-agent package visibility via namespace binds — different Agent
Processes seeing different `/lib/pkg` contents, which requires skill discovery
through the namespace and is the real "profiles" story — reproducibility,
registry/signing, `q` registered in `/bin`). This change promises none of it:
v0 projects the whole store at `/lib/pkg` for all agents and controls
per-agent skill exposure through the existing override mechanisms. The prior
non-goal warning against turning `/mnt` into a package manager
(define-alan-app-service-integration) is respected — the projection lives
under `/lib`, and Plan 9's `/lib` is precisely where data files belong.

## Risks / Trade-offs

- [Vocabulary table drift — ecosystems invent new tool names] → table is
  converter data with a conservative default: unknown tokens are reported,
  never mapped; a table update is a converter-version bump, which triggers
  re-materialization on next upgrade.
- [Adapter preamble subtly changes prompt behavior] → preamble is standard,
  minimal, versioned; body stays verbatim so upstream diffs remain meaningful.
- [`required_tools` names correspond to no alan tool (e.g. `web_search`)] →
  intended honest-failure signal, not an error; loader validation constrains
  token syntax only.
- [Install report warnings ignored, silent degradation returns at runtime] →
  availability issues also surface through existing exposure/inspection paths,
  not only at install time.
- [Skill-id collision between packages or with hand-authored skills] → warn
  and skip by default; the manifest records ownership so collisions are
  detectable both ways.
- [User edits a materialized skill, upgrade wants to replace it] → manifest
  hashes detect divergence; warn-and-skip unless forced; the report tells the
  user to upstream their edit or fork the package.
- [Helper execution needs namespace→backing resolution the bash path may not
  have yet] → the store is a single declared host-directory mount, so
  resolution is a deterministic prefix mapping via the mount table; it is a
  scoped v0 task, not an open-ended exec-through-namespace program. Preambles
  carry only `/lib/pkg` paths, so nothing re-materializes when the backing or
  resolution mechanism changes.

## Gap findings from the ai-berkshire dogfooding run (seeds for follow-up changes)

Recorded here per the change scope; none are implemented in this change.

1. **Web access capability.** alan has no web search/fetch tool; every
   research-grade skill in the workload requires it. Recommended shape per the
   alan worldview: a mountable search/fetch file server (a `searchfs` analog
   to `llmfs`), not a bolted-on host tool. Blocking for layer-2 dogfooding
   (actually producing a research report).
2. **Multi-agent orchestration.** `/investment-team` spawns four parallel
   background analysts plus a coordinating lead. alan's natural expression is
   child Agent Processes with file-based handoff (`child_runs` / delegated
   launch targets exist), but there is no contract for parallel fan-out, join,
   and result aggregation equivalent to Team/TaskCreate.
3. **Background escalation surfacing.** The workload's documented failure mode
   (background agent cannot prompt for permission, degrades silently) maps to
   alan's PolicyEngine `escalate` → Yield path for child processes; how child
   Yields bubble to the user is untested and needs a dedicated pass once
   multi-agent lands.
4. **`$ARGUMENTS` / parameterized invocation.** alan has no argument
   substitution for force-selected skills. The adapter preamble works for
   implicit invocation; whether alan wants first-class skill arguments is an
   open product question, deliberately not answered here.

## Migration Plan

Additive change; no existing behavior is modified. Rollback is removing the
`q` command family and the `provenance` block definition; materialized
skill packages remain valid ordinary skill packages (unknown sidecar fields
are tolerated by the existing contract), losing only lifecycle management.

## Open Questions

- Exact wording of the standard adapter preamble (finalized during
  implementation; versioned so it can evolve).
- How a package names its materializable skills in v0: convention-scan
  (`skills/*.md` command-style, `*/SKILL.md` portable) versus an optional
  package manifest file in the source repo. Default assumption:
  convention-scan with explicit include/exclude flags; a source-side manifest
  can come later without breaking the contract.
