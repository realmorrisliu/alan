## Summary

- Removed the macOS Ask alan floating command input, Command-P command-input
  ownership, and `ShellCommandTabView`.
- Removed first-party New alan Tab creation from menus, sidebar/context
  actions, action registry, App Intents, automation helpers, shell control
  commands, and terminal launch resolution.
- Kept CLI and terminal-launched agent behavior intact: users can still run
  `alan`, `alan chat`, or `alan ask` from ordinary terminal tabs, and terminal
  activity metadata can still recognize user-launched agent processes.

## Verification

- `bash clients/apple/scripts/test-shell-action-registry.sh`
- `bash clients/apple/scripts/test-terminal-runtime-service.sh`
- `bash clients/apple/scripts/test-shell-automation-command-seams.sh`
- `bash clients/apple/scripts/test-shell-runtime-metadata.sh`
- `bash clients/apple/scripts/check-shell-action-registry-integration.sh`
- `bash clients/apple/scripts/check-shell-contracts.sh`
- `xcodebuild -project clients/apple/alan-macos.xcodeproj -scheme alan-macos -configuration Debug -derivedDataPath debug/xcode-derived/alan-macos-build build`
- `bash clients/apple/scripts/check-shell-app-intents-metadata.sh`
- `openspec validate remove-macos-ask-alan-and-alan-tabs --strict`
- `openspec validate --all --strict`

## Evidence

- Active macOS app source and the Xcode project no longer contain
  `ShellCommandTabView`, `Ask alan...`, `newAlanTab`, `createAlanTab`,
  `AlanCreateAlanTabIntent`, or first-party `.alan` launch target paths.
- The generated App Intents metadata no longer contains Create Alan Tab.
- Normal terminal tab creation remains available through the shared shell action
  registry and native Shell menu.
