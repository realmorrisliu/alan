## Context

The canonical specification tree is the planning authority for Alan, but 23
capabilities still have generated `TBD` Purpose text. `openspec/config.yaml`
also declares an `archive` artifact rule that the installed `spec-driven`
schema does not recognize, so every instruction lookup emits a warning.

Several active changes were written before the daemon-era removal and the
accepted direct-file architecture. They still cite deleted Console sources or
authorize named compatibility bridges for host attachment and package
projection. Those active documents are executable planning inputs, not inert
history, so leaving them unchanged would authorize new debt.

This change is the prerequisite for `remove-residual-compatibility-shims` and
`remove-legacy-macos-persistence`; both must start from current canonical
contracts. `finish-namespace-native-engine-boundary` begins only after those two
cleanup changes are complete. Archived changes remain immutable.

## Goals / Non-Goals

**Goals:**

- Make every canonical capability self-identifying without changing its
  accepted requirements.
- Make the repository OpenSpec configuration valid for the installed schema.
- Remove stale current-source references from active changes.
- Turn the accepted no-new-bridge policy into an enforceable planning contract.
- Rewrite affected active changes so missing aP, file-tree, package, or binfs
  foundations are explicit prerequisites.

**Non-Goals:**

- Implement any product runtime behavior described by the affected active
  changes.
- Design Alan for macOS attachment to Alan OS.
- Implement `alan-binfs` or the package-management change.
- Rewrite or normalize immutable files under `openspec/changes/archive/`.
- Fold the four cleanup changes into one implementation unit.

## Decisions

### 1. Purpose text summarizes current ownership only

Each placeholder Purpose will be replaced with one or two sentences derived
from that capability's existing requirements. Purpose cleanup must not add,
remove, or reinterpret normative behavior.

Alternative considered: archive and recreate placeholder capabilities. Rejected
because it would manufacture history and risk requirement drift for a metadata
cleanup.

### 2. Archive guidance lives under supported task rules

The unsupported `rules.archive` key will be deleted. Its useful guidance will
be folded into `rules.tasks`: implementation, verification, merge, spec sync,
and archive readiness must be explicit, and only merged/synced changes are
archive-ready. The archive destination convention remains documented in the
task rule rather than pretending `archive` is a schema artifact.

Alternative considered: tolerate the warning until the CLI adds an archive
artifact. Rejected because current configuration must match the current schema,
not a speculative future one.

### 3. Active changes are current authority and must name current files

`add-macos-shell-component-system` will remove references and guard baselines
for the deleted `Views/Console/` remote-control surface. Its remaining
component-system scope will be recalculated from files that exist on the branch
where it is implemented. The change will not retain a historical explanation in
its active requirements or tasks.

Alternative considered: keep the references as historical context. Rejected
because immutable archives provide historical context; active changes should be
actionable against the current tree.

### 4. Missing native foundations block dependent features

The following bridge strategies are prohibited in active plans and production
code:

- callback or DTO façades that become an app-facing authority;
- `ShellContentInstance` or host-action translation layers used in place of
  direct file clients;
- named host compatibility bridges for Groove Master, UPDF, or Alan Voice;
- namespace-bootstrap package projection used in place of a real package/binfs
  mount.

Groove Master, UPDF, and Alan Voice may proceed only when their service trees
and the required Alan for macOS file-client attachment exist. The programmable
client surface may expose packaged commands only after the normal package store
is mounted into the command namespace. Until then, dependent tasks stay blocked
rather than landing an intentionally temporary path.

Alternative considered: permit narrowly named bridges with deletion gates.
Rejected because the project explicitly chose a hard cut, and a bridge would
make the retired host boundary current again.

### 5. Validation is semantic and scoped to current surfaces

A focused repository check will cover canonical specs, OpenSpec config, and
active changes while excluding `openspec/changes/archive/`. It will reject:

- generated Purpose placeholders;
- unknown OpenSpec artifact-rule keys or instruction warnings;
- references to deleted source paths;
- bridge authorization language and known bridge identifiers.

The check will use targeted patterns plus allowlisted explanatory references in
this cleanup change. It will not reject generic uses of words such as
"adapter", "projection", or "compatibility" when they describe a legitimate
boundary.

## Risks / Trade-offs

- [Purpose wording accidentally changes capability meaning] → Keep requirement
  bodies untouched and review each Purpose against its existing headings.
- [Bridge guard becomes a broad word blacklist] → Match named bridge patterns
  and normative authorization phrases, then test both rejected and allowed
  fixtures.
- [Active changes become temporarily blocked] → Make each missing native
  foundation an explicit entry criterion so blocked work is honest and
  discoverable.
- [Config guidance is lost with `rules.archive`] → Preserve the useful merge,
  spec-sync, and archive-readiness rules under the supported `tasks` artifact.

## Migration Plan

1. Replace all 23 canonical Purpose placeholders without touching requirement
   bodies.
2. Move archive-readiness guidance into `rules.tasks` and remove
   `rules.archive`.
3. Update the five affected active changes to current paths and direct-boundary
   entry criteria.
4. Add the focused current-surface validation and positive/negative fixtures.
5. Run strict validation for every active change and the full OpenSpec tree.
6. Land this change before either middle cleanup change begins.

Rollback is a normal revert before dependent changes land. After dependent
changes rely on the clean contracts, rollback must include those dependent
changes rather than restoring bridge authorization selectively.

## Open Questions

None.
