# Space Creation Flow — Design

Date: 2026-06-13
Status: Approved direction, pending implementation plan

## Decisions (from dialogue)

- Manual Space creation gets a deliberate flow; programmatic stays instant.
- Presentation: **in-sidebar takeover** (Arc-style). Alan's sidebar (~264pt) is
  not narrower than Arc's (~240pt), so width is not an obstacle.
- Manual form **requires a name** (Create disabled while empty) — this is the
  source-level fix for the wall-of-"S".
- Fields: **name + icon + terminal profile**. No color/theme.
- Programmatic path derives a default name from the working directory.

## Two creation paths

```
Titlebar "+"  ──▶  sidebar switches to ShellSpaceCreationForm
                    (name required, icon, profile) ──▶ Create ──▶ createSpace(...)

CLI / worktree / API  ──▶  createSpace(title: derivedDefault, ...) instantly
                            (no form), title from working directory leaf
```

Both converge on the existing `createSpace` mutation; only the manual path
shows UI and only it requires a typed name.

## Components (isolated, testable)

### `ShellSpaceDefaultName` (pure helper)
`static func derive(fromWorkingDirectory path: String?) -> String`
- nil/empty → `""` (caller substitutes `"Space N"` via existing index logic).
- else: last path component; strip a trailing `.git`; trim slashes; if the
  leaf is empty (e.g. "/"), return `""`.
- Script-testable in isolation (no app-target deps).

The `"Space N"` fallback stays where it is (`creatingSpace`), so when the
derived name is empty the current numbering is preserved.

### `ShellSpaceCreationForm` (SwiftUI view)
- Inputs: available terminal profiles, the curated icon list
  (`ShellSpaceIconCatalog.curatedSymbols`), and an initial focus signal.
- State: `name: String`, `selectedIcon: String?` (nil = monogram default),
  `selectedProfileID: String?`.
- Output: `onCreate(name:iconSystemName:profileID:)` and `onCancel()` callbacks.
- Layout (top→bottom), all on the sidebar paper surface using `ShellType` /
  `ShellSpacing` / paper-ink tokens:
  - Icon preview tile: shows `ShellSpacePresentationIcon.resolve(systemName:
    selectedIcon, title: name)` — monogram of the typed name until a symbol is
    picked. Reuses the shipped resolver and the slider's render logic.
  - Name field: `TextField` with placeholder "Space name…"; first responder on
    appear; Return triggers Create when enabled.
  - Icon strip: a horizontally scrolling row of curated symbols plus a
    leading "Default" (monogram) chip; selecting toggles `selectedIcon`
    (Default sets it nil). Scrolls because the sidebar is narrow.
  - Profile selector: a `Menu`/picker over `TerminalProfileStore` profiles,
    mirroring the existing Space-context-menu profile control.
  - Footer: Create (primary, disabled while `name` is blank) + Cancel.
- The view is independently previewable: feed it sample profiles and verify
  layout at 264pt, light/dark, empty vs typed name.

### Sidebar takeover (`ShellSidebarView`)
- Add `@State private var isCreatingSpace = false`.
- The titlebar `+` action sets `isCreatingSpace = true` instead of calling
  `createTerminalSpace()`.
- When true, the sidebar body renders `ShellSpaceCreationForm` in place of the
  Space slider + tab list, with the same short transition style used for Space
  paging (respect reduce-motion).
- `onCreate` calls a new controller method, then sets `isCreatingSpace = false`
  and selects the new Space. `onCancel` / Esc just clears the flag.

### Controller + mutation
- `createSpace` gains `presentationIconSystemName: String? = nil`, threaded
  into `creatingSpace` so the Space is born with its icon (no second mutation).
- `creatingSpace` default-name line changes from `title ?? "Space \(index)"`
  to `title ?? <derived-or-"Space N">`: callers that pass a `workingDirectory`
  get `ShellSpaceDefaultName.derive(...)` applied when `title` is nil; the
  index fallback remains for the no-cwd case.
- A controller entry the form calls, e.g.
  `createSpaceFromForm(name:iconSystemName:profileID:)`, which forwards to
  `createSpace`.

## Spec amendment

`macos-shell-ui-ux-conformance`, "Stable compact controls" scenario currently
says the titlebar New Space button "directly creates a standard new Space
instead of opening a menu of Space variants." Amend to: the button opens the
in-sidebar creation form; a menu of Space *variants/types* remains prohibited;
the form creates a single standard Space. Add a scenario for the instant
programmatic path with a derived default name.

## Testing

- `ShellSpaceDefaultName.derive`: leaf extraction, `.git` strip, trailing
  slash, empty/nil → "".
- Existing focused shell tests stay green; sidebar presentation test updated if
  it asserts the `+` action's old instant-create behavior.
- Token guard/baseline: the new view uses tokens only; ratchet if any literals
  are introduced (aim for none).
- Screenshot checkpoint: form at 264pt (light/dark), empty-name disabled
  Create, icon strip scroll, live monogram→symbol preview, post-create slider
  shows the named/iconed Space.

## Why not …

- **Modal sheet / popover**: rejected in favor of the Arc-style in-sidebar
  takeover the user chose; width is not a constraint.
- **Prefilling a default name in the manual form**: rejected — an empty,
  required name is what forces distinct names and kills the wall-of-"S";
  manual Spaces have no working directory to derive from anyway.
- **Reusing the context-menu icon picker as the form control**: it is a menu,
  not an inline control; only the symbol list + resolver are reused.

## Correction (gate feedback 2026-06-13): draft target in the slider

The first implementation replaced the whole sidebar (slider + tab list) with
the form. That is wrong. Arc keeps the Space slider visible, appends a new
**draft** Space target at the end (selected), and hosts the name/icon/profile
inputs in the tab-list region below. Corrected design:

- **Trigger** unchanged: titlebar `+` → `host.isPresentingSpaceCreation = true`.
- **Slider stays visible.** While creating, `ShellSidebarSpaceSlider` renders
  one extra trailing **draft target**, treated as the selected target. Its
  glyph is `ShellSpacePresentationIcon.resolve(systemName: draftIcon,
  title: draftName)` — so it shows the live monogram of what the user types and
  switches to the chosen symbol. The draft is not a real `ShellSpace`; no Space
  or terminal is created until Create (avoids spawn-then-kill on Cancel).
- **Only the tab-list region becomes the form.** The content region shows
  `ShellSpaceCreationForm` while creating, else `spaceContentPager`. The slider
  is never replaced.
- **Form slims down**: the standalone preview tile is removed (the slider draft
  target is the preview now). Form = required name field + inline curated icon
  strip + profile selector + Create/Cancel.
- **Shared draft state** lives in `ShellSidebarView` (`@State spaceDraftName`,
  `spaceDraftIcon`, `spaceDraftProfileID`), reset on
  `onChange(of: host.isPresentingSpaceCreation)`. The form binds to it (so the
  slider updates live); `fixedSpaceSlider` passes a draft descriptor down to
  `ShellSidebarSpaceSlider`.
- **Commit/cancel**: Create → `createSpaceFromForm(name: spaceDraftName, …)`;
  Cancel/Esc → clear the flag, draft target disappears, prior selection
  restored. `createSpaceFromForm` already selects the new real Space.
- **Main workspace area** during creation shows the draft's empty-space
  placeholder (no tabs yet), consistent with the empty-Space placeholder
  already shipped.
