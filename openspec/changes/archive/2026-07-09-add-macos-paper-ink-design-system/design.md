# Paper & Ink Design System — Design Doc

Date: 2026-06-12
Status: Approved direction, pending implementation plan
Scope: Design language document + design token refactor for the macOS shell
(`clients/apple/alan-macos`). Component-level redesigns are explicitly deferred
to follow-up batches.

## Problem

The macOS shell has a sophisticated token layer (`ShellDesignTokens.swift`:
palette, radii, shadows, metrics, materials) and a detailed behavioral UI
contract (`openspec/specs/macos-shell-ui-ux-conformance`), yet the visible
result reads rough and derivative of Arc. Root causes identified:

1. **No design identity.** The shell borrows Arc's navigation forms without an
   organizing visual idea of its own, so every decision defaults to imitation.
2. **No typography or spacing scale.** Font sizes are scattered magic numbers
   (17, 13, 12, 11.6, 11.4); paddings are ad hoc (28, 11, 5). Polish cannot be
   applied or verified consistently without these tokens.
3. **Signal saturation.** The attention color (orange) renders on every Space
   slider icon in observed states, destroying its meaning as a warning color.
4. **Flat palette namespace.** ~40 palette entries are peers with no semantic
   grouping, so nothing stops a new view from picking the wrong role.
5. **Behavioral specs cannot encode taste.** The 1591-line UI contract answers
   "what happens" but not "how to judge an unspecified case."

## Approved Design Decisions

Decided in brainstorming dialogue:

| Decision | Choice |
| --- | --- |
| Core temperament | Calm studio (quiet, low-pressure, work-forward) |
| Signature direction | "Paper & Ink" contrast as the system foundation |
| Agent-presence motion | Narrowed to a phase-2 subsystem (2–3 moments only) |
| Typographic discipline | Demoted to token-layer rules (mono accent track) |
| Delivery shape | Design language doc + token refactor first; component work in follow-up batches |
| Dark mode worldview | "A lamp at night" — metaphor inverts, hierarchy persists |

## The Metaphor

**By day, Alan is an ink well in a bright studio.** A light, paper-like
workshop chrome surrounds a dark, precise terminal surface. This inverts the
terminal-app norm (dark-on-dark everywhere) and is already half-present in the
code: `ShellPalette.terminal` is dark even in light mode.

**By night, the metaphor inverts to a lamp in a dark room.** Chrome sinks
below the terminal in luminance; the terminal surface becomes the light
source.

Both worldviews share one invariant: **the working surface is where the light
falls.** Visual resources (contrast, light, shadow, saturation) always favor
the terminal.

## The Four Principles (with decision tests)

Each principle ships with a test question. The tests are the point: they
resolve cases the behavioral spec does not cover.

1. **Ink is the focus.** All visual resources tilt toward the terminal
   surface.
   *Test: does this change make the eye land on the terminal more easily, or
   does it pull the eye into chrome?*
2. **Paper recedes.** Chrome uses only cool, low-saturation neutrals.
   Decoration without information content is removed.
   *Test: if this element were deleted, what information would the user lose?
   No answer → delete it.*
3. **Signals are scarce.** Color carries semantics. The action color (orange)
   appears only when the user must act. Agent activity is expressed through
   luminance (breathing), never hue.
   *Test: if every eligible signal lit up at once, would the screen still be
   calm?*
4. **Mono is Alan's accent.** Machine facts — paths, branches, process names,
   counts, key hints — render in the mono track at small sizes. Human-facing
   copy renders in SF Pro. Terminal DNA permeates the paper.
   *Test: is this string a machine fact or human language?*

## Signature Detail: The Well Rim

The boundary between paper and ink is the screenshot-recognizable element.
Specification to be detailed in the design language doc:

- Rim highlight on the top inner edge of the terminal surround.
- Contact shadow on the bottom outer edge.
- Light source fixed at top-left across the whole shell (existing shadow
  tokens already lean this way: negative x offsets).
- Existing `workspacePanelRim` treatment in `TerminalPaneView.swift` is the
  starting point; it gets promoted to a named, documented token set
  (`wellRim`, `wellShadow`) instead of inline gradients.

## Deliverable 1: Design Language Document

Location: `docs/design/design-language.md` (English, matching repo docs
convention). Contents:

1. The metaphor (day/night worldviews, the invariant).
2. The four principles, each with its decision test.
3. The well-rim signature specification.
4. Type and spacing scale rationale (roles, not numbers-first).
5. Signal semantics table: which states may use `signalAction`, which use
   luminance, which stay silent.
6. Relationship to OpenSpec: the design language governs judgment; the UI
   conformance spec governs behavior. Conflicts resolve toward the spec for
   behavior, toward this doc for appearance.

