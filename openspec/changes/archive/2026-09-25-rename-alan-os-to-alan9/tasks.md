# Tasks

## 1. Decision and planning

- [x] 1.1 Record ADR-0057, the naming map and migration boundary; verify the ADR links to the owning change and distinguishes accepted naming from pending adoption.
- [x] 1.2 Create proposal, design and product-brand-identity delta; verify they preserve Alan, `alan`, aP and existing identifiers and do not settle input-routing design.

## 2. Documentation implementation

- [x] 2.1 Inventory active `Alan OS` and `Alan Kernel` prose in README, AGENTS.md, current docs and OpenSpec; classify historical quotations and literal identifiers so every remaining old-name occurrence has a reason.
- [x] 2.2 Apply the naming map to active explanatory prose, preserving existing paths, links, spec IDs, dispositions and historical ADR/archive content; review the diff against the inventory.
- [x] 2.3 Inspect existing brand validation. No implementation exists in the repository quality command, scripts, crates, or CI; per user direction, add no checker in this rename and carry the existing product-brand-identity gap as a separate follow-up.

## 3. Verification and delivery

- [x] 3.1 Run `openspec validate rename-alan-os-to-alan9 --strict` and the repository's `just quality` gate after adoption; record results and review that no runtime, storage or identifier migration entered the diff.
- [x] 3.2 Complete PR review and required current-head CI, then merge the documentation implementation; record the reviewed and merged commits.
- [x] 3.3 Sync only the delivered delta into canonical product-brand-identity, verify terminology and strict OpenSpec validation, then archive when implementation and sync are merged.

## Verification Record

- `openspec validate rename-alan-os-to-alan9 --strict` — passed.
- `openspec validate --all --strict` — 65/65 items passed; long-requirement notices are informational.
- `just quality` — passed.
- The change contains documentation/OpenSpec terminology edits only; no runtime, store-path, or machine-identifier migration was made.
- PR #934 reviewed commit `cf88b3e3dc540890734d6d14aa5d0030da816667`; all current-head checks passed and Codex reported no major issues. Merged as `5db591a3b37d20b21ec23862669fe1948555c8b2`.
- The two delivered naming requirements were synced to `product-brand-identity` in PR #935 (`d5d7017cf3095b4ad9da26e7bf9e41fd28d75012`), strict validation passed at 65/65, and PR #935 merged as `53b862b06a0cdb98d37f3110a8d6d8a6aae64ce0`.
- The allowlisted brand checker remains a separate follow-up; it was not added or claimed as implemented. This change is archived at `openspec/changes/archive/2026-09-25-rename-alan-os-to-alan9/` after both prerequisite PRs merged.
