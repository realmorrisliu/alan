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
- Replace the current Ghostty-owned process boundary on the implementation
  branch before merging the feature to `main`.

**Non-Goals:**

- Do not rewrite Alan's terminal UI, sidebar, split model, or terminal surface
  controller.
- Do not implement a terminal emulator from scratch.
- Do not introduce Linux, Windows, or cross-platform PTY ownership support in
  this change; the implementation target is the macOS Apple client runtime.
- Do not claim terminal processes survive app quit unless a later daemon-owned
  runtime explicitly provides that capability.
- Do not replace all Ghostty APIs in one change; unsupported attachment seams
  must be identified and either added to the fork or guarded as blockers.
- Do not include the privileged helper or managed-user PTY provider in this
  change; that provider depends on this runtime boundary and belongs in its own
  follow-up change.

## Decisions

1. Make Alan the PTY and process owner.

   `AlanTerminalRuntimeService` should create an Alan-owned
   `AlanTerminalPtyHandle` for each terminal ContentInstance. That handle owns
   PTY allocation, child launch, process group metadata, resize propagation,
   stdin delivery, EOF, signal requests, exit observation, and bounded transcript
   capture. Host views and Ghostty adapters receive only the attachment
   interface they need.

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

3. Vendor the Ghostty fork as a pinned submodule under `third_party/ghostty`.

   Add a repository-managed submodule for an Alan-maintained Ghostty fork rather
   than requiring each developer to provide an arbitrary external checkout.
   Setup scripts should build or reuse artifacts derived from the pinned
   revision and report drift when artifacts do not match the submodule source.
   The submodule lives at `third_party/ghostty` because Ghostty is a repo-level
   source dependency, while generated Apple artifacts remain under
   `clients/apple/` or cache directories.

   Alternative considered: keep `ALAN_GHOSTTY_REPO` pointing to a developer's
   local checkout. That is useful for experimentation, but it makes review,
   bisecting, and CI reproduction depend on untracked local state.

4. Build local GhosttyKit artifacts as macOS-only developer artifacts.

   Alan's first Ghostty embedding target is the macOS terminal host. The setup
   script should therefore default to Ghostty's `native` xcframework target
   instead of `universal`, because Ghostty's current `universal` target includes
   iOS and iOS simulator slices that Alan does not need. Local artifacts should
   record the xcframework target and SIMD setting in cache metadata so setup
   checks can detect stale links.

   The current macOS 27 beta toolchain also requires two developer-build
   guardrails: Zig dependency downloads run with proxy variables cleared because
   Zig 0.15.2 receives `400 Bad Request` through the local HTTP proxy, and
   `simd=false` is the default because Zig 0.15.2's bundled libc++ does not
   compile Ghostty's SIMD C++ path against the macOS 27 SDK. These are local
   build constraints, not dependency mirrors or product fallbacks.

   Alternative considered: build Ghostty's `universal` target by default. That
   increases local setup fragility by requiring iOS slices and extra toolchain
   paths before Alan has a use for them.

5. Replace the process owner on the implementation branch.

   The implementation should happen on a dedicated feature branch where terminal
   ContentInstances are cut over to the Alan-owned PTY runtime before the branch
   is considered mergeable. The new PTY runtime, Ghostty attachment adapter, and
   process controller should still have fake implementations so most lifecycle
   and control-plane tests do not require a live Ghostty renderer.

   Alternative considered: introduce a long-lived selector between the current
   process owner and the Alan-owned PTY runtime. That lowers short-term
   implementation risk, but it adds a fallback mode that the product does not
   need if the work is isolated and verified before merge.

6. Keep app-quit semantics honest.

   Alan-owned PTY/process state improves in-process control, but it does not by
   itself make terminal sessions survive Alan app termination. The restore
   contract should continue to create new terminal runtimes from persisted
   snapshots unless a later daemon-backed PTY owner is introduced.

7. Make the implementation reviewable through staged runtime seams.

   The implementation should not start by patching the live Ghostty bridge in a
   single large cutover. First, terminal launch should become an explicit
   `AlanTerminalBootRequest` carrying executable, arguments, working directory,
   environment, launch strategy, and non-secret profile metadata. Then Alan can
   introduce fake PTY/process runtime handles and focused tests before adding a
   real Darwin PTY backend. The Ghostty fork attachment seam is a hard gate for
   production UI cutover, because the current Ghostty surface API only accepts
   command/cwd/env launch configuration and does not expose an external PTY
   endpoint for Alan to supply.

8. Restrict this change to the macOS app-process PTY owner.

   The first implementation target is the Apple client on macOS. Alan may add
   other PTY owners later, but this change should not create Linux, Windows, or
   cross-platform abstractions. The runtime in this change owns Darwin PTYs
   inside the macOS app process and attaches GhosttyKit as a macOS renderer.

## Risks / Trade-offs

- Ghostty may not expose a clean external-PTY attachment seam -> Patch the
  Alan-maintained fork in small, reviewed slices and keep the implementation
  branch unmerged until the seam is proven.
