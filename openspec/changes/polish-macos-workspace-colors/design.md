## Context

Alan's macOS shell already models the right side as a generic content
container: Spaces, Tabs, and PaneSlots own layout, while ContentInstances own
terminal, markdown, settings, and future content-specific rendering. The
current UI implementation mostly follows that model for mounted content:
terminal leaves go through `ShellTerminalLeafView`, while markdown and settings
go through `ShellBoundedContentLeafView`.

Two visual details still contradict the model. `MacShellRootView` paints the
window root with `ShellMaterialBackgroundView(.windowBackdrop)`, which creates a
translucent material/tint/gradient backing rather than a native opaque base.
`TerminalPaneView` also applies `ShellTerminalSurfaceFrame` to the entire pane
canvas, so an empty Space and non-terminal content inherit terminal-surface
chrome even though no terminal content is mounted.

## Goals / Non-Goals

**Goals:**

- Make the primary shell backing surface use an opaque native base color:
  `rgb(1,1,1)` in light appearance and a solid adaptive dark base in dark
  appearance.
- Render an empty selected Space as a workspace-level placeholder, not an empty
  terminal surface.
- Keep the empty placeholder terminal-first by preserving the `New Tab` primary
  action that creates a normal terminal tab in the current Space.
- Keep all rounded rim, clipping, and shadow frame treatment owned by the
  workspace panel and split/container layer. Terminal content keeps its dark
  canvas and terminal-specific runtime controls, but it does not own a surface
  frame.
- Preserve existing markdown/settings content dispatch and avoid creating
  terminal runtimes for non-terminal content.

**Non-Goals:**

- No new content picker, `New...` menu, markdown creation flow, or settings
  shortcut in the empty placeholder.
- No redesign of the sidebar, collapsed floating sidebar, command palette, or
  overlay material system beyond root backing interactions needed for this
  change.
- No dark-mode design overhaul beyond making the affected surfaces adaptive and
  legible.
- No persistence, daemon API, runtime protocol, or content-container model
  changes.

## Decisions

1. **Root backing becomes solid before any future material pass.**

   The implementation should either update the root-window role or introduce a
   narrow root-backing view/token so the main shell window paints an opaque
   adaptive base. Light mode uses `rgb(1,1,1)`. Dark mode uses the existing
   adaptive palette style with a solid dark value. The root backing must not
   depend on `NSVisualEffectView`, wallpaper blending, or a gradient wash in
   this change.

   Alternative considered: keep the material role and tune opacity. That keeps
   the current Arc-like direction but does not satisfy the requested native app
   synchronization or make visual verification deterministic across wallpapers.

2. **The empty Space is a workspace placeholder.**

   The no-tab/no-pane branch should render through a dedicated empty workspace
   placeholder view or equivalent workspace-level branch. It should use adaptive
   text/control tokens and the current `New Tab` command path, but it should not
   be wrapped by terminal background, terminal rim, or terminal surface shadow.

   Alternative considered: special-case light colors inside the existing
   terminal frame. That fixes the screenshot superficially while leaving the
   wrong ownership boundary in place.

3. **Frames belong to workspace containers; terminal styling belongs to terminal content.**

   The outer workspace panel owns the right-side rounded clipping, rim, and
   shadow. Split layout owns internal boundaries and dividers. Terminal leaves
   keep the dark terminal canvas and terminal-only controls, but they do not own
   rounded rim/shadow frame chrome, even in mixed terminal/settings/markdown
   pane trees. Markdown and settings continue through their bounded content
   renderers.

   Alternative considered: keep one terminal-styled frame around the whole pane
   tree because most current content is terminal content. That preserves the old
   look but conflicts with the content-container contract and makes future
   non-terminal surfaces inherit terminal chrome by default.

   Alternative considered: give terminal leaves their own frame only in mixed
   pane trees. That preserves more of the old split-terminal visual treatment,
   but it creates two different ownership models: terminal becomes a
   self-framed content type while settings and markdown are framed by the
   workspace. The selected design keeps frame ownership uniform.

## Risks / Trade-offs

- **Risk:** Removing terminal leaf frame ownership could make mixed split panes
  look too flat if dividers do not carry enough boundary information.
  **Mitigation:** keep the outer workspace panel frame, preserve split divider
  contrast, and verify single-pane terminal, split terminal, mixed content,
  settings, markdown, and empty Space states.

- **Risk:** Changing the `windowBackdrop` role globally may affect Quick
  Terminal Peak or overlays that currently reuse that role.  
  **Mitigation:** audit every `.windowBackdrop` usage and introduce a narrower
  root backing role if an overlay should retain material treatment.

- **Risk:** Dark mode could regress if the solid base is chosen only from the
  light-mode target.  
  **Mitigation:** define the base as an adaptive token and verify both resolved
  color schemes through the existing `ShellAppearanceMode` path.

- **Risk:** Contract tests that currently look for terminal-surface wrappers may
  become stale.  
  **Mitigation:** update tests to assert the new ownership boundary rather than
  the old wrapper location.

- **Risk:** Historical names such as `terminalSurfaceInsets` can keep implying
  terminal-owned workspace chrome after the architecture changes.
  **Mitigation:** rename the workspace panel metrics and view parameters to
  `workspacePanel...` while leaving true terminal runtime/surface-controller
  names unchanged.

## Migration Plan

No persisted state migration is needed. The change is a view/token refactor
inside the macOS client. Rollback is reverting the root backing token/view and
the empty workspace placeholder frame ownership.

## Open Questions

- None for this change. Sidebar and overlay material tuning remains a separate
  future design pass.
