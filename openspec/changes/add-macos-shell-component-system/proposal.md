## Why

The macOS SwiftUI presentation layer has design tokens (`ShellDesignTokens`) and a
nascent control library (`ShellFormControls`), but the library is adopted by exactly
one surface (the Space creation form). The project already has a design-token guard
(`scripts/check-shell-design-tokens.sh`) that ratchets per-file raw-literal counts
(`system(size:`, `Color(red:`, numeric `.padding(`); its shell-surface baseline is
`TerminalPaneView.swift` 63, `ShellSidebarView.swift` 16, `MacShellRootView.swift` 1.
But two gaps remain. First, that guard is only a local `just` recipe — it is **not run
in CI**, so the ratchet is a manual promise. Second, even where styling does use tokens
correctly, the same presentational concepts are duplicated across giant view files —
five ad-hoc "row" structs (`ShellSettingsRow`, `ShellSettingsAgentSummaryRow`,
`TerminalInfoRow`, `ShellTabSidebarRow`, `ShellSidebarTabControlRow`) plus separate
card/chip implementations (`TerminalInfoCard`, `TerminalPaneChip`) buried as
`private struct`s inside 2,500–4,100 line files. There is no contract that defines a
reusable primitive catalog, requires feature views to compose it, or wires the
existing raw-literal guard into CI — so duplication grows unchecked.

## What Changes

- Introduce a **presentational component layer** for the macOS SwiftUI client: a
  named catalog of reusable primitives (surfaces, controls, rows, indicators,
  labels) plus the shared `ButtonStyle`/`ViewModifier` styles they are built from,
  housed in a dedicated design-system home (`Views/Shell/Components/`, absorbing the
  existing `Controls/`).
- Establish a **layering contract**: semantic tokens (`ShellPaper`/`ShellInk`/
  `ShellSignal`/`ShellPalette`/`ShellType`/`ShellSpacing`) or primitives are the
  styling source; **referencing a token namespace, including `ShellPalette.*`, is
  compliant — the debt is raw literals** (`Color(red:`/`Color.red`/
  `.font(.system(size:`/numeric `.padding(`). The raw-literal floor is enforced by the
  **existing** `check-shell-design-tokens.sh` guard rather than a new parallel count;
  this change **wires that guard into CI** (it is only a local recipe today). New and
  migrated feature views compose primitives and MUST NOT add new raw literals or new
  primitive-role duplicates.
- Separate **style from structure** (SwiftUI-idiomatic): button/field press and
  hover behavior lives in `ButtonStyle`/`ViewModifier`/`*Style` types in the
  design-system layer, not scattered through feature files.
- Require a **`#Preview` gallery** for every primitive covering all states
  (default/hover/selected/disabled/dark), and bake accessibility (Dynamic Type,
  VoiceOver labels, reduce-motion) into the primitives.
- Migrate existing surfaces via a **strangler-fig sequence** — one surface per change
  (five at landing: terminal-pane SwiftUI chrome and settings surface in
  `TerminalPaneView.swift`; sidebar and space slider in `ShellSidebarView.swift`; root
  chrome in `MacShellRootView.swift`), with completeness verified against the
  design-token guard's per-file baseline (63 / 16 / 1) so no surface is left unowned —
  replacing raw literals with tokens/primitives and verifying screenshot parity, rather
  than a single big-bang rewrite.
- Consolidate the duplicated shell implementations (five row structs → `ShellRow`;
  `TerminalInfoCard`/`TerminalPaneChip` and the `ShellWorkspacePanelFrame` modifier →
  canonical surface/indicator primitives), and route shell controls through the
  existing canonical `ShellButtonPressStyle` / `ShellTextField` rather than ad-hoc
  styles.

Scope is the primary macOS **shell** SwiftUI presentation layer only. Ghostty/AppKit
terminal-host internals (`TerminalHostView`, terminal surface input/attachment) and
the legacy/mobile remote-control console (`Views/Console/`) are explicitly out of
scope; both remain governed by `macos-app-architecture-maintainability` (the console
surfaces must stay isolated from the primary shell), so neither is pulled through the
shell design-system home.

## Capabilities

### New Capabilities
- `macos-shell-component-system`: the presentational component layer contract for the
  macOS SwiftUI client — token single-source rule, the reusable primitive catalog,
  style/structure separation, the per-primitive preview gallery and accessibility
  baseline, and the strangler-fig adoption + measurable-drift-reduction requirements.

### Modified Capabilities
<!-- None. This change adds a new, narrowly-scoped contract. It complements
     macos-app-architecture-maintainability (file/ownership boundaries) and
     macos-shell-ui-ux-conformance (visual/interaction rules) without changing
     their requirements; the design doc records the demarcation. -->

## Impact

- **Code**: New design-system home under `clients/apple/alan-macos/Views/Shell/Components/`
  (absorbs `Views/Shell/Controls/ShellFormControls.swift`). Incremental edits to
  feature surfaces: `ShellSidebarView.swift`, `MacShellRootView.swift`,
  `ShellWorkspaceView.swift`, the Space slider, and the SwiftUI settings/info chrome
  currently inside `TerminalPaneView.swift`. `Support/ShellDesignTokens.swift` gains
  semantic-layer clarity but no token-value changes.
- **CI/tooling**: wire the existing `scripts/check-shell-design-tokens.sh` guard into
  CI (`.github/workflows/ci.yml`) so the raw-literal ratchet is blocking, not just a
  local `just guard-shell-design-tokens` recipe. Optionally extend the guard later to
  cover `RoundedRectangle`-with-literal-radius and raw named hues.
- **Tests**: `just apple-shell-focused-tests`, `just guard-shell-design-tokens`, and
  `apple-shell-ui-smoke` gate each migration phase; visual changes reviewed against
  screenshots.
- **Docs**: Apple client README directory section updated to document the
  `Components/` design-system home.
- **No behavior change for users** is intended; each phase must hold screenshot
  parity. Out of scope: Rust crates, terminal-host AppKit bridges, non-macOS clients,
  and the legacy/mobile remote-control console (`Views/Console/`), which
  `macos-app-architecture-maintainability` requires to stay isolated from the primary
  macOS shell — it is not pulled through the shell design-system home.
