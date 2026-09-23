## Context

See proposal.md. The user has stopped using the desktop and authorized removal.
Source references and Cargo manifests show shell-core is consumed only by its
FFI crate and Apple tests/client. CLI, os-host and tui do not import either crate.

## Goals / Non-Goals

Remove tracked desktop implementation and its active maintenance obligations.
Do not uninstall applications, alter accounts/keychain/Host stores, delete ignored
local artifacts, or delete the independent third_party/ghostty checkout.

## Decisions

- Delete tracked clients/apple and shell-core/FFI, rather than migrate their
  pane/window/terminal-emulator model into Alan. Herdr owns those interactions.
- Keep existing credentials in crates/os-host/src/boot.rs, Host Mount adapter
  in crates/os-host/src/host_mounts.rs and the runtime sandbox enforcement.
  The Swift privileged helper belongs to retired managed-user PTY behavior;
  it is not the Rust Tool sandbox owner.
- Remove workspace entries, exact dependency-ledger entries and Just recipes.
  Change guards to scan existing Rust/installer paths and reject reintroduced
  tracked desktop sources. Ignored build products are not supported source.
- Remove all requirements in the 21 desktop-only capabilities listed in the
  proposal; shared platform/runtime specs remain authoritative. At post-merge
  sync, delete empty capability files and update active links, retaining Git
  history and immutable archives.
- Existing broad programmable-client/interaction drafts are superseded inventory.
  Tracer-bullet tasks own current work; no GUI restart or generic packaging
  implementation is promised.

## Risks / Trade-offs

- Hidden Rust dependency → Cargo workspace build and architecture gate.
- A guard silently scanning absent paths → retarget scans and run quality.
- Over-removal of platform safety → Host mount/keychain/sandbox tests; no edits
  to their runtime owners.
- Stale desktop requirements → explicit removal deltas and post-merge sync.

## Migration Plan

Apply the deletion and validate locally. Review/merge with current-head CI,
then sync deltas and retire empty capabilities before archive. Source can be
restored from the parent Git revision; this change does not mutate user data.
