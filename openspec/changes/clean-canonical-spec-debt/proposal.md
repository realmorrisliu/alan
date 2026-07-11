## Why

Alan's canonical OpenSpec tree still contains generated placeholder purposes, an
unsupported `archive` rule that warns on every artifact-instruction lookup, and
active changes that cite deleted macOS Console paths or authorize temporary
compatibility bridges. Leaving those planning surfaces active would let retired
architecture and new bridge debt re-enter the implementation immediately after
the retired host-service clean break.

## What Changes

- Replace all 23 generated canonical-spec Purpose placeholders with concise,
  capability-specific purpose statements derived from their requirements.
- Remove the unsupported `rules.archive` entry from `openspec/config.yaml` while
  preserving merge, spec-sync, and archive-readiness guidance under supported
  artifact rules.
- Remove active-change references to deleted Console/remote-control source paths
  and update any baselines or scope statements that still count those paths.
- **BREAKING**: prohibit new callback, DTO, ContentInstance, host-action, or
  namespace-bootstrap compatibility bridges in active Alan work. A dependent
  feature waits for its direct aP, file-tree, package, or binfs boundary instead
  of landing a bridge with a future deletion gate.
- Rewrite the Groove Master, UPDF, Alan Voice, and programmable-client active
  changes so direct file consumption and a normal package/binfs mount are entry
  criteria, not follow-up cleanup tasks.
- Add focused validation that canonical purposes are not placeholders, OpenSpec
  config emits no unknown-artifact warnings, and active changes do not refer to
  deleted source paths or authorize a new compatibility bridge.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `documentation-governance`: require complete canonical Purpose text, valid
  OpenSpec artifact-rule keys, current active-change references, and validation
  against newly authorized compatibility bridges.
- `alan-app-service-integration`: require Alan for macOS and other clients to
  consume the authoritative aP/file boundary directly; a missing host attachment
  or package mount is a dependency, not permission to add a temporary bridge.

## Impact

- Canonical specs under `openspec/specs/`, `openspec/config.yaml`, and OpenSpec
  validation tooling.
- Active changes `add-macos-shell-component-system`,
  `define-groove-master-alan-app`, `define-updf-product-umbrella`,
  `add-alan-voice-mvp`, and `define-alan-programmable-client-surface`.
- No production runtime behavior changes in this change; it makes future entry
  criteria and planning authority match the accepted clean-break architecture.
- No change to immutable files under `openspec/changes/archive/`.
