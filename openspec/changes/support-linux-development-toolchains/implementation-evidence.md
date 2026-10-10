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

## Supported PATH and per-command validation slice

Published mount-only head `5b3f514a` passed all 16 PR checks. The next slice
carries PATH as an explicit pure plan input, resolved by the Linux Host adapter;
startup and per-command construction share validation. Execution transports the
exact string as an argument after setup, while trusted setup retains its own
system PATH and absolute helpers. It removes directory executable scans and
the old fixed-order comparison. No Kernel/environment owner or dependency was added.

Two disposable providers of the same command verify both orders and relative
alias selection under the actual namespace runner. Their fake `mount`, `mkdir`
and `setpriv` never replace trusted setup. Invalid PATH, changed directory/root
aliases and non-UTF-8/NUL execution paths refuse before the fixture marker.
Unknown entries remain unavailable, including absent/empty directories, because
namespace mounts or scratch may otherwise make them executable providers.
Absolute aliases in remapped roots fail explicitly; identical roots retain safe
absolute aliases. Missing directories within known substrate are preserved.

The pre-review candidate passed 1409 engine tests; it does not qualify the later
revision. Added chained-alias regressions exposed a trailing-separator issue in
alias inspection (`alan-path-reviewed-engine.log`: 1409 passed, one failed, one
ignored). Component traversal now retains link inspection, including link chains,
cycles and trailing separators. The alias-slice suite `alan-path-alias-final-engine.log`
passed 1411 tests, zero failures, one existing ignored live-provider test in
13.95 seconds, with zero native early-return diagnostics. Receipt
`native-path-acceptance.json` freezes all six changed Rust source hashes, Linux
test binary and log digests. This remains a PATH slice, not a Rust build or
real-model development qualification. Current-head CI must qualify its commit.

The earlier cross-checkout commit attempt also left a duplicate Mac output
receipt in the Linux checkout registry. Under the existing admission lock,
byte equality and validation of the authoritative Mac receipt were verified;
only the foreign duplicate was moved to ignored
`target/misfiled-macos-quality-receipt-20261010.json`. Both output trees and the
authoritative registry were unchanged. Normal commits use the byte-identical
Mac verification checkout to avoid checkout-local Git environment contamination.

Linux all-target/all-feature engine Clippy with warnings denied passed on the
alias-slice sources (`alan-path-final-clippy.log`, 19.43 seconds).

### Actual Sandbox adapter and final candidate

Review added a child-process test through the actual Sandbox API: a supported
current PATH appears byte-for-byte in `printenv PATH`, and an unknown missing
entry refuses before a requested file effect. Child environments are explicit;
the parent environment is never mutated. An assertion on one executed child test
caught an initial incorrect test filter rather than accepting zero tests. The
next fixture used shell-variable expansion, which the existing conservative
parser intentionally rejects; it now uses `printenv` without weakening parsing.
Unsuccessful attempts remain in `alan-path-adapter-final-engine.log` and
`alan-path-adapter-verified-engine.log`.

The final alias checks also reject an external link chain that returns into the
same substrate and parent-traversing targets. The first macOS identity-root
fixture failed because `/var` resolves to `/private/var`; it now uses canonical
roots and confined absolute targets, matching actual plan normalization. That
failure remains `target/linux-path-current-macos-tests.log` in the Mac verification
checkout. Verified Mac suites pass 33 namespace tests and six Sandbox adapter
tests, including non-Linux refusal, without ignored cases.

Final native engine suite `alan-path-entry-final-engine.log` passes 1412 tests,
zero failures, one existing ignored live-provider test, in 16.26 seconds. No
mount, network or current-PATH native test returned early. Receipt
`native-path-entry-acceptance.json` freezes seven Rust source hashes, the actual
Linux test binary and log digest; earlier receipts retain their earlier scope.
Normal commit quality and current-head CI remain separate gates. Tasks 2.1/2.2
are closed for supported PATHs; installed Rust/runtime and private caches remain
tasks 2.3/2.4, so this is not complete Linux development qualification.

