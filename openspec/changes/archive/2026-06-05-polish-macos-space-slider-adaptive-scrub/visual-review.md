## Visual Review

Run: `space-slider-adaptive-open-fresh`

Fresh launch evidence:

- Built app: `debug/DerivedData/apple-shell-ui-smoke/Build/Products/Debug/Alan.app`
- Executable confirmed by `lsof -p 50789`:
  `/private/tmp/alan-space-slider-adaptive-scrub/debug/DerivedData/apple-shell-ui-smoke/Build/Products/Debug/Alan.app/Contents/MacOS/Alan`
- Bundle id: `app.alanworks.macos.space-slider-adaptive`
- Smoke manifest: `debug/artifacts/space-slider-adaptive-open-fresh/manifest.txt`
- Capturable window: `capture-alan-window.sh --pid 50789 --list` returned window `23016`.

Light-mode density screenshots:

- `debug/artifacts/space-slider-adaptive-open-fresh/01-space-create.png`
  shows the 3-Space low-density state with all Space targets rendered as named,
  single-line controls.
- `debug/artifacts/space-slider-adaptive-open-fresh/04-six-spaces.png`
  shows the 6-Space medium-density state with a selected title control and
  compact inactive short-title controls.
- `debug/artifacts/space-slider-adaptive-open-fresh/05-nine-spaces.png`
  shows the 9-Space high-density state with compact inactive indicators and a
  selected title control. The New Space affordance is visually disabled at cap.

Interaction evidence:

- `debug/artifacts/space-slider-adaptive-open-fresh/06-hover-indicator.png`
  records a cursor-warp hover attempt over the high-density slider. SwiftUI hover
  expansion did not visibly trigger from this non-Accessibility path, so this is
  not treated as proof of hover expansion.
- Local Accessibility scripting was unavailable:
  `osascript -e 'tell application "System Events" to count of processes'`
  returned error `-10827`, and the smoke manifest recorded
  `skipped_ui_scripting_steps=Accessibility permission was unavailable or disabled`.
- Hover expansion, scrub preview focus, drag/wheel commit, cancel behavior,
  reduced motion, and vertical-scroll protection are covered by
  `clients/apple/scripts/test-shell-sidebar-space-slider-layout.sh`, including:
  `verifiesHighDensityHoverExpandsTheHoveredIndicator`,
  `verifiesScrubFocusIsDistinctFromSelectedSpace`,
  `verifiesDragScrubPreviewAndCommitTarget`,
  `verifiesWheelScrubPreviewAndCommitTarget`,
  `verifiesScrubCancelRestoresTheSelectedSource`, and
  `verifiesWheelIntentRoutingProtectsVerticalScroll`.

Post-commit selected state:

- The 3-, 6-, and 9-Space screenshots were captured after successful
  `space.create` control-plane commands. Each command returned `applied=true`,
  and the captured slider selected the newly created Space after the state
  mutation committed.

Scope check:

- The implementation diff is limited to the macOS sidebar Space slider polish,
  the Space creation cap guard, focused shell contract checks/tests, and this
  OpenSpec change.
