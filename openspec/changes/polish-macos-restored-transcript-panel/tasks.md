## 1. Restored Panel Presentation

- [x] 1.1 Align `RestoredTerminalTranscriptView` text origin with the live terminal text column.
- [x] 1.2 Match terminal-like monospace font size, weight, row height, color, and horizontal scrolling behavior.
- [x] 1.3 Keep the restored panel visually distinct with a quiet background and separator while avoiding warning-banner or card-heavy styling.
- [x] 1.4 Preserve bounded, stable panel height based on restored transcript rows.

## 2. Restored Transcript State Clearing

- [x] 2.1 Add a shell-state mutation that removes a terminal content's restored transcript snapshot by content ID.
- [x] 2.2 Persist the shell state after restored transcript dismissal so the snapshot does not return after restart.
- [x] 2.3 Add runtime registry and runtime service APIs to evict restored transcript cache by content ID.
- [x] 2.4 Ensure view reconstruction, tab switching, and pane remounting do not reintroduce a cleared restored panel.

## 3. Clear Intent Routing

- [x] 3.1 Route terminal `Ctrl-L` through restored transcript dismissal while preserving Ghostty key delivery.
- [x] 3.2 Add or reuse an explicit Alan Clear command, including Cmd-K or menu routing, that dismisses the restored transcript and performs normal terminal clear behavior.
- [x] 3.3 Support typed `clear` dismissal only through a reliable terminal input or semantic-command seam; avoid output parsing or broad false positives.
- [x] 3.4 Keep unsupported raw escape-sequence clear detection out of the implementation unless Ghostty exposes a reliable signal.

## 4. Verification

- [x] 4.1 Add controller or shell-state tests for snapshot removal and manifest persistence after dismissal.
- [x] 4.2 Add runtime service tests for restored cache eviction.
- [x] 4.3 Add terminal host/controller tests for `Ctrl-L` and explicit Clear routing.
- [x] 4.4 Add typed `clear` tests if typed command recognition is implemented.
- [x] 4.5 Add focused restored panel layout tests or view-model assertions.
- [x] 4.6 Run targeted Apple shell/runtime tests and OpenSpec validation.
- [ ] 4.7 Perform a fresh Alan dev relaunch visual check after implementation and record evidence before claiming UI completion.

  Attempted `test-shell-ui-smoke.sh` with rebuilt and direct-launch smoke bundles. The app process launched, but the smoke window never appeared for capture in this environment, so visual evidence remains blocked.
