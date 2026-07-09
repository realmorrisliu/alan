# Paper & Ink Design System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish the Paper & Ink design identity as a written design language plus a refactored token layer (`ShellType`, `ShellSpacing`, paper/ink/signal palette domains, dark-mode lamp hierarchy) with lint and screenshot guards.

**Architecture:** All token additions land in the existing `ShellDesignTokens.swift` (no new app-target files, so no Xcode project membership changes). Old `ShellPalette` names become aliases to new domain tokens, so view files keep compiling unchanged; views migrate per-file in follow-up changes. One deliberate view-file exception: `MacShellRootView.swift` regains the pinned sidebar's material backing (Task 4b), fixing the root cause of the washed-out light chrome. Raw RGB tuples for the surfaces involved in the dark-mode rework move into a testable `ShellSurfaceValues` layer so the lamp hierarchy is assertable in a script test. Guards follow existing repo patterns (`scripts/check-*.sh` + justfile recipes; Swift assertions via `swiftc` script tests).

**Tech Stack:** SwiftUI/AppKit (macOS), bash scripts, just, ScreenCaptureKit capture tool (`clients/apple/scripts/capture-alan-window.sh`).

**Branch:** `paper-and-ink-design-system` (already created). Conventional commit style.

**Visual checkpoints:** Tasks 5 and 7 end with screenshots the maintainer must approve. Dark-mode values in this plan are starting proposals, expected to be tuned at the checkpoint.

---

### Task 1: Design language document

**Files:**
- Create: `docs/design/design-language.md`

- [ ] **Step 1: Create the document**

Write `docs/design/design-language.md` with exactly this content:

````markdown
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
````

- [ ] **Step 2: Commit**

```bash
git add docs/design/design-language.md
git commit -m "docs(design): add Paper & Ink design language"
```

---

### Task 2: Token test harness (failing first)

**Files:**
- Create: `clients/apple/scripts/test-shell-design-tokens.swift`
- Create: `clients/apple/scripts/test-shell-design-tokens.sh`

- [ ] **Step 1: Write the test runner script**

Create `clients/apple/scripts/test-shell-design-tokens.sh` (mode 755), same
pattern as `test-shell-sidebar-presentation.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
BUILD_DIR="${TMPDIR:-/tmp}/alan-shell-design-token-tests"
MODULE_CACHE_DIR="${BUILD_DIR}/clang-module-cache"
TEST_BINARY="${BUILD_DIR}/shell-design-token-tests"

mkdir -p "$MODULE_CACHE_DIR"

CLANG_MODULE_CACHE_PATH="$MODULE_CACHE_DIR" swiftc \
    "$REPO_ROOT/clients/apple/alan-macos/Support/ShellDesignTokens.swift" \
    "$REPO_ROOT/clients/apple/scripts/test-shell-design-tokens.swift" \
    -o "$TEST_BINARY"

"$TEST_BINARY"
```

- [ ] **Step 2: Write the failing assertions**

Create `clients/apple/scripts/test-shell-design-tokens.swift`:

```swift
import SwiftUI

var failures = 0

func expect(_ condition: Bool, _ label: String) {
    if condition {
        print("PASS \(label)")
    } else {
        failures += 1
        print("FAIL \(label)")
    }
}

// Type scale: integer roles, two tracks.
expect(ShellType.display == 17, "ShellType.display is 17")
expect(ShellType.heading == 13, "ShellType.heading is 13")
expect(ShellType.row == 12, "ShellType.row is 12")
expect(ShellType.caption == 11, "ShellType.caption is 11")
expect(ShellType.monoLabel == 11, "ShellType.monoLabel is 11")
expect(ShellType.monoCaption == 10, "ShellType.monoCaption is 10")

// Spacing scale: 4pt-derived semantic steps.
expect(ShellSpacing.hair == 2, "ShellSpacing.hair is 2")
expect(ShellSpacing.tight == 4, "ShellSpacing.tight is 4")
expect(ShellSpacing.control == 8, "ShellSpacing.control is 8")
expect(ShellSpacing.row == 12, "ShellSpacing.row is 12")
expect(ShellSpacing.section == 16, "ShellSpacing.section is 16")
expect(ShellSpacing.panel == 24, "ShellSpacing.panel is 24")

// Lamp hierarchy: in dark appearance, every paper surface sits below the
// ink surface in relative luminance.
let inkDark = ShellSurfaceValues.luminance(ShellSurfaceValues.inkSurfaceDark)
for (name, paper) in ShellSurfaceValues.darkPaperSurfaces {
    expect(
        ShellSurfaceValues.luminance(paper) < inkDark,
        "lamp: \(name) sits below ink surface in dark mode"
    )
}

// Daylight hierarchy: ink stays far darker than paper in light appearance.
let inkLight = ShellSurfaceValues.luminance(ShellSurfaceValues.inkSurfaceLight)
for (name, paper) in ShellSurfaceValues.lightPaperSurfaces {
    expect(
        ShellSurfaceValues.luminance(paper) > inkLight,
        "ink well: \(name) sits above ink surface in light mode"
    )
}

if failures > 0 {
    print("\(failures) shell design token check(s) failed")
    exit(1)
}
print("All shell design token checks passed")
```

