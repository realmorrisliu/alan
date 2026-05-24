## Why

alan's macOS shell has an agent-readable control plane, but it lacks the
automation and test coverage expected from a native terminal-grade app. To make
future terminal work safe, alan needs App Intents/automation surfaces and real
Apple-client tests around shell state, runtime delivery, control-plane behavior,
and UI smoke flows.

## What Changes

- Add App Intents for core shell actions: create terminal tab, create alan tab,
  split pane, focus pane, close pane/tab, send text, read pane summary, and open
  attention items.
- Keep alan's file/socket control plane as the agent-facing automation contract,
  while using App Intents for user/system automation via Shortcuts, Spotlight,
  and future macOS surfaces.
- Add Apple-client unit tests for shell model mutations, runtime registry/service
  behavior, boot profile resolution, and local control-plane command execution.
- Add UI smoke tests or scripted screenshot checks for launch, space/tab
  switching, split creation, command UI, pane-scoped Find behavior, and basic
  terminal input.
- Extend build/test documentation and `just`/script entry points so the focused
  Apple checks are discoverable and repeatable.
- Keep tests layered so most run without real Ghostty artifacts, while a smaller
  integration lane verifies the linked Ghostty path when artifacts are prepared.

## Capabilities

### New Capabilities
- `macos-shell-automation-surfaces`: Defines App Intents and user/system
  automation behavior for alan's macOS shell.

### Modified Capabilities
- `macos-shell-build-test-contract`: Apple client tests become a required part
  of the macOS shell quality gate, including unit, control-plane, and UI smoke
  coverage.
- `macos-shell-control-plane-reliability`: Agent-facing control-plane behavior
  must remain aligned with App Intent outcomes and test fixtures.
- `macos-shell-ui-ux-conformance`: UI conformance gains repeatable screenshot or
  smoke-test evidence for key user flows.

## Impact

- Apple client project: new XCTest/UI test targets if absent, App Intent files,
  test fixtures/mocks, and project build settings.
- Scripts/docs: `clients/apple/README.md`, `justfile`, and Apple test helper
  scripts gain focused build/test commands.
- Runtime boundaries: test seams around terminal runtime service, control-plane
  command execution, and boot profile resolution must be stable enough for mocks.
- User/system automation: Shortcuts and Spotlight can drive alan shell actions
  without depending on private socket details.
