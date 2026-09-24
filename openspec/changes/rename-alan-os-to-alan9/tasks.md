# Tasks

## 1. Decision and planning

- [x] 1.1 Record ADR-0057, the naming map and migration boundary; verify the ADR links to the owning change and distinguishes accepted naming from pending adoption.
- [x] 1.2 Create proposal, design and product-brand-identity delta; verify they preserve Alan, `alan`, aP and existing identifiers and do not settle input-routing design.

## 2. Documentation implementation

- [x] 2.1 Inventory active `Alan OS` and `Alan Kernel` prose in README, AGENTS.md, current docs and OpenSpec; classify historical quotations and literal identifiers so every remaining old-name occurrence has a reason.
- [x] 2.2 Apply the naming map to active explanatory prose, preserving existing paths, links, spec IDs, dispositions and historical ADR/archive content; review the diff against the inventory.
- [ ] 2.3 Inspect existing brand validation and adjust only if necessary to permit lowercase alan9 as system branding; verify its existing checks still reject invalid Alan product branding.

## 3. Verification and delivery

- [x] 3.1 Run `openspec validate rename-alan-os-to-alan9 --strict` and the repository's `just quality` gate after adoption; record results and review that no runtime, storage or identifier migration entered the diff.
- [ ] 3.2 Complete PR review and required current-head CI, then merge the documentation implementation; record the reviewed and merged commits.
- [ ] 3.3 Sync only the delivered delta into canonical product-brand-identity, verify terminology and strict OpenSpec validation, then archive when implementation and sync are merged.

## Verification Record

- `openspec validate rename-alan-os-to-alan9 --strict` — passed.
- `openspec validate --all --strict` — 65/65 items passed; long-requirement notices are informational.
- `just quality` — passed.
- The change contains documentation/OpenSpec terminology edits only; no runtime, store-path, or machine-identifier migration was made.