- [ ] **Step 3: Run it to verify it fails**

```bash
chmod +x clients/apple/scripts/test-shell-design-tokens.sh
bash clients/apple/scripts/test-shell-design-tokens.sh
```

Expected: **compile FAILURE** with `cannot find 'ShellType' in scope` (and
the same for `ShellSpacing`, `ShellSurfaceValues`). That is the failing
state; do not commit yet.

---

### Task 3: `ShellType`, `ShellSpacing`, `ShellSurfaceValues`, and color domains

**Files:**
- Modify: `clients/apple/alan-macos/Support/ShellDesignTokens.swift`

This task is a **pure refactor**: `ShellSurfaceValues` carries the *current*
dark values, so no appearance changes yet. The dark rework happens in Task 4
as its own reviewable commit.

- [ ] **Step 1: Add the value layer and scales**

In `ShellDesignTokens.swift`, directly after the `private extension Color`
block (after line 30), insert:

```swift
/// Raw light/dark RGB values for the core paper and ink surfaces.
/// Kept as plain tuples so script tests can assert the Paper & Ink
/// luminance hierarchy (see docs/design/design-language.md).
enum ShellSurfaceValues {
    static let paperRootLight: (Double, Double, Double) = (1.0, 1.0, 1.0)
    static let paperRootDark: (Double, Double, Double) = (0.055, 0.061, 0.074)
    static let paperCanvasLight: (Double, Double, Double) = (0.94, 0.94, 0.965)
    static let paperCanvasDark: (Double, Double, Double) = (0.045, 0.050, 0.062)
    static let paperWindowLight: (Double, Double, Double) = (0.972, 0.973, 0.985)
    static let paperWindowDark: (Double, Double, Double) = (0.055, 0.061, 0.074)
    static let paperSidebarLight: (Double, Double, Double) = (0.922, 0.924, 0.953)
    static let paperSidebarDark: (Double, Double, Double) = (0.071, 0.079, 0.096)
    static let paperWorkspaceLight: (Double, Double, Double) = (0.979, 0.98, 0.989)
    static let paperWorkspaceDark: (Double, Double, Double) = (0.050, 0.056, 0.070)

    static let inkSurfaceLight: (Double, Double, Double) = (0.10, 0.12, 0.16)
    static let inkSurfaceDark: (Double, Double, Double) = (0.050, 0.061, 0.076)
    static let inkRaisedLight: (Double, Double, Double) = (0.16, 0.18, 0.24)
    static let inkRaisedDark: (Double, Double, Double) = (0.100, 0.116, 0.145)

    static var lightPaperSurfaces: [(String, (Double, Double, Double))] {
        [
            ("paperRoot", paperRootLight),
            ("paperCanvas", paperCanvasLight),
            ("paperWindow", paperWindowLight),
            ("paperSidebar", paperSidebarLight),
            ("paperWorkspace", paperWorkspaceLight),
        ]
    }

    static var darkPaperSurfaces: [(String, (Double, Double, Double))] {
        [
            ("paperRoot", paperRootDark),
            ("paperCanvas", paperCanvasDark),
            ("paperWindow", paperWindowDark),
            ("paperSidebar", paperSidebarDark),
            ("paperWorkspace", paperWorkspaceDark),
        ]
    }

    static func luminance(_ rgb: (Double, Double, Double)) -> Double {
        0.2126 * rgb.0 + 0.7152 * rgb.1 + 0.0722 * rgb.2
    }
}

/// Role-based type scale. Two tracks, integer sizes only; weights stay
/// per-context. See docs/design/design-language.md.
enum ShellType {
    static let display: CGFloat = 17
    static let heading: CGFloat = 13
    static let row: CGFloat = 12
    static let caption: CGFloat = 11
    static let monoLabel: CGFloat = 11
    static let monoCaption: CGFloat = 10

    static func pro(_ size: CGFloat, weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight)
    }

    static func mono(_ size: CGFloat, weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight, design: .monospaced)
    }
}

/// Semantic 4pt spacing scale. New layout code uses these names instead of
/// raw numeric paddings.
enum ShellSpacing {
    static let hair: CGFloat = 2
    static let tight: CGFloat = 4
    static let control: CGFloat = 8
    static let row: CGFloat = 12
    static let section: CGFloat = 16
    static let panel: CGFloat = 24
}

/// Paper domain: chrome surfaces. Calm, cool, low-saturation; always recedes
/// behind the ink surface.
enum ShellPaper {
    static let root = Color.shellAdaptive(
        light: ShellSurfaceValues.paperRootLight,
        dark: ShellSurfaceValues.paperRootDark
    )
    static let canvas = Color.shellAdaptive(
        light: ShellSurfaceValues.paperCanvasLight,
        dark: ShellSurfaceValues.paperCanvasDark
    )
    static let window = Color.shellAdaptive(
        light: ShellSurfaceValues.paperWindowLight,
        dark: ShellSurfaceValues.paperWindowDark
    )
    static let sidebar = Color.shellAdaptive(
        light: ShellSurfaceValues.paperSidebarLight,
        dark: ShellSurfaceValues.paperSidebarDark
    )
    static let workspace = Color.shellAdaptive(
        light: ShellSurfaceValues.paperWorkspaceLight,
        dark: ShellSurfaceValues.paperWorkspaceDark
    )
}

/// Ink domain: the terminal surface family and the well rim.
enum ShellInk {
    static let surface = Color.shellAdaptive(
        light: ShellSurfaceValues.inkSurfaceLight,
        dark: ShellSurfaceValues.inkSurfaceDark
    )
    static let raised = Color.shellAdaptive(
        light: ShellSurfaceValues.inkRaisedLight,
        dark: ShellSurfaceValues.inkRaisedDark
    )
    /// Top inner edge of the terminal surround ("the well rim").
    static let rimHighlight = Color.shellAdaptive(
        light: (1.0, 1.0, 1.0),
        lightAlpha: 0.18,
        dark: (1.0, 1.0, 1.0),
        darkAlpha: 0.07
    )
    /// Bottom outer contact line of the terminal surround.
    static let rimShadowLine = Color.shellAdaptive(
        light: (0.0, 0.0, 0.0),
        lightAlpha: 0.14,
        dark: (0.0, 0.0, 0.0),
        darkAlpha: 0.32
    )
}

/// Signal domain: meaning-bearing color. Governed by the signal semantics
/// table in docs/design/design-language.md — anything not listed there is
/// silent.
enum ShellSignal {
    /// The only orange: the user must act (input, approval, intervention).
    static let action = Color.shellAdaptive(
        light: (0.82, 0.55, 0.24),
        dark: (0.94, 0.68, 0.34)
    )
    /// Keyboard focus and scrub preview only; never a status color.
    static let focus = Color.shellAdaptive(
        light: (0.31, 0.39, 0.71),
        dark: (0.50, 0.60, 0.94)
    )
    static let focusSoft = Color.shellAdaptive(
        light: (0.90, 0.92, 0.98),
        dark: (0.18, 0.22, 0.34)
    )
    /// Reserved phase-2 agent-activity interface: maximum luminance delta a
    /// breathing surface may add over its resting value.
    static let breathLuminanceDelta: Double = 0.06
}
```

