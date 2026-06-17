# Shell Core Parity Fixtures

This directory stores Swift-exported parity fixtures for `alan-shell-core`.
Each fixture is JSON and its `id` must match its path without the `.json`
extension, using `/` separators.

Initial domains:

- `split-tree/`
- `reducer/`
- `manifest/`
- `action-registry/`
- `control-command/`
- `terminal-profile/`
- `settings-summary/`

Fixtures compare semantic inputs, operations, and expected outputs. Platform
objects such as AppKit views, Ghostty surfaces, PTY handles, file locations,
socket transports, and privileged executors do not belong in these fixtures.