- Submodule setup can make builds more complex -> Keep generated artifacts
  cached outside normal source diffs and make setup errors explicit.
- PTY/process ownership touches security-sensitive local execution paths ->
  Keep launch environments, profiles, signals, and file descriptors behind
  narrow runtime APIs with focused tests.
- One-step replacement has a larger branch-local blast radius -> Keep the work
  isolated on the implementation branch and require focused fake-runtime,
  Ghostty integration, and manual terminal UI verification before merge.

## Ghostty Fork Seam Finding

The pinned fork currently exposes the embedded surface API through
`include/ghostty.h` and `src/apprt/embedded.zig`. `ghostty_surface_config_s` /
`apprt.Surface.Options` accepts platform data, cwd, shell command, environment,
initial input, and wait-after-command behavior. It does not accept an externally
owned PTY file descriptor or stream endpoint.

Internally, Ghostty's terminal IO layer routes through `termio.backend.Kind`,
which currently exposes only the `.exec` backend. That backend owns subprocess
creation, PTY allocation, read-thread setup, resize, writes, process watching,
and process info reporting. Therefore the minimal fork seam is not an Alan-side
bridge-only change. The fork needs an external-PTY attachment path that either:

- adds a new `termio.backend` variant for externally owned PTY fds, or
- extends the existing exec path with an explicit externally owned PTY mode that
  bypasses subprocess launch while preserving renderer/protocol IO, resize,
  output ingestion, and lifecycle reporting.

The preferred patch is a new backend variant because it keeps Ghostty-owned exec
semantics separate from Alan-owned process semantics. The embedded C API should
then expose a small fd/ownership option, and Alan should fail integration checks
when that option is unavailable instead of falling back to `command` launch.

## Migration Plan

Implement the change in reviewable slices. Each slice should leave the tree in a
validated state and avoid mixing managed-user helper work into the PTY runtime
boundary.

1. Dependency source slice:
   - Add `third_party/ghostty` as the pinned Alan-maintained Ghostty fork
     submodule.
   - Update Ghostty setup scripts to prefer the submodule and keep
     `ALAN_GHOSTTY_REPO` as an explicit development override.
   - Record source revision/cache metadata so setup checks can report missing or
     stale artifacts.
   - Default local artifacts to macOS `native` and `simd=false`, clear proxy
     variables for Zig dependency downloads, and patch the fork's Metal build
     step to resolve `metal`/`metallib` through `xcodebuild -find-executable`
     instead of relying on `xcrun` stubs.

2. Boot request and fake runtime slice:
   - Introduce `AlanTerminalBootRequest` and route Terminal Profile resolution
     into structured launch data instead of only a renderer command string.
   - Add Alan PTY/process runtime protocols plus fake runtime handles for tests.
   - Verify launch metadata, input, resize, EOF, signal result, exit result, and
     bounded transcript behavior without requiring Ghostty.

3. Darwin PTY backend slice:
   - Implement the app-process-owned PTY backend for ordinary local terminal
     launches.
   - Own PTY allocation, child launch, process group/session setup, cwd/env
     projection, nonblocking IO, window-size updates, input delivery, EOF,
     signal delivery, wait/reap, and bounded transcript capture.
   - Prove this backend with focused non-UI shell tests before connecting it to
     Ghostty rendering.

4. Ghostty fork attachment slice:
   - Patch the Alan-maintained Ghostty fork with the minimal external-PTY
     attachment support Alan needs.
   - Regenerate or relink `GhosttyKit.xcframework`, resources, terminfo, and
     headers from the pinned fork revision.
   - Add a clear integration check that reports unsupported attachment seams
     instead of silently falling back to Ghostty-owned process launch.

5. Alan Ghostty bridge slice:
   - Split renderer responsibilities from PTY/process runtime ownership in the
     Apple bridge.
   - Attach Ghostty rendering to `AlanTerminalPtyHandle` instances supplied by
     the runtime service.
   - Preserve renderer readiness, surface lifecycle, scrollback behavior, input
     translation, and terminal metadata projection while Alan remains the source
     of process truth.

6. Production cutover slice:
   - Make terminal ContentInstance construction create the Alan-owned PTY runtime
     before renderer attachment.
   - Remove the normal Ghostty-owned command/cwd/env launch path from Alan
     terminals.
   - Do not keep a long-lived runtime selector or fallback process owner.

7. Verification and archive slice:
   - Run focused fake-runtime tests, Darwin PTY backend tests, Ghostty integration
     checks, Apple shell contract checks, `openspec validate`, and manual Alan dev
     terminal verification.
   - Sync accepted spec deltas into `openspec/specs/` and archive the change
     only after implementation and verification are complete.

Rollback before merge is to abandon or revert the implementation branch. After
merge, rollback is a normal revert of the Alan-owned PTY runtime change set and
Ghostty dependency setup.

## Open Questions

None.