Note: `Color.shellAdaptive` is `private` to this file, which is why the
domains must live in `ShellDesignTokens.swift`.

- [ ] **Step 2: Re-point the old `ShellPalette` names at the domains**

In `enum ShellPalette`, replace these eight member definitions (keep every
other member untouched). They become aliases so the 4 view files that use
`ShellPalette` keep compiling; new code uses the domain names directly:

```swift
    // Deprecated aliases — use ShellPaper / ShellInk / ShellSignal in new
    // code. Views migrate per-file in follow-up changes.
    static let rootBacking = ShellPaper.root
    static let canvas = ShellPaper.canvas
    static let window = ShellPaper.window
    static let sidebar = ShellPaper.sidebar
    static let workspace = ShellPaper.workspace
    static let terminal = ShellInk.surface
    static let terminalSoft = ShellInk.raised
    static let attention = ShellSignal.action
    static let accent = ShellSignal.focus
    static let accentSoft = ShellSignal.focusSoft
```

(That is ten names; delete their old `Color.shellAdaptive(...)` bodies.)

- [ ] **Step 3: Run the token test — expect a partial pass**

```bash
bash clients/apple/scripts/test-shell-design-tokens.sh
```

Expected: the type-scale, spacing, and `ink well:` (light mode) assertions
all PASS; the five `lamp:` (dark mode) assertions **FAIL** and the binary
exits 1. This is intentional: `ShellSurfaceValues` still carries the legacy
dark values, where chrome (sidebar 0.071) is *brighter* than ink (0.050) —
exactly the inversion Task 4 fixes. Do not "fix" the test; Task 4 makes it
green.

