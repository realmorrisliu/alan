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

The private-environment slice was committed as `8d9e371` and published to draft
PR #1046. All 16 distinct checks passed on that exact head. The preceding pending
publication paragraphs are historical; they do not qualify the next candidate.

## Automatic standard-Rustup Host adapter slice

The existing Host adapter now inspects standard Rustup proxies, whitelisted
selection settings and bounded installed runtime roots. It preserves original
PATH and explicit Rustup override while creating an independent private catalog;
neither Host Cargo home nor the Rustup management directory is mounted. Selection
metadata, component manifests and executable digests are revalidated before
admission. Missing components/targets and selected fmt/Clippy helper loss/escape
have explicit refusal paths. The pure plan still performs no Host inspection.

Actual Sandbox execution first exposed its default writable-cwd admission guard,
which blocked a valid read-only Cargo build. Bash preflight and execution now share
the native namespace readable-cwd decision; weaker backends retain their existing
writable-cwd requirement. Private build output and read-only mounts enforce the
source boundary. Host inspection/hashing runs on a blocking worker without user
execution, preserving cancellable Process ownership.

Three actual namespace-backed Sandbox cases exercise project-file, original
environment and command `+selector` precedence. Each reports rustc/cargo/rustdoc
1.97.0, compiles the independently read-only local dependency, passes one unit
test and one real doctest, runs `cargo fmt -- --check` and offline Clippy with
warnings denied. Required project rustflags/rustdocflags remain effective. All
six source files remain byte-identical and neither project nor dependency gets
a target directory. Original Rustup settings and proxy bytes remain unchanged.

Five native negatives refuse before a writable project's `marker` can be created:
missing environment selector, missing command selector, missing project component,
a project selection changed after successful execution, and a runtime-root alias.
Portable regressions also check nearest directory override/legacy-file precedence,
undelegated parent configuration, runtime loss and byte changes, unknown wrappers,
private metadata exclusion, changed helper authority and changed component metadata.
Review found an escaping component-manifest parent could be read by Host inspection;
it now refuses before metadata reading and has a dedicated regression. Linux's
existing full shell/protected-path checks still reject opaque/control-flow scripts;
the exploratory heredoc case is unsupported, not native development acceptance.

