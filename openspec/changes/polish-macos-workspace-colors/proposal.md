## Why

The macOS shell currently paints the whole window through a translucent,
material-tinted backdrop and renders an empty Space inside the terminal surface
frame. This makes light-mode Alan look less like native macOS apps and makes an
empty generic workspace read as a dark terminal, even though the right side is a
general content container for terminal, settings, markdown, and future content.

## What Changes

- Make the primary shell window backing color an opaque native-style base color,
  using `rgb(1,1,1)` in light appearance and a system-appropriate solid dark
  base in dark appearance.
- Remove default root-window material, transparency, and gradient wash from the
  main backing surface for this change; material treatment remains deferred to a
  later dedicated pass.
- Treat an empty selected Space as a workspace-level placeholder, not an empty
  terminal surface.
- Keep the empty Space action terminal-first: the primary action remains
  `New Tab` and creates a normal terminal tab in the current Space.
- Keep terminal-specific dark canvas, rim, and shadow treatment scoped to
  terminal content leaves.
- Keep existing settings and markdown content rendering as bounded content
  surfaces rather than terminal runtime surfaces.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `macos-shell-ui-ux-conformance`: Refine the default shell visual contract so
  the root backing surface is opaque/native, empty Spaces are generic workspace
  placeholders, and terminal styling belongs only to terminal content.

## Impact

- Affected code:
  - `clients/apple/alan-macos/MacShellRootView.swift`
  - `clients/apple/alan-macos/TerminalPaneView.swift`
  - `clients/apple/alan-macos/Support/ShellDesignTokens.swift`
  - Relevant shell UI contract tests under `clients/apple/scripts/`
- No daemon API, runtime protocol, provider, persistence, or dependency changes.
- No breaking changes.