- [ ] **Step 4: Verify existing focused shell tests still pass**

```bash
just apple-shell-focused-tests
```

Expected: all scripts pass (the alias refactor must not change any resolved
color in light mode; dark values are still byte-identical to legacy).

- [ ] **Step 5: Commit the refactor**

```bash
git add clients/apple/alan-macos/Support/ShellDesignTokens.swift \
        clients/apple/scripts/test-shell-design-tokens.sh \
        clients/apple/scripts/test-shell-design-tokens.swift
git commit -m "feat(macos-shell): add ShellType/ShellSpacing scales and paper-ink-signal color domains"
```

---

### Task 4: Dark-mode lamp rework

**Files:**
- Modify: `clients/apple/alan-macos/Support/ShellDesignTokens.swift` (the `ShellSurfaceValues` dark tuples only)

- [ ] **Step 1: Replace the dark values**

In `ShellSurfaceValues`, change exactly these seven tuples (starting
proposals — final values are tuned at the Task 5 checkpoint):

```swift
    static let paperRootDark: (Double, Double, Double) = (0.036, 0.040, 0.050)
    static let paperCanvasDark: (Double, Double, Double) = (0.032, 0.036, 0.046)
    static let paperWindowDark: (Double, Double, Double) = (0.036, 0.040, 0.050)
    static let paperSidebarDark: (Double, Double, Double) = (0.042, 0.047, 0.060)
    static let paperWorkspaceDark: (Double, Double, Double) = (0.038, 0.042, 0.053)

    static let inkSurfaceDark: (Double, Double, Double) = (0.105, 0.118, 0.142)
    static let inkRaisedDark: (Double, Double, Double) = (0.150, 0.168, 0.200)
```

(Seven tuples total: five paper, two ink.)

- [ ] **Step 2: Run the token test to verify the lamp assertions now pass**

```bash
bash clients/apple/scripts/test-shell-design-tokens.sh
```

Expected: all assertions PASS including the five `lamp:` lines, exit 0.

- [ ] **Step 3: Commit**

```bash
git add clients/apple/alan-macos/Support/ShellDesignTokens.swift
git commit -m "feat(macos-shell): rework dark palette to the lamp hierarchy"
```

---

### Task 4b: Restore the pinned-sidebar paper backing

**Files:**
- Modify: `clients/apple/alan-macos/MacShellRootView.swift:336-352` (`pinnedSidebarSurface()`)
- Modify: `clients/apple/alan-macos/Support/ShellDesignTokens.swift`

**Root cause** (confirmed via `git show f7adad3`): the workspace-colors
change replaced the root `ShellMaterialBackgroundView(.windowBackdrop)` with
the opaque pure-white `ShellPalette.rootBacking` and deferred material
treatment to "a later dedicated pass" — this is that pass. The pinned sidebar
paints no background of its own (only the floating overlay carries
`.sidebarGlass`), so the default state renders sidebar rows directly on pure
white, and the white selected-row card has nothing to lift off.

This is the batch's one deliberate view-file change. The root stays opaque
(per the merged workspace-colors decision); the sidebar column regains its
own material, satisfying the sidebar-material requirement in
`macos-shell-ui-ux-conformance` ("unified tinted macOS material stack").

- [ ] **Step 1: Give the pinned sidebar its material back**

