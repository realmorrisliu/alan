## Why

Manual Space creation is too casual: the titlebar `+` calls
`createTerminalSpace()` with no title, so every manually-created Space is named
`"Space N"` (`ShellStateMutations.swift:272`). With the new monogram identity,
that means every such Space monograms to "S" — a wall of identical "S" targets
in the slider. The root cause is not the monogram; it is that Spaces are born
without a deliberate name or icon. Arc solves this with a real creation flow
(name + icon + profile) so Spaces are distinct from birth. Alan should too,
while keeping programmatic/CLI creation instant.

## What Changes

- **In-sidebar creation form (manual path).** The titlebar `+` no longer
  creates instantly; it switches the sidebar content to a creation form
  (Arc-style takeover, short animation; `+` again / Esc / Cancel returns).
  The form: an icon preview tile, a name field (placeholder "Space name…",
  Create disabled while empty so a real name is required), an inline curated
  icon strip (first entry is the name-derived monogram default), a terminal
  Profile selector, and Create / Cancel.
- **Live icon preview.** The preview tile shows the name's monogram until a
  symbol is chosen, then the symbol — the user sees the Space's identity as
  they build it.
- **Smart default name (programmatic path).** CLI / worktree / API creation
  stays instant (no form) but derives the default name from the working
  directory leaf / repo name (stripping `.git`), falling back to `"Space N"`
  only when no working directory is available.
- **Creation parameters.** `createSpace` gains a `presentationIconSystemName`
  passthrough so the chosen icon is set atomically at creation (reusing the
  curated symbol list + monogram resolution already shipped; only an inline
  icon-strip control is new — the existing icon picker is a context menu, not
  reused as UI).

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `macos-shell-ui-ux-conformance`: the "titlebar New Space button directly
  creates a standard new Space instead of opening a menu of Space variants"
  requirement is amended — `+` opens the in-sidebar creation form; a menu of
  Space *variants/types* remains prohibited; programmatic creation remains
  instant with a derived default name.

## Impact

- `clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift` — sidebar
  takeover state (`creationMode`) + the new `ShellSpaceCreationForm` view and
  inline icon strip.
- `clients/apple/alan-macos/ShellHostController.swift` /
  `Models/Shell/ShellStateMutations.swift` — `createSpace` icon passthrough;
  default-name derivation from working directory.
- A `ShellSpaceDefaultName.derive(fromWorkingDirectory:)` pure helper
  (script-testable).
- Spec delta + screenshot checkpoint.

## Out of Scope

- Per-Space color/theme (deliberately dropped earlier).
- Editing name/icon of an existing Space beyond the already-shipped context
  menu (rename + Space Icon submenu).
- A full SF Symbol browser (curated set only).
- Changing the programmatic creation call sites' behavior beyond the default
  name (they keep their current launch semantics).
