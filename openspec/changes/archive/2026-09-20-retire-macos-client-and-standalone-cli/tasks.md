# Tasks

## 1. Consumer inventory and channel boundary

- [x] 1.1 Record the audited Apple, shell-core/FFI, Host, credential, Host Mount, sandbox, store, Cargo, CI, and release consumers in the change design and verify every retained path has an owner.
- [x] 1.2 Update `InstallChannel` and Host launch-label tests so standalone stable/dev names no longer depend on an app bundle identifier; verify focused `alan` tests pass.

## 2. Standalone CLI/Host distribution

- [x] 2.1 Add the repository-local standalone installer for `alan`, `alan-os-host`, and `alan-os-host-dev`, with explicit destination and non-owned-file refusal; verify an isolated temporary install and `alan --version` succeed.
- [x] 2.2 Add the standalone release archive assembler and manifest/checksum output; verify a local archive contains only the required executables and metadata.
- [x] 2.3 Replace `just install`, `just uninstall`, and `just release` app workflows with the standalone commands; verify `just --list` exposes no app-bundle installer or runner.

## 3. Remove retired delivery consumers

- [x] 3.1 Remove app-only Sparkle, appcast, notarization, app-bundle, cask, and dev-channel release scripts and secrets whose consumers were confirmed in 1.1; verify no active `just`, CI, or release path invokes them.
- [x] 3.2 Update CI build/upload jobs and repository quality scripts to build/check standalone CLI and Host binaries without an Xcode archive; verify the affected workflow YAML and scripts pass their shell checks.
- [x] 3.3 Replace current install/release/testing docs and README guidance with standalone CLI/Host commands while leaving archived history unchanged; verify active references no longer prescribe Alan.app, Sparkle, appcast, or embedded CLI delivery.

## 4. OpenSpec and canonical documentation

- [x] 4.1 Validate the proposal, design, spec deltas, and task list with OpenSpec; verify `openspec validate retire-macos-client-and-standalone-cli --type change --strict` passes.
- [x] 4.2 Sync the completed delta into canonical specs after implementation is merged; verify no canonical requirement still presents the retired app distribution as supported.

## 5. Verification and delivery

- [x] 5.1 Run `just quality`, focused `alan`/`os-host` tests, standalone distribution checks, and workspace tests; record failures as scoped follow-up work rather than weakening the gate.
- [x] 5.2 Mark the branch ready for review and request Codex review; verify all review comments are classified by root cause, fixes are tested, and resolved comments produce no new major findings.
- [x] 5.3 Merge the reviewed branch and archive the change only after canonical spec synchronization; verify the merged commit, clean worktree, and archive path on the latest `main`.