In `MacShellRootView.swift`, `pinnedSidebarSurface()`, add a `.background`
between `.clipped()` and `.ignoresSafeArea(edges: .top)`:

```swift
        if sidebarPresentation.showsPinnedSurfaceContent {
            sidebarContent(isSwipeEnabled: true)
                .frame(width: sidebarWidth)
                .offset(x: sidebarPinnedContentOffsetX)
                .opacity(sidebarPinnedContentOpacity)
                .allowsHitTesting(!isSidebarCollapsed)
                .frame(width: sidebarPinnedVisibleWidth, alignment: .leading)
                .clipped()
                .background {
                    ShellMaterialBackgroundView(.sidebarGlass)
                        .ignoresSafeArea(edges: .top)
                }
                .ignoresSafeArea(edges: .top)
        } else {
```

The background sits outside `.opacity` and inside the visible-width frame, so
collapse animation narrows the material with the surface instead of fading
it.

- [ ] **Step 2: Deepen the paper values so the material reads as paper**

In `ShellSurfaceValues`, change the light sidebar tuple:

```swift
    static let paperSidebarLight: (Double, Double, Double) = (0.902, 0.906, 0.940)
```

In `enum ShellPalette`, change two members in place (`materialScrim` is the
fill of `.sidebarGlass`, so after Step 1 it governs the default pinned
state):

```swift
    static let sidebarRowSelected = Color.shellAdaptive(
        light: (1.0, 1.0, 1.0),
        lightAlpha: 0.88,
        dark: (0.215, 0.235, 0.282),
        darkAlpha: 0.78
    )
```

```swift
    static let materialScrim = Color.shellAdaptive(
        light: (0.745, 0.755, 0.845),
        lightAlpha: 0.50,
        dark: (0.030, 0.037, 0.050),
        darkAlpha: 0.78
    )
```

- [ ] **Step 3: Run the token test and focused shell tests**

```bash
bash clients/apple/scripts/test-shell-design-tokens.sh
just apple-shell-focused-tests
```

Expected: all PASS (the light `ink well:` assertions tolerate the slightly
darker paper — sidebar luminance 0.91 is still far above ink 0.12).

- [ ] **Step 4: Commit**

```bash
git add clients/apple/alan-macos/MacShellRootView.swift \
        clients/apple/alan-macos/Support/ShellDesignTokens.swift
git commit -m "fix(macos-shell): restore pinned-sidebar material backing lost in workspace-colors pass"
```

**Tuning iteration (gate feedback 2026-06-12):** the sidebar-only backing
split the chrome into gray-left/white-right. Superseded by moving
`ShellMaterialBackgroundView(.sidebarGlass)` to the window root in
`MacShellRootView` body and removing the pinned-sidebar background, plus the
spec delta in `specs/macos-shell-ui-ux-conformance/spec.md`.

---

### Task 5: Visual checkpoint — dark-mode lamp

**Files:** none (verification only)

- [ ] **Step 1: Build and install the dev-channel app**

```bash
just install-dev
```

Expected: `Alan Dev.app` builds and installs. Never touch the stable
`Alan.app` channel.

- [ ] **Step 2: Capture before/after-style evidence**

Launch Alan Dev, switch the in-app appearance toggle to Dark, arrange one
terminal tab, then:

```bash
mkdir -p debug/screenshots/paper-ink-lamp
./clients/apple/scripts/capture-alan-window.sh --channel dev \
  --output debug/screenshots/paper-ink-lamp/dark-single-tab.png
```

Also capture Light appearance with one selected and one unselected tab
visible in the sidebar:

```bash
./clients/apple/scripts/capture-alan-window.sh --channel dev \
  --output debug/screenshots/paper-ink-lamp/light-single-tab.png
```

- [ ] **Step 3: Maintainer approval gate**

Show both screenshots to the maintainer. Approval criteria:

- Dark: terminal surface clearly reads as the brightest large surface; chrome
  reads as the room around it; no muddy mid-gray sandwich.
- Light: sidebar paper reads as a perceptible cool gray rather than white;
  the selected tab row and selected Space pill read as distinct raised white
  cards against that paper; everything else is unchanged.

If values need tuning, adjust the `ShellSurfaceValues` tuples (Task 4) or
the Task 4b values (`paperSidebarLight`, `sidebarRowSelected`,
`materialScrim`), re-run the token test, rebuild, recapture, and amend the
corresponding commit (`git commit --amend --no-edit`) before proceeding.

