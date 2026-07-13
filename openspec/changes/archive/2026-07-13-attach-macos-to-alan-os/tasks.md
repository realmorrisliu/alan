## 1. Attach Alan for macOS to the system Host

- [x] 1.1 Add Swift aP wire client/import adapter using the matching stable/dev Unix endpoint
- [x] 1.2 Add platform Host start/discovery, channel, peer, boot-ID, and readiness validation
- [x] 1.3 Obtain one app-level Shell Process through Local Entry Service and render basic namespace Shell behavior
- [x] 1.4 Prove app/window exit releases connections without stopping Alan OS or its Processes

## 2. Add Agent attachment domain state

- [x] 2.1 Add Process Reference with boot ID and PID plus qid/reference verification
- [x] 2.2 Add Agent Attachment with caller-held output/request/action/UI offsets
- [x] 2.3 Add Agent ContentInstance payload containing only reference, offsets, and presentation
- [x] 2.4 Guard shell persistence against Tape, Agent Machine, provider, Tool, socket, raw Host path, or secret state

## 3. Build the file-backed Agent renderer

- [x] 3.1 Hydrate `/proc/<pid>` and `/agent/<pid>` through Alan Shell/aP file operations
- [x] 3.2 Tail AgentFS streams from saved offsets with overlap dedupe and visible retention-gap handling
- [x] 3.3 Write input, request responses, machine control, interrupts, and explicit stop through files
- [x] 3.4 Support multiple ContentInstances/renderers attached to one Process with independent fids and offsets

## 4. Implement restore and lifecycle semantics

- [x] 4.1 Restore live Agent views after SwiftUI view and macOS app recreation without creating Processes
- [x] 4.2 Render exited Process evidence and missing/boot-mismatched Process unavailability without PID redirection
- [x] 4.3 Make Pane, Tab, window, ContentInstance, and app close detach only
- [x] 4.4 Make Stop Process an explicit `/proc/<pid>/ctl` action with distinct confirmation/copy
- [x] 4.5 Test layout move, duplicate view, last-view close, app crash/relaunch, Host restart, and PID reuse

## 5. Wire native capability adapters

- [x] 5.1 Observe Host Mount Service requests and connect native directory authorization/security scope
- [x] 5.2 Return bounded hostfs export results without projecting raw Host paths into Alan OS
- [x] 5.3 Observe Connection Service requests and connect browser/device login plus Keychain
- [x] 5.4 Return only opaque credential references and remove local profile/default authority

## 6. Remove app-owned runtime paths

- [x] 6.1 Remove any macOS app-owned Kernel, Agent Execution Engine, Root Agent, or Alan OS boot path
- [x] 6.2 Keep Space/Tab/Pane/window commands host-owned and route Agent/Shell commands through Alan OS files
- [x] 6.3 Preserve existing terminal ContentInstance/Ghostty ownership without folding it into Agent attachment
- [x] 6.4 Add architecture guards against runtime-state duplication and app-owned Alan OS lifecycle

## 7. Verify product behavior

- [x] 7.1 Run Rust aP/attachment tests, shell-core/FFI tests, focused Apple shell tests, and strict OpenSpec validation
- [x] 7.2 Build and install a fresh `Alan Dev.app` without touching Alan stable
- [x] 7.3 Run fresh-relaunch UI smoke for Shell entry, Agent attach, detach, stop, restore, Host Mount, and Connection login request surfaces
- [x] 7.4 Verify stable/dev endpoint and System Store isolation in signed app builds

## 8. Review and archive readiness

- [x] 8.1 Submit after `implement-minimal-service-manager` is merged and archived
- [x] 8.2 Complete current-HEAD Codex review, zero unresolved threads, green CI, and delayed recheck before merge
- [x] 8.3 Sync macOS attachment deltas into canonical specs after implementation merge
- [x] 8.4 Archive only after source tests, rendered verification, implementation merge, and canonical sync are complete
