# Initial environment and fixture evidence — 2026-10-10

This is setup and baseline evidence, not delivered Linux toolchain support.
The production baseline is main `faf747c2`; the Linux branch now contains an
unmerged native mount-safety repair. The change's four planning artifacts
are complete and CI-pinned OpenSpec 1.4.1 strict change validation passes.

## Fresh environment inventory

OrbStack ubuntu is aarch64, UID 501, kernel
`7.0.14-orbstack-00380-ga7e0a2dc9535`. Shell and compiler are present; system git
is absent. Installed Rust 1.97 executes through the existing rustup proxies.
PATH includes user and OrbStack directories outside the fixed reified substrate.
No system packages, personal Rust defaults or Alan stores were changed.
Inventory receipt: output-presentation worktree
`target/linux-environment-20261010.json`.

## Task-owned development fixture

Unprivileged apt package download and `dpkg-deb --extract` placed git 2.53.0 and
its runtime files in a unique owned Linux qualification directory. This is not
system installation. Package digests, fixture identity and source byte hashes
are recorded in its `provisioning.json`; local launch receipt is
`target/linux-fixture-provisioning.json`.

A small project depends on a separate local Rust crate. Its test expects three,
but production returns one. Real `cargo +1.97.0 test --offline --locked` compiled
both crates and failed that assertion with exit 101. The frozen git baseline is
`06d88f324cb0c0925e7d87ac08059f23a17f4bd1`; status and diff are empty after the
run. Cargo home and compiler output are fixture-private. This is an intentional
RED task setup, not an Alan execution failure or a supported-backend pass.

The first fixture commit warned that the compiled system git exec path could not
find the maintenance helper. The extracted package contains it; explicit fixture
`GIT_EXEC_PATH` now resolves that package's git-core. `git --exec-path`, status and
diff verify the corrected setup. Keep this observation: executable version alone
does not establish complete tool runtime support.

A baseline build of the existing native runner tests completed through the
repository's managed build-output lease, with private Cargo home and Rust 1.97.
Both reported tests returned early: runner readiness failed while binding
`/usr/lib` to the isolated `/lib`; toolchain readiness was not reached, and
selection fell back to Landlock. Thus there were zero successful native smoke
executions, despite the test harness's `2 passed` summary.

The minimal private user/mount namespace reproduction exits 32 on a plain
`--bind /usr/lib`. A library subdirectory without the inherited child mount
binds successfully; `--rbind /usr/lib` and `--rbind -o ro=recursive /usr/lib`
both exit zero. Mount information identifies an inherited read-only OrbStack
kernel-module submount beneath `/usr/lib`. Receipts are
`target/linux-lib-bind-red.json` and `target/linux-lib-bind-hypotheses.json`;
the baseline log is the fixture's `alan-runner-baseline.log`.

The repair requires an actual native root/child write-denial regression and a
capability probe that rejects helpers ignoring recursive read-only options.
No toolchain-build acceptance has passed. Remaining product rows are not run.

## Native mount-safety implementation slice

The controlled outer/inner user+mount namespace regression failed on the old
runner with exit 101 at the declared read-only bind, before user execution.
Receipt: `alan-submount-red.log`. The first recursive-option candidate then
failed qualification: the installed helper used its legacy mount API, which
left child mounts writable or rejected the operation despite a zero-exit
library-only bind. Private fixture probes retain these unsuccessful attempts
in `recursive-readonly-probes.json`, `mount-api-probes.json` and
`mount-options-probes.json`. A direct kernel syscall probe could protect the
tree, distinguishing kernel capability from helper behavior.

The implementation uses existing recursive bind plus per-entry read-only
remount of the private kernel mount table, shared by capability probe and
execution. Trusted absolute printf decodes kernel escapes and a sentinel
preserves trailing newlines. It adds no dependency, public option or execution
owner. All root/device-file bind semantics, authority validation, command
capture and cancellation owners remain in place.

The final nested fixture validates read-only root/child denial, writable root
and child access, inherited read-only children within writable trees, escaped
namespace/Host paths, unchanged original canaries and the exact authorized
marker. A broken-helper test simulates a successful child remount that leaves
the child writable; capability selection rejects it. Existing native visibility,
network-denial and cancellation tests actually execute after the repair.

The affected tools suite passed 308/308 on real OrbStack Linux without native
runner early-return skips (`alan-mount-final.log`, 13.06 seconds). A nearby
Landlock test was also corrected to use a unique private outside-mount canary
and argument-based paths instead of deleting a fixed file in HOME. Its first
temporary-directory candidate failed because Landlock intentionally permits
ambient `/tmp`; the fixture now uses a unique private HOME child outside that
allowance and never deletes a preexisting user path. That initial full-engine
run retained 1406 passed, one failed and one ignored in
`alan-mount-engine-final.log`. The corrected full Linux engine library suite
passed 1407 tests, zero failures, one explicitly ignored live-provider test,
in 14.27 seconds (`alan-mount-engine-verified.log`). There were zero native
early-return skip diagnostics. `native-mount-acceptance.json` freezes all four
source-file SHA-256 hashes and the actual Linux test binary digest. The ignored
test requires live OpenAI credentials/model/network and is not part of this
mount-safety acceptance.

macOS `just quality` passed, including standalone distribution. Cross-host
artifact registry entries cannot be verified against another host's filesystem
identity: the first shared-owner Mac invocation correctly refused the guest
output. A dedicated linked checkout now owns Mac quality output via the existing
explicit `ALAN_BUILD_OWNER` / `ALAN_QUALITY_TARGET_DIR` inputs. That first full
quality invocation uses the Linux implementation checkout as workspace. Git
hooks retain checkout-local Git environment and cannot safely use another
checkout as build owner, so the normal final commit runs from the Mac verification
checkout with a byte-identical indexed tree (verified with `git write-tree`).
No output was borrowed,
deleted or reassigned. Logs: `target/linux-mount-quality.log` (refusal) and
`target/linux-mount-quality-macos.log` (pass). The final normal commit hook must
recheck quality. Pinned strict change validation, source-size and architecture
gates pass. PATH transport, selected Rust/runtime, private Cargo scratch and
confined real build qualification remain open.

Linux engine all-target/all-feature Clippy with `-D warnings` passed
(`alan-mount-clippy.log`). Caller review covers both declaration/substrate loops,
trusted-helper resolution and argument assembly, startup and per-command probes,
setup-marker ordering and shared synchronous/cancellable setup. Root/device binds
stay nonrecursive because their fresh/file sources have no child mounts.