Final Linux all-target/all-feature engine Clippy with warnings denied also
passes (`alan-path-entry-final-clippy.log`, 19.69 seconds).

## Private environment and real read-only Rust build slice

Published PATH head `4151f93c` passed all 16 distinct PR checks. This slice
uses the existing per-command tmpfs and Process lifecycle for private home,
Cargo/rustup/tmp/XDG cache and read-only build output. It avoids explicitly
mounted scratch descendants, mode 0700, and keeps writable project output/config
behavior. New native tests run the same plan twice and assert fresh caches,
retained delegated contents, private canary absence and unchanged Host fixtures.
The cancellation regression now writes private Cargo scratch before cancellation
and asserts that its descendant cannot produce the delayed project effect.

Rustup first failed because `/proc/self/exe` was absent. The runner now mounts
fresh read-only procfs for its private PID namespace and execs chroot so PID 1
cannot expose an outside-root setup helper. A private canary is absent through
`/proc/1/root`, the Host test PID is absent, and proc writes are denied. Native
`/proc` mount conflicts refuse before effects; virtual Alan Process paths remain
excluded. Real compilation exposed missing system cc alternatives and GCC
libexec content; both system directories join the existing read-only substrate.

The opt-in native Rust fixture explicitly supplies task-owned proxy and runtime
inputs to the existing plan. Rustc/Cargo/rustdoc report 1.97.0, an independently
read-only mounted local dependency compiles, and both the project unit test and
an actual doctest pass under `cargo test --offline --locked`. Project Cargo
configuration is required by a compile-time assertion. All six project/dependency
files and runtime rustc bytes remain unchanged; no project/dependency target or
personal Cargo credentials are created. This closes task 2.4, not task 2.3:
automatic Host adapter selection/projection and complete Linux qualification are
still open. Portable CI without explicit fixture inputs skips this Rust case
and must not count that as native qualification.

First failures remain in `alan-private-env-red.log` (HOME absent),
`alan-private-env-rust-first.log` (proc absent), `alan-private-env-proc-first.log`
(cc absent), `alan-private-env-engine-final.log` (GCC liblto plugin absent),
`alan-private-env-runtime-final.log` (rustdoc proxy absent) and
`alan-private-env-rustdoc-final.log` (fixture omitted Cargo's separate rustdocflags).
The last correction adds legitimate project rustdoc configuration and retains
both compile/config assertions rather than suppressing documentation tests.
Final `alan-private-env-complete-engine.log` passes 1416 tests, zero failures,
one existing ignored live-provider test, in 22.76 seconds. No relevant native
case returned early; successful Rust stdout/stderr are retained in that log.

The task-owned proxy originally used a hardlink to personal rustup. Before any
native use it was replaced by an independent byte-identical copy; the original
binary bytes/defaults were unchanged, but its link metadata was touched by that
initial attempt. `rust-proxy-provisioning.json` records this correction. The
owned provisioning helper now copies instead of linking; no whole Cargo home or
Rust management directory is projected. Fixture provisioning and explicit lower
runner execution are not real-model development or automatic runtime support.

Native source/binary/log identity is frozen in `native-private-env-acceptance.json`.
Mac verification, quality, Clippy and new-head CI are separate gates below.

Linux all-target/all-feature engine Clippy with warnings denied passed in
19.62 seconds (`alan-private-env-complete-clippy.log`). The byte-identical Rust
snapshot passes 34 macOS namespace tests and six Sandbox adapter tests, zero
failures/ignored. Source-size and architecture gates pass. Pinned OpenSpec 1.4.1
strict validation remains the normative CLI gate, separate from the newer global
CLI. A normal commit will run full quality and standalone distribution; current
head CI is required before considering this slice reviewed for delivery.