---

### Task 6: Design-token lint guard with baseline

**Files:**
- Create: `scripts/check-shell-design-tokens.sh`
- Create: `scripts/shell-design-token-baseline.txt` (generated)
- Modify: `justfile`

- [ ] **Step 1: Write the guard script**

Create `scripts/check-shell-design-tokens.sh` (mode 755), following the
violations-array style of `scripts/check-agent-root-layout-strings.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
shell_dir="$repo_root/clients/apple/alan-macos"
baseline_file="$repo_root/scripts/shell-design-token-baseline.txt"

# Raw design literals new shell UI code must not add:
#   - hard-coded font sizes:        system(size:
#   - hard-coded RGB colors:        Color(red: / NSColor(red:
#   - hard-coded numeric paddings:  .padding(... <digit> ...)
pattern='system\(size:|Color\(red:|NSColor\(red:|\.padding\([^)]*[0-9]'

is_allowed_file() {
  case "$1" in
    */Support/ShellDesignTokens.swift) return 0 ;;
    */Support/ConsoleAdaptiveColor.swift) return 0 ;;
    *) return 1 ;;
  esac
}

mode="check"
if [[ "${1:-}" == "--update-baseline" ]]; then
  mode="update"
fi

current="$(mktemp)"
trap 'rm -f "$current"' EXIT

while IFS= read -r file; do
  if is_allowed_file "$file"; then
    continue
  fi
  count="$(grep -cE "$pattern" "$file" || true)"
  if [[ "$count" -gt 0 ]]; then
    rel="${file#"$repo_root"/}"
    printf '%s:%s\n' "$rel" "$count" >>"$current"
  fi
done < <(find "$shell_dir" -name '*.swift' | sort)

if [[ "$mode" == "update" ]]; then
  cp "$current" "$baseline_file"
  echo "Baseline updated: $baseline_file"
  exit 0
fi

violations=()
while IFS=: read -r rel count; do
  allowed=0
  if [[ -f "$baseline_file" ]]; then
    baseline_entry="$(grep -F "$rel:" "$baseline_file" | tail -1 || true)"
    if [[ -n "$baseline_entry" ]]; then
      allowed="${baseline_entry##*:}"
    fi
  fi
  if (( count > allowed )); then
    violations+=("$rel: $count raw design literals (baseline $allowed)")
  fi
done <"$current"

if ((${#violations[@]})); then
  printf 'Raw design literals exceed the recorded baseline:\n' >&2
  printf '%s\n' "${violations[@]}" >&2
  printf '\nUse ShellType / ShellSpacing / ShellPaper / ShellInk / ShellSignal tokens\n' >&2
  printf '(clients/apple/alan-macos/Support/ShellDesignTokens.swift), or run\n' >&2
  printf 'scripts/check-shell-design-tokens.sh --update-baseline after a reviewed migration.\n' >&2
  exit 1
fi

echo "Shell design token guard passed"
```

- [ ] **Step 2: Generate the baseline and run the guard**

```bash
chmod +x scripts/check-shell-design-tokens.sh
./scripts/check-shell-design-tokens.sh --update-baseline
./scripts/check-shell-design-tokens.sh
```

Expected: baseline file lists roughly five files (TerminalPaneView.swift and
Views/Console/ContentView.swift carry the bulk, ~45 each); second command
prints `Shell design token guard passed`, exit 0.

- [ ] **Step 3: Verify the guard fails on a seeded violation**

```bash
printf 'import SwiftUI\nlet seeded = Font.system(size: 13.7)\n' \
  > clients/apple/alan-macos/Views/Shell/SeededViolation.swift
./scripts/check-shell-design-tokens.sh; echo "exit=$?"
rm clients/apple/alan-macos/Views/Shell/SeededViolation.swift
```

Expected: guard prints the violation for `SeededViolation.swift: 1 raw design
literals (baseline 0)` and `exit=1`. After `rm`, re-run the guard once more
and confirm exit 0.

- [ ] **Step 4: Add the justfile recipe**

In `justfile`, after the `guard-macos-auto-update` recipe, add:

```make
# Check macOS shell design-token literals against the recorded baseline
guard-shell-design-tokens:
    ./scripts/check-shell-design-tokens.sh
```

