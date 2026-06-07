## Why

Alan's macOS terminal runtime still treats Ghostty as the place where child
process and renderer ownership meet. That makes terminal process control,
signal delivery, transcript capture, and future agent-readable terminal
semantics depend on renderer-side seams that Alan cannot fully audit or evolve.

This change restores the Alan-owned PTY runtime direction: Alan owns terminal
child processes, PTYs, lifecycle metadata, and delivery semantics, while a
forked Ghostty dependency supplies the terminal renderer and protocol engine
through a reviewed integration boundary.

## What Changes

- Define an Alan-owned PTY/process runtime below terminal ContentInstances.
- Move shell launch, PTY allocation, process-group tracking, resize, input
  delivery, EOF, signal, and exit observation into Alan-owned runtime services.
- Make Ghostty attach to an Alan-provided PTY endpoint rather than acting as the
  primary owner of the child process tree.
- Add a repository-managed Ghostty dependency strategy: an Alan-maintained
  Ghostty fork is vendored as a git submodule, with generated framework,
  resources, and terminfo produced from that pinned source.
- Replace the current Ghostty-owned process boundary on the implementation
  branch; do not add a long-lived fallback runtime selector.
- Add verification requirements for PTY lifecycle, process-group signaling,
  renderer attachment, dependency pinning, and build/test reproducibility.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `macos-terminal-runtime-foundation`: Add Alan-owned PTY/process ownership and
  Ghostty renderer attachment boundaries.
- `macos-shell-terminal-lifecycle`: Add lifecycle semantics for Alan-owned
  terminal child processes, signal delivery, runtime replacement, and Terminal
  Profile launch ownership.
- `macos-shell-build-test-contract`: Add repository-managed Ghostty fork and
  submodule setup, validation, and integration-test requirements.

## Impact

- Apple client terminal runtime services, terminal host adapters, Ghostty
  bridge code, shell launch profiles, transcript capture, close guards, and
  control-plane text delivery.
- Repository dependency layout, likely through a `third_party/ghostty` or
  equivalent submodule path plus scripts that build or cache
  `GhosttyKit.xcframework`, `ghostty-resources`, and `ghostty-terminfo` from the
  pinned fork.
- Build documentation, CI or focused local checks for Ghostty dependency
  preparation, and tests that can run with fake PTY and fake Ghostty adapters.
