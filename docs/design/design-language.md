# Alan Design Language: Paper & Ink

This document governs **appearance judgment** for the Alan macOS shell. The
behavioral UI contract lives in
`openspec/specs/macos-shell-ui-ux-conformance/spec.md`. When they touch the
same area, the OpenSpec contract wins on behavior; this document wins on
appearance. Cases neither covers are resolved by the principle tests below.

## The Metaphor

**By day, Alan is an ink well in a bright studio.** A light, paper-like
workshop chrome surrounds a dark, precise terminal surface. Terminal apps
almost universally choose dark-on-dark; Alan's light studio wrapping a dark
well is its recognizable silhouette.

**By night, the metaphor inverts to a lamp in a dark room.** Chrome sinks
below the terminal in luminance; the terminal surface becomes the light
source.

Both worldviews share one invariant: **the working surface is where the light
falls.** Contrast, light, shadow, and saturation always favor the terminal.

## Principles and Decision Tests

1. **Ink is the focus.** All visual resources tilt toward the terminal
   surface.
   *Test: does this change make the eye land on the terminal more easily, or
   does it pull the eye into chrome?*

2. **Paper recedes.** Chrome uses only cool, low-saturation neutrals.
   Decoration without information content is removed.
   *Test: if this element were deleted, what information would the user lose?
   No answer → delete it.*

3. **Signals are scarce.** Color carries semantics. The action color appears
   only when the user must act. Agent and command activity is expressed
   through luminance, never hue.
   *Test: if every eligible signal lit up at once, would the screen still be
   calm?*

4. **Mono is Alan's accent.** Machine facts — paths, branches, process names,
   counts, key hints — render in the mono track at small sizes. Human-facing
   copy renders in SF Pro.
   *Test: is this string a machine fact or human language?*

## Signature Detail: The Well Rim

The paper/ink boundary is the screenshot-recognizable element. The terminal
surround is treated as a physical well edge:

- Rim highlight (`ShellInk.rimHighlight`) on the top inner edge.
- Contact shadow (`ShellInk.rimShadowLine` plus `ShellShadows.workspacePanel`)
  on the bottom outer edge.
- The implied light source is fixed at top-left across the whole shell
  (existing shadow tokens already use negative x offsets; keep that).
- No decorative glows, colored borders, or per-pane card treatment.

## Type Scale (`ShellType`)

Two tracks, integer sizes only. Fractional sizes are forbidden.

| Role          | Track   | Size | Typical use                              |
| ------------- | ------- | ---- | ---------------------------------------- |
| `display`     | SF Pro  | 17   | Empty-state titles, onboarding moments   |
| `heading`     | SF Pro  | 13   | Space titles, settings section heads     |
| `row`         | SF Pro  | 12   | Sidebar rows, buttons, primary labels    |
| `caption`     | SF Pro  | 11   | Secondary lines, accessories             |
| `monoLabel`   | SF Mono | 11   | Branch, path, process, counts            |
| `monoCaption` | SF Mono | 10   | Dense machine detail, key hints          |

Weights stay per-context (medium default, semibold for selection/emphasis);
the scale governs sizes, not weights.

## Spacing Scale (`ShellSpacing`)

4pt base: `hair` 2, `tight` 4, `control` 8, `row` 12, `section` 16,
`panel` 24. New layout code uses these names; raw numeric paddings beyond the
recorded baseline fail `scripts/check-shell-design-tokens.sh`.

## Color Domains

- **Paper (`ShellPaper`)** — chrome surfaces and chrome foregrounds. Cool,
  low-saturation neutrals.
- **Ink (`ShellInk`)** — the terminal surface family and the well rim.
- **Signal (`ShellSignal`)** — meaning-bearing color, governed by the table
  below.

`ShellPalette` names remain as compatibility aliases during view migration;
new code uses domain names.

## Signal Semantics

| State                                            | Treatment                                  |
| ------------------------------------------------ | ------------------------------------------ |
| Agent/command blocked on user (input, approval)  | `ShellSignal.action` (the only orange)     |
| Failure requiring user intervention              | `ShellSignal.action`                       |
| Agent or long command running                    | Luminance only (`breathLuminanceDelta`)    |
| Keyboard focus, scrub preview                    | `ShellSignal.focus` (indigo)               |
| Success, completion, idle, title changes         | Silent — no color, no badge                |

Anything not in this table defaults to **silent**. Adding a new colored state
requires updating this table first.

## Light Mode: White Is Elevation

Paper carries a perceptible cool gray value; pure white is reserved for
raised surfaces — the selected tab row, the selected Space pill, the
workspace panel. If a resting chrome surface renders as white, the paper has
washed out and selection contrast has been spent for free.

## Dark Mode: The Lamp Hierarchy

In dark appearance every paper surface must sit **below** the ink surface in
relative luminance (asserted by `test-shell-design-tokens.sh`). The terminal
is the brightest large surface on screen; chrome reads as the dark room
around it.