Run `just guard-shell-design-tokens`; expected: `Shell design token guard passed`.

- [ ] **Step 5: Commit**

```bash
git add scripts/check-shell-design-tokens.sh \
        scripts/shell-design-token-baseline.txt justfile
git commit -m "feat(macos-shell): add design-token lint guard with baseline"
```

---

### Task 7: Screenshot state matrix

**Files:**
- Create: `clients/apple/scripts/capture-shell-state-matrix.sh`
- Modify: `justfile`

- [ ] **Step 1: Write the matrix script**

Create `clients/apple/scripts/capture-shell-state-matrix.sh` (mode 755). It
is deliberately semi-manual: the operator arranges each state, the script
captures consistently named files:

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
out_dir="${1:-$REPO_ROOT/debug/screenshots/state-matrix-$(date +%Y%m%d-%H%M%S)}"
mkdir -p "$out_dir"

states=(
  empty-space
  single-tab
  split-panes
  multi-space
  dark-mode
  reduced-transparency
)

echo "Capturing the Alan Dev shell state matrix into: $out_dir"
echo "Target app: Alan Dev.app (dev channel). Do not use the stable app."

for state in "${states[@]}"; do
  echo ""
  echo "==> Arrange the Alan Dev window for state: $state"
  read -r -p "    Press Enter to capture..."
  "$SCRIPT_DIR/capture-alan-window.sh" --channel dev \
    --output "$out_dir/$state.png"
  echo "    Captured $out_dir/$state.png"
done

echo ""
echo "State matrix complete: $out_dir"
```

- [ ] **Step 2: Add the justfile recipe**

In `justfile`, after the `apple-shell-ui-smoke` recipe, add:

```make
# Capture the macOS shell screenshot state matrix (semi-manual, dev channel)
apple-shell-screenshot-matrix out_dir="":
    bash clients/apple/scripts/capture-shell-state-matrix.sh {{out_dir}}
```

- [ ] **Step 3: Run the matrix once and review**

```bash
chmod +x clients/apple/scripts/capture-shell-state-matrix.sh
just apple-shell-screenshot-matrix
```

Walk through all six states against the running Alan Dev app. Expected: six
PNGs in the output directory. Review them against the design language doc
(lamp hierarchy in `dark-mode.png`; unchanged light appearance elsewhere).
This is the second maintainer visual gate.

- [ ] **Step 4: Commit**

```bash
git add clients/apple/scripts/capture-shell-state-matrix.sh justfile
git commit -m "feat(macos-shell): add screenshot state-matrix capture tool"
```

---

### Task 8: Final verification and change bookkeeping

**Files:**
- Modify: `openspec/changes/add-macos-paper-ink-design-system/tasks.md`

- [ ] **Step 1: Full verification pass**

```bash
just verify
just apple-shell-focused-tests
bash clients/apple/scripts/test-shell-design-tokens.sh
just guard-shell-design-tokens
```

Expected: all green. (`just verify` covers the Rust workspace, which this
change does not touch, but it is the repo's required post-change loop.)

- [ ] **Step 2: Check off completed items in tasks.md**

Mark the implementation and verification checkboxes in
`openspec/changes/add-macos-paper-ink-design-system/tasks.md` that this plan
completed. Leave PR review and archive items unchecked.

- [ ] **Step 3: Commit**

```bash
git add openspec/changes/add-macos-paper-ink-design-system/tasks.md
git commit -m "chore(openspec): check off paper-ink design system implementation tasks"
```

---

## Self-Review Notes

- Spec coverage: design doc (Task 1), ShellType/ShellSpacing (Tasks 2–3),
  palette domains + aliases + wellRim/signalBreath (Task 3), dark lamp rework
  (Task 4) with visual checkpoint (Task 5), lint guard with baseline
  (Task 6), screenshot matrix (Task 7). All design.md deliverables have
  owning tasks.
- The Task 3 commit intentionally lands with the five `lamp:` assertions
  failing if run standalone; Tasks 3 and 4 are separate commits for
  reviewability but the test suite is only required green from Task 4 Step 2
  onward. If a strictly green-per-commit history is preferred, squash Tasks 3
  and 4.
- View files are untouched. Two intended visual changes, both token-level:
  dark-mode lamp hierarchy (Task 4) and light-mode paper separation
  (Task 4b). Everything else is byte-identical via the alias strategy.