First attempts remain separately recorded: `alan-rustup-first.log` (TOML document
parsing and a pure scratch contract regression), `alan-rustup-adapter-first.log`
(read-only cwd admission), `alan-rustup-components-final.log` (wrong test enum),
`alan-rustup-components-checked.log` (fixture missing fmt proxy),
`alan-rustup-helper-proxies.log` (older fixture's exact four-entry assertion),
`alan-rustup-reviewed-engine.log` (unsupported heredoc/control flow), and
`alan-rustup-final-clippy.log` (unused import and two collapsible branches).
The task-owned proxy now contains eight known entries. Only four helper symlinks
were added to that owned directory; `rust-helper-proxy-provisioning.json` records
the operation. The earlier independent-copy/hardlink metadata caveat remains.

The pre-platform-import native suite `alan-rustup-lint-checked-engine.log` has
1425 passed, zero failed, one existing live-provider ignore, 64.35 seconds.
Final source `alan-rustup-final-crosshost-engine.log` repeats 1425 passed, zero
failed, one ignore in 71.32 seconds; no relevant native case returned early.
`native-rustup-acceptance.json` freezes eleven source hashes, binary/log identity,
original runtime/default/proxy evidence and the eight actual cases. The earlier
1424-pass pre-lint receipt remains `native-rustup-pre-lint-acceptance.json`, and
the pre-platform-import receipt is `native-rustup-pre-macos-export-acceptance.json`.
The byte-identical macOS snapshot passes 41 namespace tests and seven Sandbox
adapter tests, zero failures/ignores. Source-size and pinned strict OpenSpec
1.4.1 validation (69/69) pass. Linux all-target/all-feature engine Clippy with
warnings denied passed in 20.23 seconds before the final platform-import adjustment;
the final source passes in 32.85 seconds (`alan-rustup-final-crosshost-clippy.log`).
The initial macOS normal-commit gate caught an unused non-Linux import; the type
import now follows the same Linux/Unix-test boundary as the inspector. The next
normal commit passed full quality and standalone distribution without bypass;
receipts are `target/linux-rustup-quality-commit.log` and
`target/linux-rustup-quality-commit-final.log` in the macOS verification worktree.
New-head CI remains a separate gate before publication can qualify this candidate.

This closes task 2.3, not complete Linux development qualification or real-model
code authorship. Git/RED-GREEN, live dependency grant/revocation/escape, complete
developer-entry isolation/network and descendant cancellation remain open.
Portable CI without explicit native fixture inputs skips the real Rust case;
its green test count cannot replace the recorded native execution above.

## Native development and descendant lifecycle slice

The added opt-in adjacent Sandbox fixture uses task-owned git 2.53.0 and the
selected installed Rust 1.97 runtime through the normal enforcing execution API.
It provisions seven exact project/dependency files and a local Git baseline;
Host provisioning is distinct from confined developer execution. The actual
RED run observes a failing assertion and successful isolation test. A bounded
Sandbox write corrects one production expression, GREEN passes, and native
git diff/porcelain show exactly that source change. Read-only project builds
use private output. All seven original file contents are rechecked afterward.

The independent dependency is read-only. Removing its Sandbox declaration after
cached success fails to load its manifest; an initially missing declaration also
fails with fresh private output. A dependency source alias into an ungranted
sibling cannot execute its compile-error payload. Compiled tests deny fixture
credentials, runtime/dependency/source writes and connection to a live Host
loopback listener. Each command has fresh private Cargo state. These are manual
Sandbox declaration tests: they are not evidence of HostMountService live-grant
admission or actual Bash Tool support for a disjoint dependency.

The first compile attempt failed on a fixture assertion's String/reference type;
`alan-development-first.log` retains it. The first compiled lifecycle attempt
passed cancellation but failed timeout before observing the developer descendant
(`alan-development-compiled.log`, 85.78 seconds). The diagnostic retry preserves
the actual two-second deadline error and about six-second end-to-end startup
(`alan-development-diagnostic.log`, 99.70 seconds). The corrected fixture allows
ten seconds for the command and schedules the descendant effect after fifteen.
It must observe actual live developer processes before either interruption, so
an early deadline never counts as descendant qualification. Both abort and
timeout wait beyond the scheduled delay and verify one start, no later write,
no surviving project process and removal of the runner directory that existed
while the command was live. The focused lifecycle run passed in 118.87 seconds
(`alan-development-lifecycle.log`); final-source full-suite evidence is separate.

An independent child with an unsupported PATH entry records actual readiness:
namespace capabilities and runner smoke pass, tool visibility fails, and Landlock
is selected with the explicit PATH reason. Requiring the namespace backend refuses
before the fixture marker. This is fallback evidence, not a namespace success.

Final native source passes 1426 engine tests, zero failures and one existing
live-provider ignore in 137.73 seconds (`alan-development-final-engine.log`), with
no relevant native early returns. Linux all-target/all-feature Clippy with warnings
denied passes in 19.56 seconds (`alan-development-final-clippy.log`). The task-owned
`native-development-acceptance.json` retains twelve Rust source hashes, test binary,
logs/failed attempts, platform/PATH/tool identities and bounded original runtime
and settings hashes, which match the prior receipt. The previous provisioning
hardlink metadata correction remains a limitation, not an unchanged-metadata claim.

This closes task 3.5. Tasks 3.1–3.4 remain open for complete provisioning and real
product-entry/live-grant qualification. ADR-0058's single-active-grant native shell
excludes the disjoint dependency despite the combined Sandbox fixture succeeding;
a narrow explicit read-only-grant exception is awaiting the user's design choice.
Do not change that accepted boundary or claim Bash support from the lower-layer run.
The independent thirty real-model development repetitions have not started.

## Complete installation inventory and actual Bash entry

Published lifecycle head `80f8585` passes all sixteen checks on that exact head.
The following independent native evidence extends its qualification without
changing production authority or enabling automatic routing.

The frozen engine test binary is invoked directly, avoiding a Host Cargo/Rustup
build between inventory snapshots. All installed toolchain trees, the original
Cargo proxy directory and Rustup settings are inventoried without following
symlinks: 379 nodes, including content hashes, link targets, mode, UID/GID,
device/inode/link count and modification/change times. Access time is excluded
because reads update it. Before/after inventories are byte-identical; the fixture
passes in 120.973 seconds. Installed shell package identity is
`dash 0.5.12-12ubuntu3`; selected git/Rust identities and unsupported PATH fallback
remain in the earlier receipt. The task-owned records are
`native-development-stores-before.json`, `native-development-stores-after.json`,
`native-development-stores-acceptance.json` and
`alan-development-stores-verification.log`.

This comparison starts after owned fixture provisioning. It does not erase the
earlier hardlink metadata correction or claim unchanged access times. Personal
Cargo registry/credential trees are neither inventoried nor projected. This
closes task 3.1 with the provisioning caveat retained.

An adjacent opt-in Linux Host test reuses the existing service/approval/binding
fixtures. It registers a Process namespace, approves one read-only project grant,
reconciles the live service before each invocation and calls the actual Bash Tool
through ToolContext and the native Host adapter. Backend selection is normal,
not forced; the actual readiness report selects `linux_reified_namespace`.
Version output is rustc/cargo 1.97.0. Two actual offline builds/unit tests use
private output and fresh per-command Cargo state. Compiled tests deny ungranted
home credentials, project/in-project dependency/runtime writes and a connection
to a live Host loopback listener. The Host accepts no connection; all six source
files and runtime/canary bytes remain exact; no project target is created.
Adapter-captured stderr contains no native project backing root. Revoking the
selected grant rejects its pinned future binding; a fresh binding without an
adapter makes the next Bash invocation fail before its marker.

The dependency in this Bash fixture is inside the selected read-only project;
it is not a disjoint grant and does not resolve the outstanding ADR-0058 choice.
This exercises the actual Bash/service adapter boundary, not Agent generation,
policy admission or the full Root workflow. It closes task 3.4, while 3.2/3.3
retain the actual disjoint-dependency/live-grant requirement.

The focused Bash fixture passes in 22.31 seconds (`alan-native-bash-first.log`).
Full Linux Host tests pass 49, zero failures and two existing explicitly ignored
TypeSafe mounted-Connection/Root probes in 22.73 seconds
(`alan-native-bash-final-host.log`); the Bash fixture does not return early.
Actual version/build/build command durations are 5.594/6.053/5.618 seconds.
Host all-target/all-feature Clippy with warnings denied passes in 23.74 seconds
(`alan-native-bash-final-clippy.log`). `native-bash-acceptance.json` freezes nine
Host/Tool source hashes, the unchanged engine source hashes, actual binary/log
identity, grant/entry scope and outcomes. The complete 379-node installation
inventory is still exact after the Bash runs (`native-bash-stores-after.json`).
These remain scripted native tests; thirty repeated real-model runs are open.
