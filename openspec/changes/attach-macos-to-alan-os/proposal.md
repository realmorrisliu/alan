## Why

Alan for macOS currently owns a native terminal shell but does not attach to
the system-level Alan OS. Once Host and Service Manager exist, macOS should
become a renderer/native-capability host over aP without acquiring Agent
Execution Engine, Kernel, or Process lifecycle authority.

## What Changes

- Attach stable/dev macOS apps to their matching system Alan OS Unix socket and
  enter through a Local Entry Service-created Shell Process.
- Add Agent ContentInstance support backed by an Agent Attachment, not an
  app-owned runtime.
- Persist Process Reference (`boot-id + PID`), renderer-owned stream offsets,
  and presentation only; verify boot identity and `/proc` before reattachment.
- Restore live Agent views across macOS app restart while Alan OS continues;
  show terminal or unavailable state without recreating Processes after exit or
  Host restart.
- Make closing Pane, Tab, window, or ContentInstance detach only; stopping a
  Process is an explicit `/proc/<pid>/ctl` action.
- Keep Space/Tab/Pane/window actions in the host plane and Agent/Shell commands
  in the Alan OS command plane.
- Add native Host Mount authorization and Connection login/Keychain adapters
  that answer service-owned requests without exposing raw paths or secrets.
- Remove any macOS app-owned Alan OS boot path; app termination must not stop
  Agent Processes.

## Capabilities

### New Capabilities

- `macos-alan-os-attachment`: Local aP attachment, Shell entry, Agent
  ContentInstance projection, Process Reference validation, offsets, detach,
  explicit stop, and native-capability adapters.

### Modified Capabilities

- `alan-renderer-host-contract`: Require renderer attachment through Process
  files and caller-held offsets without runtime-state duplication.
- `macos-shell-content-containers`: Add Agent ContentInstance references while
  keeping layout ownership host-local.
- `macos-app-instance-lifecycle`: Decouple app lifetime from Alan OS Host and
  Agent Process lifetime.
- `macos-shell-workspace-persistence`: Persist attachment references and
  presentation, not Process or Agent Machine truth.
- `provider-connection-contract`: Route native login and Keychain operations as
  Host adapters for Connection Service.
- `host-directory-mounts`: Route macOS directory authorization through Host
  Mount Service.

## Impact

Touches the Swift shell model, persistence, control actions, socket/aP client,
Agent renderer, Host Mount UI, connection login, install-channel discovery, and
macOS verification. Depends on `implement-minimal-service-manager`; does not
redesign terminal ContentInstance ownership or package management.
