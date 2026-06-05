## Automated Verification

- `bash clients/apple/scripts/test-shell-settings-surface.sh`
- `bash clients/apple/scripts/test-shell-runtime-metadata.sh`
- `bash clients/apple/scripts/check-shell-contracts.sh`
- `openspec validate polish-macos-settings-native-surface --strict`
- `openspec validate add-macos-settings-navigation --strict`
- `git diff --check`
- `xcodebuild -project clients/apple/alan-macos.xcodeproj -scheme alan-macos
  -configuration Debug -destination generic/platform=macOS -derivedDataPath
  /tmp/alan-xcode-derived-settings-polish build`
- `xcodebuild -project clients/apple/alan-macos.xcodeproj -scheme alan-macos
  -configuration Debug -destination generic/platform=macOS -derivedDataPath
  /tmp/alan-xcode-derived-settings-polish ARCHS=arm64
  PRODUCT_BUNDLE_IDENTIFIER=app.alanworks.macos.dev PRODUCT_NAME="Alan Dev"
  INFOPLIST_KEY_CFBundleDisplayName="Alan Dev" CODE_SIGNING_ALLOWED=NO build`

## Result

The implementation passes the updated control-panel checklist at the model,
contract, OpenSpec, and build levels:

- Settings title, source list, and detail area share one shallow native pane plane.
- The selected source-list row uses compact macOS capsule chrome with primary
  selected text and no blue accent bar.
- The detail area renders as direct sections with quiet dividers, not as a white
  page sheet or card stack.
- System metadata rows use the shared title/detail template so values sit under
  labels instead of forming a database-like table.
- Rows with controls use a bounded trailing accessory column so toggles,
  segmented controls, Copy, Show..., and Export buttons do not hug the far right
  edge.
- Daemon endpoint exposes a native Copy button; Shell state, Alan home, and
  Skill packages use Show... buttons without web-style external link arrows.
- Agent is grouped into Agent, Runtime, Skills, and Entry Points, with Skill
  packages inside Skills and no separate Sources section.
- Terminal rows use compact native copy such as Default profile, New profile,
  Create..., and Preview..., and the Login shell row no longer repeats its title
  as secondary copy.
- Diagnostics remains a real toggle plus export action, preserving Settings as
  a control panel rather than an About page.
- The surface avoids dashboard cards, decorative gradients, web-page hero
  spacing, and Apple System Settings-style card groups.

## Visual Verification Status

Fresh Alan Dev visual verification now passes.

- Built `/tmp/alan-xcode-derived-settings-polish/Build/Products/Debug/Alan Dev.app`
  with `PRODUCT_BUNDLE_IDENTIFIER=app.alanworks.macos.dev`.
- Launched that fresh Alan Dev bundle without quitting the stable Alan app.
- Opened Settings through `Cmd+,` in light mode.
- Captured and reviewed the General, Agent, and System Settings states with
  `screencapture -l` after the repository capture helper stalled in
  ScreenCapture handoff.

Observed results:

- General opens by default and shows compact capsule source-list navigation,
  direct section dividers, unified title/detail/control rows, aligned toggles,
  and no dashboard cards.
- Agent switching works and shows the long Agent, Runtime, Skills, and Entry
  Points content with compact row rhythm, bounded trailing controls, and
  subordinate values.
- System switching works and shows Bundle ID, Channel, Updates, daemon endpoint
  Copy, Shell state Show..., Alan home Show..., and diagnostics toggle rows with
  native action copy and controlled accent use.
- The Settings surface remains inside the shell content area and avoids
  web-style cards, marketing panels, and oversized page spacing.
