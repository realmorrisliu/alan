## Context

Alan already has a service-backed terminal runtime foundation: terminal
ContentInstances own stable runtime identity, host views attach as adapters, and
control-plane delivery is expected to be truthful. The remaining architectural
gap is lower in the stack. The current Ghostty integration still makes Ghostty's
runtime artifacts the practical source of truth for terminal process ownership,
which limits Alan's ability to send process-group signals, inspect terminal
activity, capture bounded transcript state, and evolve agent-facing terminal
semantics.

The restored direction is to make Alan own the PTY and child-process tree, then
attach Ghostty as the terminal renderer/protocol engine. Alan can still use
Ghostty's terminal quality, AppKit surface behavior, and rendering model, but
the process boundary becomes Alan-owned and testable.

## Goals / Non-Goals

**Goals:**

- Introduce an Alan-owned PTY runtime service for terminal ContentInstances.
- Keep terminal launch, resize, input, EOF, signals, process-group tracking,
  exit status, and transcript capture behind Alan-owned runtime handles.
- Adapt the Ghostty bridge so Ghostty consumes an Alan-provided PTY endpoint and
  reports renderer/protocol metadata without owning child-process truth.
- Vendor a pinned Alan-maintained Ghostty fork as a repository submodule and
  generate local build artifacts from that source.
- Preserve the current service-backed runtime while migrating one boundary at a
  time behind fakes and focused integration tests.

**Non-Goals:**

- Do not rewrite Alan's terminal UI, sidebar, split model, or terminal surface
  controller.
- Do not implement a terminal emulator from scratch.
- Do not claim terminal processes survive app quit unless a later daemon-owned
  runtime explicitly provides that capability.
- Do not replace all Ghostty APIs in one change; unsupported attachment seams
  must be identified and either added to the fork or guarded as blockers.

## Decisions

1. Make Alan the PTY and process owner.

   `AlanTerminalRuntimeService` should create an Alan-owned
   `AlanTerminalPtyHandle` for each terminal ContentInstance selected for the
   Alan-owned PTY runtime path. That handle owns PTY allocation, child launch,
   process group metadata, resize propagation, stdin delivery, EOF, signal
   requests, exit observation, and bounded transcript capture. Host views and
   Ghostty adapters receive only the attachment interface they need.

   Alternative considered: keep asking Ghostty for more process-control seams.
   That preserves the current shape but keeps Alan's lifecycle semantics behind
   renderer-specific APIs and makes control-plane behavior hard to verify.

2. Treat Ghostty as renderer/protocol adapter over an Alan PTY endpoint.

   The Ghostty bridge should attach to a PTY file descriptor or equivalent
   stream object supplied by Alan. Ghostty remains responsible for terminal
   protocol parsing, rendering, scrollback behavior, input translation, and
   AppKit surface callbacks. Alan remains responsible for the child process and
   authoritative terminal lifecycle.

   Alternative considered: fork Ghostty and keep its existing app/runtime model
   as the primary process owner. That would provide a local patch point, but it
   would not fix the ownership boundary.

3. Vendor the Ghostty fork as a pinned submodule.

   Add a repository-managed submodule for an Alan-maintained Ghostty fork rather
   than requiring each developer to provide an arbitrary external checkout.
   Setup scripts should build or reuse artifacts derived from the pinned
   revision and report drift when artifacts do not match the submodule source.

   Alternative considered: keep `ALAN_GHOSTTY_REPO` pointing to a developer's
   local checkout. That is useful for experimentation, but it makes review,
   bisecting, and CI reproduction depend on untracked local state.

4. Introduce the runtime behind testable protocols.

   Production code should keep the current Ghostty-backed service path until the
   Alan-owned PTY path is ready. The new PTY runtime, Ghostty attachment adapter,
   and process controller should have fake implementations so most lifecycle and
   control-plane tests do not require a live Ghostty renderer.

   Alternative considered: switch the whole terminal runtime in one PR. That
   increases risk across input, scrollback, selection, rendering, and close
   behavior at the same time.

5. Keep app-quit semantics honest.

   Alan-owned PTY/process state improves in-process control, but it does not by
   itself make terminal sessions survive Alan app termination. The restore
   contract should continue to create new terminal runtimes from persisted
   snapshots unless a later daemon-backed PTY owner is introduced.

## Risks / Trade-offs

- Ghostty may not expose a clean external-PTY attachment seam -> Patch the
  Alan-maintained fork in small, reviewed slices and keep the current runtime
  path available until the seam is proven.
- Submodule setup can make builds more complex -> Keep generated artifacts
  cached outside normal source diffs and make setup errors explicit.
- PTY/process ownership touches security-sensitive local execution paths ->
  Keep launch environments, profiles, signals, and file descriptors behind
  narrow runtime APIs with focused tests.
- Dual runtime paths can drift -> Add contract checks that both paths preserve
  truthful delivery, metadata projection, and close behavior during migration.

## Migration Plan

1. Add the Ghostty fork submodule and update setup scripts to derive artifacts
   from the pinned revision.
2. Add Alan PTY/process runtime protocols plus fake implementations.
3. Implement the Alan-owned PTY runtime path behind a disabled or internal
   runtime selection seam.
4. Adapt the Ghostty bridge to attach to Alan-owned PTYs.
5. Move terminal launch, resize, input delivery, signal delivery, and exit
   observation to the Alan runtime path.
6. Run focused fake-runtime tests, Ghostty integration tests, and manual
   terminal UI verification before making the Alan-owned path default.
7. Remove the legacy Ghostty-owned process path after parity is proven.

Rollback before defaulting is to disable the Alan-owned PTY runtime selection
and keep using the existing Ghostty-backed runtime. After defaulting, rollback
requires restoring the prior runtime selector and dependency setup.

## Open Questions

- Should the first Alan-owned PTY runtime live only inside the macOS app process,
  or should a later slice move ownership into an Alan daemon for cross-app
  terminal continuity?
- What exact Ghostty fork patch is needed to support an external PTY endpoint
  without carrying unrelated Ghostty app architecture?
- Where should the submodule live: `third_party/ghostty`,
  `clients/apple/vendor/ghostty`, or another repo-standard dependency path?