## Deliverable 2: Token Refactor (`ShellDesignTokens.swift`)

Additive and reorganizing only; **no view-level visual changes in this batch**
except the dark-mode palette rework noted below.

### 2a. New `ShellType`

Role-based, integer sizes, two tracks:

- Pro track (SF Pro): `display` (17), `heading` (13), `row` (12), `caption`
  (11), with defined weights per role.
- Mono track (SF Mono): `monoLabel` (11), `monoCaption` (10).
- Fractional sizes (11.6, 11.4) are eliminated; each maps to the nearest role.

### 2b. New `ShellSpacing`

4pt-based semantic scale: `hair` (2), `tight` (4), `control` (8), `row` (12),
`section` (16), `panel` (24). Scattered literals migrate to these names in
follow-up view migrations.

### 2c. `ShellPalette` reorganization into three domains

- **Paper domain** (`paper*`): current sidebar/window/workspace family.
- **Ink domain** (`ink*`): current terminal/terminalSoft family, plus new
  `wellRim` / `wellShadow` tokens for the signature boundary.
- **Signal domain** (`signal*`): `attention` renames to `signalAction` with
  narrowed semantics ("the user must act"); new `signalBreath` luminance
  variable reserved as the phase-2 agent-activity interface.

### 2d. Dark-mode value rework (intended visual change 1 of 2)

Current dark values contradict the lamp worldview: chrome (sidebar 0.071) is
*lighter* than the terminal (0.050). Rework so chrome sinks to ~0.04 family
and the terminal lifts to a slightly luminous value, restoring
terminal-as-light-source hierarchy. Exact values tuned during implementation
against screenshots.

### 2d-bis. Light-mode paper separation (intended visual change 2 of 2)

Root cause (confirmed in history): the workspace-colors pass (`f7adad3`)
replaced the root window material with opaque pure white and deferred
material treatment to a later dedicated pass; the pinned sidebar paints no
background of its own (only the floating sidebar overlay carries
`.sidebarGlass`), so the default state renders sidebar rows directly on pure
white and the white selected-row card has nothing to lift off.

Fix: move `ShellMaterialBackgroundView(.sidebarGlass)` to the window root in
`MacShellRootView` body (replacing `ShellPalette.rootBacking`), giving the
whole chrome — sidebar column and workspace margins — one continuous paper
material surface. The sidebar-only backing approach was rejected at the
maintainer gate: placing the material only behind the sidebar column while
the root stayed opaque white produced a hard gray-left / white-right seam,
splitting the chrome rather than unifying it. Removing the sidebar-only
`.background` block avoids double-stacking material (which would recreate the
seam as a darker sidebar). Then deepen the paper values (stronger cool scrim,
slightly darker sidebar value, more solid selected-row card). Principle: in
light mode, pure white belongs only to raised surfaces. Exact values tuned
during implementation against screenshots.

### 2e. Migration strategy

Old palette names remain as deprecated aliases pointing at new tokens. View
files migrate per-file in follow-up batches; no big-bang diff.

## Deliverable 3: Governance Guards

1. **Lint script** (`scripts/`): grep-level check failing CI/local runs when
   `Views/` (and other shell UI directories) contain raw `Font.system(size:`,
   raw RGB literals, or non-whitelisted numeric paddings. Ships with a
   baseline file listing existing violations (exempt until their file is
   migrated); only new or worsened violations fail. New code is forced
   through tokens.
2. **Screenshot state matrix** (justfile target): renders six canonical
   states — empty space, single tab, split panes, multi-Space, dark mode,
   reduced transparency — for eyeball review on every visual change. This
   gives the existing "UI conformance is verified visually" requirement an
   executable tool.

## Explicitly Out of Scope (ordered follow-up batches)

1. **Attention-saturation bug investigation** — why `strongestAttention` is
   non-idle for visibly empty Spaces (likely small fix; can run parallel to
   this batch).
2. **Empty-state redesign** — depends on new tokens. Current
   `ShellEmptyWorkspacePlaceholder` (left-aligned composition, hover-role
   button background, cold copy) is the known offender.
3. **Space identity system** — per-Space low-saturation ink color + optional
   icon; depends on signal-domain definitions.
4. **Agent breathing motion** — depends on `signalBreath`; highest taste
   risk, deliberately last.

## Verification

This batch is done when:

- The project builds and existing UI tests pass.
- Dark-mode screenshots confirm the lamp hierarchy (chrome below terminal in
  luminance).
- All other appearances are visually unchanged (alias strategy guarantees
  this; spot-check via the screenshot matrix).
- The lint script passes with the recorded baseline and fails on a
  deliberately seeded new violation.
