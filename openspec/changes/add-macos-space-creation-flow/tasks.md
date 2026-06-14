# Tasks

Each task is one reviewable commit; keep `test-shell-design-tokens.sh`,
`check-shell-contracts.sh`, `apple-shell-focused-tests`, and
`check-shell-design-tokens.sh` green.

## Implementation

- [x] 1. `ShellSpaceDefaultName.derive(fromWorkingDirectory:)` pure helper +
      script test (leaf, `.git` strip, trailing slash, nil/empty → "").
- [x] 2. Thread default-name derivation into `creatingSpace`: when `title` is
      nil and a `workingDirectory` is present, use the derived name; keep the
      `"Space N"` index fallback otherwise. (Programmatic path now self-names.)
- [x] 3. `createSpace` + `creatingSpace` gain `presentationIconSystemName`
      passthrough so a Space is born with its icon; controller entry
      `createSpaceFromForm(name:iconSystemName:profileID:)`.
- [x] 4. `ShellSpaceCreationForm` SwiftUI view (icon preview, required name
      field, inline curated icon strip with Default/monogram chip, profile
      selector, Create/Cancel) using `ShellType`/`ShellSpacing`/paper-ink
      tokens; independently previewable.
- [x] 5. Sidebar takeover in `ShellSidebarView`: `isCreatingSpace` state;
      titlebar `+` opens the form instead of instant-create; Create/Cancel/Esc
      transitions (reduce-motion aware); select new Space on create.
- [x] 6. Spec delta: amend the titlebar New Space requirement (form, not
      instant; variants still prohibited) + instant programmatic derived-name
      scenario.

## Verification

- [ ] Manual: `+` → form → name+icon+profile → Create → named/iconed Space;
      slider no longer a wall of "S".
- [ ] Programmatic: CLI/worktree Space is named from its directory, not
      "Space N".
- [ ] Build + focused shell tests + default-name script test green; token
      guard green.
- [ ] Screenshot checkpoint: form at 264pt light/dark, empty-name disabled
      Create, icon strip scroll, live preview.

## Review and Archive

- [ ] PR review.
- [ ] Sync spec deltas into `openspec/specs/` after merge.
- [ ] Archive change.
