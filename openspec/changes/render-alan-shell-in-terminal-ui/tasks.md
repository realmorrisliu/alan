## 1. Runtime namespace launch surface

- [x] 1.1 Expose the namespace-native runtime launch handle from `alan-agent-engine`, including runtime lifetime control, the mounted aP root transport, and the concrete root-agent path needed by renderer hosts
- [x] 1.2 Add focused tests proving the launch helper boots a live namespace-backed root agent surface that a shell client can read and write

## 2. File-backed terminal UI path

- [x] 2.1 Add an explicit `alan-terminal-ui` backend selector in `crates/alan` / `crates/tui` so the existing daemon-backed path remains available while a local file-backed path can be launched
- [x] 2.2 Implement the local file-backed TUI runner that tails `<agent>/io/output`, writes `<agent>/io/input`, and writes `interrupt` to `/proc/<pid>/ctl` through `alan-shell`/aP operations
- [x] 2.3 Add focused tests for the file-backed runner state and keep the daemon-backed compatibility runner passing

## 3. Verification and change hygiene

- [x] 3.1 Run focused Rust tests covering the new runtime launch surface and file-backed terminal UI path
- [x] 3.2 Run `openspec validate render-alan-shell-in-terminal-ui --strict`
- [ ] 3.3 After merge, sync `alan-renderer-host-contract` and `rust-inline-tui` delta specs into `openspec/specs/` before archiving the change
