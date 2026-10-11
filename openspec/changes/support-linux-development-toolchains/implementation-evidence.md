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

## Command-local Rust selection repair

Published Bash-isolation head `a947948b` passed all sixteen checks. Subsequent
review reproduced a pre-admission selection gap: `printf effect > marker;
RUSTUP_TOOLCHAIN=missing cargo test` skipped Rust inspection because the parser's
first word was the environment assignment. Native execution wrote the marker
before Cargo reported the missing runtime. A task-owned TMPDIR observer retained
the actual `effect` bytes in `inline-selector-red-effects.json`; portable and
native RED logs are `alan-rust-inline-red-compiled.log`,
`alan-rust-inline-native-red.log` and `alan-rust-inline-effect-red.log`.

The repair reuses the existing normalized command/argument view, handles `env --`
assignment operands there, and validates the effective Rust invocation. Explicit
`+selector` wins over the last literal command-local override, which wins over
inherited/project selection. PATH/Rustup-home replacements and persistent shell
selection changes refuse before effects. Ordinary output containing these words
and unrelated assignments remain content. The internal helper export follows
Rust inspection's Linux/Unix-test compilation conditions; macOS Clippy exposed
the initial unconditional unused import and passes after that repair.

This does not admit native shell wrappers or enable autonomous Linux bash. The
first full native attempt failed because its new positive fixture assumed `env`
was admitted; the existing full shape guard correctly refused it. That failure
is retained in `alan-rust-prefix-final-engine.log` (1427 passed, one failed, one
ignored). Corrected literal-prefix and explicit-CLI native builds pass exact
rustc/cargo/rustdoc 1.97, unit/doctests/fmt/Clippy, with no source/dependency target
output. Missing inline and persistent selection changes refuse before the
preceding marker; actual `env`/environment-reset shapes are refused by the
existing shape guard before the marker. Normalized wrapper inspection remains a
portable test, not a claim of new native wrapper support.

The focused repair suite passes ten tests in 75.13 seconds. Intermediate native
full-source verification passes 1428/0/1 in 145.95 seconds; after the macOS-only
conditional-import fix, final-source native engine passes **1428 tests, zero
failures and one existing ignore in 141.96 seconds**
(`alan-rust-prefix-final-engine-v3.log`). The actual live-service Bash entry
is rebuilt with the new engine; full Host passes **49/0/2 in 25.02 seconds**
(`alan-rust-prefix-final-host.log`), preserving its recorded isolation/revocation
assertions. Linux engine and Host all-target/all-feature Clippy with warnings
denied pass in 20.42 seconds (`alan-rust-prefix-final-clippy.log`). macOS passes
43 portable namespace/Rustup tests with zero failures in 0.06 seconds, and its
engine all-target/all-feature Clippy passes in 10.15 seconds. Native Linux
claims do not derive from macOS tests or existing ignored live-provider probes.

`native-rust-prefix-acceptance.json` freezes 23 source hashes, both actual native
binaries, final logs, prior attempts and scope limits. Complete original
installation inventory remains 379 nodes and byte-identical to the retained
baseline after all final native execution (`native-rust-prefix-stores-after.json`,
SHA-256 `118fea4c93ee341a24f3757175fc76aa7e0562ae7335ba257a6127c647e72b46`). Earlier
provisioning metadata caveats remain in force. Native engine binary SHA-256:
`81f5254758fc7c62457d57e3529f819f83b24f95352221dd8cba146ccb89c0b5`;
native Host binary SHA-256:
`faccba3a99066c065cc24ea3f7c38979f8ec5e6eeee32b0c6f5f6da0ba34f811`.

Normal pre-commit quality/distribution enforcement and fresh exact-head CI remain
the delivery workflow. No Host grant policy changed: 3.2/3.3 still require actual
cross-grant product qualification, and the thirty real-model repetitions remain
open. This repair supplements the existing supported-tool inspection tasks; it
does not mark the remaining final-delivery checklist complete.

## Shell-local directory selection boundary

Published `81af8fdd` passed all sixteen exact-head checks. Further review
reproduced a second admission gap: `printf effect > marker; cd child; cargo test`
inspected the initial project selection, then the actual shell entered a child
whose toolchain file requested a missing runtime. The old native result was
exit 1 after a marker write, not a pre-admission refusal. A separate owned-TMPDIR
observer records the actual marker bytes and failing test-binary digest in
`cwd-selector-red-effects.json`; `alan-rust-cwd-red-build.log`,
`alan-rust-cwd-red.log` and `alan-rust-cwd-effect-red.log` retain the RED attempt.

The repair reuses the normalized command reader and existing selection validator.
Literal `cd` targets inside delegated Host mount authority add both the old and
canonical target directory to a bounded possible-state set, retaining conditional
or failed-cd outcomes rather than guessing shell control flow. Every potential
implicit runtime/component/helper selection is checked and its selection metadata
is retained for runner revalidation. This supports normal literal child-project
builds without changing the Agent Process directory binding or Host grant policy.
Unknown, missing, escaping or more than 64 possible directory states require a
fixed CLI/inline/inherited selector or refuse before user effects with guidance to
use the existing standalone Process directory change. Existing wrapper restrictions
and no-autonomous-Linux-bash policy remain unchanged.

Four positive native child-project cases exercise implicit project selection and
fixed CLI, command-local and inherited selection after `cd child`. All execute
exact rustc/cargo/rustdoc 1.97, compile with a separate lower-layer read-only
dependency, pass unit/doctests/fmt/Clippy, retain exact root/child/dependency sources
and create no project/child/dependency target. The missing child selection refuses
before the preceding marker. Portable regressions additionally verify changed
child selection metadata, unknown cwd, escaping aliases and the state bound.

The initial blanket-cd refusal candidate passed its narrower suites but did not
preserve ordinary implicit subdirectory builds. Its receipts/logs remain separate;
it is superseded by this supported-literal implementation. A normal macOS commit
attempt also caught non-idempotent formatting in a nested expression; it failed
without creating a commit or bypassing the hook. Factoring the selection expression
made repeated fmt/check stable before freezing the final source.

The supported-literal focused native/portable Rustup candidate passes 12 tests in
128.13 seconds (`alan-rust-cwd-supported-focused.log`). Final frozen-source Linux
engine passes **1430/0/1 in 144.59 seconds**
(`alan-rust-cwd-supported-final-engine.log`), including observed-live descendant
cancel/timeout and unsupported-PATH refusal assertions. Rebuilt actual live-service
Bash plus full Host passes **49/0/2 in 23.98 seconds**
(`alan-rust-cwd-supported-final-host.log`). Linux engine/Host all-target/all-feature
Clippy with warnings denied passes in 22.18 seconds
(`alan-rust-cwd-supported-final-clippy.log`). The same source's macOS portable
namespace/Rustup suite passes **46/0/0 in 0.10 seconds**; Linux-specific claims
remain separate from portable tests and existing ignored live Connection probes.

`native-rust-cwd-supported-acceptance.json` retains 23 frozen Rust source hashes,
actual native engine/Host binary digests, final logs, the failing observer and scope
limits. Complete original installation/settings/proxy inventory remains **379
nodes, identical to the retained post-provisioning baseline**
(`native-rust-cwd-supported-stores-after.json`, SHA-256
`118fea4c93ee341a24f3757175fc76aa7e0562ae7335ba257a6127c647e72b46`). Original provisioning caveats
and personal Cargo registry/credential exclusions remain. Native engine SHA-256:
`b38740d162e346761d5696282501dd3b05086a9c7bb9dba0d6f49b2adac2f75f`;
native Host SHA-256:
`de271f90c425ff8b8f1a22a1d21155d20383e285db4c1f53a96ad2dfcf77be3f`.

Normal full pre-commit quality/distribution, pinned strict OpenSpec and fresh
exact-head CI are required to publish this follow-up. This repair does not close
cross-grant Bash delivery, user merge/canonical closure or thirty real-model runs.

## Actual Bash Git and RED/GREEN qualification

Published `8f11102b` passed all sixteen checks. The Git fixture previously used
manual read-only Sandbox declarations for its task-private install root; actual
Host shell projection does not infer that runtime mount. APT simulation would
upgrade installed curl/libcurl while adding Git dependencies, so no package
installation was performed. Instead the verified Git 2.53 binary and its three
existing runtime/template directories were copied to the unique task-owned prefix
`/usr/local/lib/alan-qualification/linux-toolchain-20261010-af1df75053/git-2.53.0`.
It is inside the already read-only `/usr` substrate. The prefix was absent before
provisioning, records its owner/source and 217 runtime file/link entries, and uses
identical Git executable bytes. No global PATH, package registration, personal
Rust default or Host project was changed by this provisioning. Ancestor directory
metadata changes from creating the new prefix are not an unchanged-filesystem claim.
The before/after installed package inventory is exact.

The original private Git PATH is still outside the native execution substrate.
An independent invocation of the actual Host test binary captures readiness
selecting Landlock with its explicit PATH reason; its native precondition fails
before this fixture creates a project or invokes Bash. The expected exit 101 and
binary/log identity are in `native-bash-git-private-root-unavailable.json` and
`alan-native-bash-git-private-root-unavailable.log`. This is an unsupported native
startup slot, not a Tool success, a new runtime defect, or qualification of the
weaker backend. The standard-prefix positive explicitly chooses its original PATH
and Git identity; product execution does not rewrite an unsupported PATH.

The new adjacent native Host fixture registers a Process namespace, approves a
selected writable project and reconciles HostMountService before each Tool call.
Normal backend readiness selects `linux_reified_namespace`; no force-backend path
is used. Actual Bash observes Git 2.53 and Rust 1.97, checks a clean baseline, runs
the failing test, then EditFileTool replaces exactly one expression through the
same live binding. Bash reruns GREEN and obtains exact diff/status: only
`src/lib.rs` changed. Seven expected source/config files and `.git/config` are
checked; Cargo output is authorized within the writable project and ignored by
Git. Selected-grant revocation rejects the pinned binding and prevents the next
Bash marker. Its dependency is inside that selected writable grant; it does not
replace the independent read-only dependency/live-grant requirement.

The first focused native fixture passes in 36.26 seconds
(`alan-native-bash-git-first.log`). Final frozen source passes Linux engine
**1430/0/1 in 135.62 seconds**
(`alan-native-bash-git-final-engine.log`) and full Linux Host
**50/0/2 in 36.55 seconds**
(`alan-native-bash-git-final-host.log`), including actual Bash read-only isolation
and descendant cancellation/timeout assertions. Linux engine/Host all-target,
all-feature Clippy with warnings denied passes in 1.60 seconds
(`alan-native-bash-git-final-clippy.log`). macOS Host passes **49/0/2 in 4.32
seconds** using its native Apple target; these portable regressions do not qualify
Linux Tool execution. An initial macOS command used the Linux-owned artifact path
and was refused before build; it was rerun from the dedicated macOS verification
worktree without borrowing native output.

`native-bash-git-acceptance.json` freezes 23 source hashes, both actual native
binary digests, logs, prefix runtime inventory and unchanged installation/package
records. The original **379 Rust installation/settings/proxy nodes** remain exact
against the retained post-provisioning baseline, and all 217 copied Git entries
remain exact. Read access times and personal Cargo registry/credentials retain the
previous exclusions; the earlier proxy hardlink provisioning correction remains
part of history. Native engine SHA-256:
`b38740d162e346761d5696282501dd3b05086a9c7bb9dba0d6f49b2adac2f75f`;
native Host SHA-256:
`9c5fc29b2c5c6cacdca96373cb691b44eedf76b2d9dbf2f508194d19312fe304`.

Only the acceptance fixture and evidence changed; production authority,
configuration, dependencies, runtime projection and automatic routing are unchanged.
Native Git version/status/diff are qualified; Host baseline init/commit are setup,
and Git network/Perl helpers are untested. Final review/full quality/pinned strict
validation/current-head CI must accompany publication. Tasks 3.2/3.3 retain their
actual disjoint read-only dependency requirement, and thirty real-model development
repetitions remain open. Do not equate scripted Tool results with code authorship.


## Actual live dependency negatives and diagnostic projection repair

The next actual HostMountService/Bash fixture extends the selected writable Git
project with missing/revoked external dependency and ungranted source-alias cases.
The earlier GREEN output is retained inside the project: these are cached
**in-grant** builds, not a previously successful external dependency build.
Without an external grant, ReadFile denies access and Cargo exits 101. A separate
explicit read-only grant then permits actual ReadFile and denies EditFile; after
revocation/reconciliation, both ReadFile and the next Cargo access fail. External
manifest/source bytes remain exact. No active disjoint read-only grant is used by
Bash, and no shell grant policy is changed.

After restoring the original manifest, the Host fixture replaces the in-grant
library source with a symlink into an ungranted sibling containing a compile_error
canary. The dependency has an explicit [lib] path so this case reaches rustc,
rather than stopping at Cargo target discovery. ReadFile rejects the alias and
actual Bash/rustc reports missing source without consuming the canary; the sibling
payload remains exact. The fixture restores the source and rechecks all seven
source/config files with only the original authorized expression correction.

The first test-only attempt against production revision `4212e7cb` exposed a real
shared projection defect: Cargo correctly refused the ungranted manifest but its
stderr named the normalized private sibling `/tmp/<fixture>/outside-dependency`.
The retained `alan-native-bash-dependency-first.log` includes this response and an
initial fixture assertion expecting "failed to load manifest" instead of Cargo's
actual "failed to load source for dependency". That assertion was corrected; no
old native binary digest was frozen before rebuilding, so the raw log and stated
base/test-only scope are the RED evidence, not an invented exact RED binary proof.

The Host adapter now projects the original output in one pass. Known mounted
roots take precedence and require a complete component boundary; private ancestors
of unavailable sibling paths use the existing `<unmapped-host-path>` marker while
retaining the diagnostic suffix. This also prevents prefix-colliding siblings
from acquiring a fabricated namespace and avoids cascading a replacement back
through another native prefix. Filesystem-root projection preserves relative text
and URI schemes. Five local regressions cover these cases, Unicode/color/file URI
text and literal role/comparison/quote content. No grant, public API, configuration
or dependency was added. Bash stdout/stderr, ToolContext JSON projection and Tool
registry errors continue to reach the same NativeToolExecutionAdapter method;
standalone/test adapters are outside this native Host repair.

The initial four projection tests and focused native fixture passed (52.13 seconds
for the fixture); after explicit library target selection, the focused fixture
passed in **51.91 seconds** (`alan-native-bash-dependency-explicit-lib.log`). The
final five projection tests pass with zero failures
(`alan-native-bash-dependency-projection-final.log`). Final frozen Linux source
passes engine **1430/0/1 in 129.85 seconds** and Host **55/0/2 in 52.71 seconds**
(`alan-native-bash-dependency-final-engine.log` and
`alan-native-bash-dependency-final-host.log`). Both actual Host development cases
execute without their opt-in early-return diagnostics. Linux engine/Host
all-target/all-feature Clippy with warnings denied passes in **1.60 seconds**.
Native macOS Host passes **54/0/2 in 4.47 seconds** from its own Apple-target artifact
owner; portable macOS coverage does not qualify Linux execution.

`native-bash-dependency-acceptance.json` freezes **24 Rust source hashes**, actual
Linux engine/Host binaries and logs. The original **379 installation/settings/proxy
nodes** and **217 copied Git runtime entries** remain exact; installed package
inventory is unchanged. The retained installation baseline SHA-256 is
`118fea4c93ee341a24f3757175fc76aa7e0562ae7335ba257a6127c647e72b46`.
Linux engine SHA-256:
`b38740d162e346761d5696282501dd3b05086a9c7bb9dba0d6f49b2adac2f75f`;
Linux Host SHA-256:
`fcf2199cb3aa502bf1d53bcf4e2479181977ef4a599cd945751384228b2b6ed4`.
The earlier provisioning hardlink correction and access-time/personal Cargo store
exclusions still apply. The receipt collector's expected Host count was corrected
from 54 to the actual 55 after the fifth local regression was added; this changes
only the ignored evidence collector and does not rerun or relabel product tests.

These scripted native Tool negatives close the actual missing/revoked/alias slots,
not complete external read-only dependency support or real-model development.
Tasks 3.2/3.3 remain open for the disjoint read-only positive and its following
revocation/cache acceptance. ADR-0058 retains the selected-single-grant shell
boundary pending the user's design choice. Thirty repeated real-model development
runs and final review/full quality/strict validation/current-head CI remain open.

Pinned OpenSpec 1.4.1 strict validation passes **69/69 items** with both existing
capability owners in the proposal. Final source review followed every production
projection caller above, constructor canonical-path invariants and the native
fixture's live reconcile/revoke sequence. Known-mount priority and one-pass
replacement preserve overlapping namespace names; the negative cache assertions
remain explicitly scoped to the earlier in-grant build. No additional grant or
standalone adapter change is claimed. Full quality and current publication-head CI
are recorded separately at publication; they cannot be inferred from the earlier
`4212e7cb` checks.


## Explicit read-only dependency candidate and discovered authority boundaries

The delivered ADR-0058 single-grant boundary cannot complete the independent
read-only dependency positive. This draft implementation candidate projects the
selected cwd grant first plus live same-Process read-only grants from Host Mount
Service. Only the selected grant contributes writable authority; other writable
and foreign-Process grants remain unavailable to shell execution. Structured file
Tool scope is unchanged. This extension is pending user adoption/merge and is not
reported as shipped policy. No manifest, prior output, common parent or personal
store adds a grant.

Retained attempts are separate and remain unsuccessful where stated:

- `alan-native-bash-readonly-grants-red.log`: test-only fixture over production
  `ce7aa5dd`, **42.47 seconds**, fails the shell readable-root assertion after
  the separate grant permits structured ReadFile. This is projection RED, before
  the external native Bash build; it does not prove an old external Cargo attempt.
  `native-bash-readonly-grants-red-identity.json` freezes its native binary,
  test-source and log identity.
- `alan-native-bash-readonly-grants-first.log`: initial candidate, **74.31
  seconds**, runs the actual external RED/correction/GREEN, compiled OS authority
  checks, live-RO source escape, restored GREEN and cached-success revocation.
  It then fails the old fixture's final expectation: a fresh revoked project cwd
  silently selects the newly added other writable project. The run remains failed.
- `alan-native-bash-missing-cwd-red.log`: actual Service reproduction, **0.01
  seconds**, prints `missing cwd unexpectedly selected /mnt/other; other writable:
  true`. Reconciliation must refuse unknown non-root cwd, keeping explicit valid
  selection and the special namespace-root setup intact.
- `alan-native-bash-readonly-grants-fixed.log`: a mechanical fixture adjustment
  matched two identical blocks and was not applied; the next run failed an old
  `unwrap()` because the corrected constructor now properly refuses missing cwd.
  It is retained separately rather than reported as another production defect.
- `alan-native-bash-readonly-grants-complete.log`: the corrected focused fixture
  completes **1/0/0 in 81.85 seconds**. The initial full candidate subsequently
  passes Linux engine **1430/0/1 in 133.36 seconds**, Host **57/0/2 in 79.81
  seconds**, warnings-denied Clippy, and macOS Host **56/0/2 in 4.11 seconds**.
  `native-bash-readonly-grants-acceptance.json` retains that pre-root-check source,
  binary and log evidence; it does not qualify the following newly added check.
- `alan-native-bash-retargeted-root-red.log`: a new review reproduction,
  **0.01 seconds**, renames an approved canonical read-only root and replaces it
  with an outside symlink. Reconciliation accepts it and reports `ungranted
  outside readable: true`; SandboxSpec's canonicalization moved authority.
  This is an uncommitted candidate/test reproduction with retained raw log;
  no exact RED binary identity was frozen before its corrected rebuild.

The common native Host constructor now validates every approved canonical root
before deriving either sandbox. Retargeted, missing or non-directory roots fail
with only the public namespace in the diagnostic. Three portable Service tests
cover same-Process read-only scope with selected-first ordering, invalid cwd, and
both RO/RW root replacement/missing/non-directory cases plus original restoration.
This is a reconciliation-time validation, not an atomic concurrent Host path
replacement or inode-pinning claim.

The actual Linux external fixture observes a semantic test failure, uses real
EditFileTool to correct the authorized expression, compiles against the explicit
read-only dependency and checks the exact source git diff. A compiled integration
test reads that dependency and verifies OS-level write denial, other same-Process
writable-project read/write denial and foreign-Process read denial. Dependency
source aliases into an ungranted sibling fail without consuming the canary. After
restored external GREEN, revocation is tested with that successful external output
still present: structured read denies and actual Cargo exits 101. Host setup of
those immutable test assertions is distinct from Tool-authored source edits.
The final exact source inventory restores the original seven files apart from the
one authorized expression correction; dependency/canary bytes remain exact.

macOS Host suites exercise the portable shared adapter. These are not actual
compiled arbitrary-reader Seatbelt confinement qualification; Linux is the native
external-dependency proof. None of these scripted Tool tests fills a real-model
qualification slot. Thirty generation-authored tasks remain NOT_RUN in the
independent `qualify-repeated-development-workflows` proposal PR #1047.


### Final root-checked native candidate

The final frozen candidate passes Linux engine **1430/0/1
in 132.84 seconds** and Host **58/0/2 in
80.06 seconds**. Both opt-in Host development cases actually
execute; no unavailable-fixture early return is counted as native acceptance.
All-target/all-feature Linux engine/Host Clippy with warnings denied passes.
macOS Host passes **57/0/2 in 3.87 seconds**, using its own Apple-target build
artifact owner. Pinned OpenSpec 1.4.1 strict validation passes **69/69 items**.

`native-bash-readonly-grants-root-checked-acceptance.json` freezes **25 source
hashes**, test binaries, new logs and retained first attempts including the
root-retarget RED; previous `native-bash-readonly-grants-acceptance.json` remains
unchanged as pre-root-check evidence. Final Linux engine binary SHA-256:
`b38740d162e346761d5696282501dd3b05086a9c7bb9dba0d6f49b2adac2f75f`;
final Linux Host binary SHA-256:
`8c6b9d32490e37e9b13ff056ea59fd6a69261bc7647943125dbaff9b7449233a`.
The **379 original Rust installation/settings/proxy nodes** still match the
recorded post-provisioning baseline, SHA-256
`118fea4c93ee341a24f3757175fc76aa7e0562ae7335ba257a6127c647e72b46`; all **217** task-owned standard-prefix Git
runtime entries and the installed package inventory remain exact. Personal Cargo
registry/credential contents and read access times retain their prior exclusions;
the original provisioning hardlink correction remains part of history.

Review traces Host Mount Service's active same-PID projection selection and
per-launch Tool registry reconciliation into the common native constructor.
Selected-first ordering preserves cwd/runtime inspection, writable authority comes
only from the selected grant, and existing mixed-access overlap checks remain.
The missing-cwd and root-validation fixes do not add a registry, config or public
API. Native proof covers the Linux compiled readers and cached external build;
macOS foreign-reader OS confinement and concurrent Host path replacement are not
claimed. Tasks 3.2/3.3 now have their required live-service native positive and
negative evidence; final publication quality/current-head CI/user merge/canonical
closure remain separate. No real-model qualification slot is completed.


### Absolute manifests and retained-directory identity follow-up

The first absolute-manifest probe passes Host 58/0/2 in 86.06 seconds, but Cargo
reuses the relative build output. Its complete-scope frozen source and raw logs
remain retained; no separate native Host binary digest was frozen before the next
rebuild. The strengthened fixture uses a fresh target/absolute-dependency directory
and requires actual `Compiling alan_git_fixture_dep` plus the compiled authority
test. This fresh-build candidate passes Host **58/0/2 in 96.37 seconds** and
warnings-denied Clippy. `native-bash-readonly-grants-absolute-fresh-acceptance.json`
retains its 25 source/binary/log hashes and exact original inventories.
Its engine result reuses the unchanged source and identical engine ELF from the
132.84-second root-checked suite; it does not claim another engine execution.

Further shared-boundary review follows HostDirFs's existing retained directory
handle. `alan-native-bash-replaced-directory-red.log` shows a same-path replacement
directory still passes canonical validation even though the exported file tree
remains attached to the original directory (**0/1/0 in 0.01 seconds**). No exact
RED binary digest was frozen before rebuilding. The fix reuses the retained handle's
device/inode metadata to ensure the native backing still names that same directory;
HostDirFs exposes only a boolean check, with no new registry or identity manager.
The constructor also checks canonical spelling agrees with the retained tree root.
The portable grant test covers replacement for both RO/RW grants and original-root
restoration. Missing/non-directory/symlink cases and public-only errors remain.

Root identity is checked at reconciliation; concurrent changes after that check
are not a descriptor-bound native-launch guarantee. The existing aP file server
continues using its already retained directory handle. New native HostFs/Host and
macOS HostFs/Host checks below qualify this identity-checked production candidate;
older receipts above remain scoped to their actual sources.


### Final retained-root candidate verification

Final Linux Host is **58/0/2 in 86.64 seconds**,
HostFs **23/0/0 in 0.01 seconds**, and warnings-denied
all-target/all-feature Clippy covers engine, Host and HostFs. Actual Bash executes
both relative and absolute native dependency manifest paths, with a fresh absolute
build output directory and observed dependency compilation. macOS passes HostFs
**23/0/0 in 0.06 seconds** and Host **57/0/2 in 4.09 seconds**. The three portable
Service tests pass, including same-path directory replacement and restoration.

`native-bash-readonly-grants-identity-checked-acceptance.json` freezes **26 source
hashes**, all three native test binaries, final and retained-attempt logs. Engine
**1430/0/1 in 132.84 seconds** is reused only after checking all unchanged engine/
Tool hashes and its exact ELF; HostFs is absent from the engine's dependency tree.
It is explicitly retained execution evidence, not a new engine run. Native Host
ELF SHA-256: `9552c02079c9824256c594fb06325278aa792215158454498cc5622081ded7bb`.
Original Rust's 379-node inventory still has SHA-256
`118fea4c93ee341a24f3757175fc76aa7e0562ae7335ba257a6127c647e72b46`; all 217 task-owned Git runtime entries and
installed packages match their recorded baselines. Previous provisioning and
personal-store/access-time exclusions remain unchanged.

The common constructor compares canonical path, retained file-tree root and
existing directory handle identity. The only additional HostFs public API is its
boolean backing-root check; AP, Kernel and Agent Process types remain unchanged.
No registry, configuration or dependency is added. This repairs observed path/
identity divergence before native Tool projection, while concurrent post-check
Host mutation remains outside the claimed guarantee.

### Publication and final boundary review refresh — 2026-10-11

Published source head cace913c passed all sixteen completed checks, including
quality, both platform test/release jobs, harnesses and CodeQL. Its inline review
inventory was empty at that publication check. The final review follows the Host Mount Service's
active same-PID projection filter through per-launch reconciliation, the common
native adapter's selected-first/read-only-only shell projection and the existing
HostDirFs directory-handle identity check. Invalid non-root cwd, revoked sticky
cwd grants, mixed-access overlap, retargeted/replaced roots and authority
amplification remain refusals; the retained compiled-reader and cached-revocation
receipts cover those boundaries. No additional production repair is found.

#1047 now incorporates cace913c and its observed production repairs. Its published
f84c21d4 source matches the frozen native candidate's 827 crate/workspace files,
with only the separately qualified observer added, and passes all sixteen checks.
That change records fifteen final-candidate macOS slots through thirty-four
development attempts; fifteen Linux slots remain NOT_RUN. Neither its real-model
receipts nor its later operator tooling setup replaces this change's frozen
installation/package/native authority receipts.

This documentation refresh prepares #1046 for formal review and requires its own
normal quality/distribution and current-head CI. It changes no source or authority,
adopts no ADR-0058 extension and merges/deploys nothing. Checklist remains 13/15
until formal review and user-controlled canonical delivery close their gates.


### Current-head review: selected Rust runtimes — 2026-10-11

Review 4239725745 is valid. Per-command discovery previously retained every
installed standard runtime in the execution substrate and private Rustup links.
A protected test compiled and executed through the actual native Sandbox adapter
selected Rust 1.97.0, but could still observe the unrelated installed stable runtime;
`red-a2-selected_only.log` retains the original assertion failure. Discovery now
collects the existing bounded command/directory selections before reducing the
runtime list and recording its manifest/executable hashes. Multiple possible
selections remain present; a non-Rust command adds no installed runtime. Startup
catalog validation is inspection, not a Process execution grant. No PATH/default,
Host grant policy, native wrapper admission or dependencies change.

Review 4239725742's direct `sh -c` example is not admitted by this Linux path.
`Sandbox::exec_with_timeout_and_capability` checks the command shape before
`exec_reified_namespace`; `permits_autonomous_bash` is true only for Seatbelt,
so Linux uses Full mode regardless of capability or enforcing-backend availability.
Full mode rejects nested shell evaluators before an outer marker can execute.
The native baseline returns that guard error, not a post-effect missing-runtime
error. New native direct/nested/inline-selector refusal controls assert both outer
and inner markers are absent. The unused experimental recursive-discovery code
was removed; no new inline shell support is introduced by this review follow-up.

Portable regression covers zero, one and multiple selected runtime mounts,
explicit `rustup run`, omission of unselected runtime validation evidence and
unchanged selection precedence. The native visibility probe positively reads
selected Cargo, cannot observe the unrelated stable runtime, must actually run
its test, and retains exact protected source contents. Raw collector mistakes
(zero tests from an incorrect exact name), the initial native P1 expectation
mismatch, and generated fixture fmt/test-count failures remain retained; none is
a real-model task or a passed native suite. Corrected final execution and artifact
identities are recorded below after verification.

The owned review cache is
`~/Library/Caches/Alan/qualification/rust-review-20261011/`; the Linux public-only
source/receipts are `/home/morris/.cache/Alan/rust-review-20261011/`. No Connection
credentials, Host private stores or frozen repeated-task receipts were copied or
changed. All fifteen Linux model slots remain NOT_RUN. Prior macOS candidate
receipts stay bound to their original source/binary; final source applicability
and user-controlled canonical delivery remain separate gates.

Final native Linux engine passes **1431/0/1 in 150.35 seconds**, including all
23 automatic-Rustup cases. The actual visibility integration assertion and three
shell refusal/absent-marker controls pass. Native engine ELF SHA-256:
`0cd3a573c305e719ba42b0b1c2c218fbea202e62888103cbf38e98443258dbb0`.
The three changed Rust file hashes match the exact native input. macOS engine
passes **1417/0/1** plus **20 integration tests**. Both Hosts' engine all-target/
all-feature Clippy pass with warnings denied; pinned OpenSpec 1.4.1 strict
validation passes **69/69**. Final and failed attempts are retained under
`rust-review-20261011/final-v1/`. Normal commit quality/distribution and fresh
current-head CI are required before treating the repair as reviewed delivery.
No fifteen-slot Linux generation or final main-source qualification is implied.


### Coverage admission: bounded Package Store contention — 2026-10-11

The first repair-head CI coverage job `114355535866` (run `38100588737`, head
`baee72b8`) fails the existing two-client Package catalog test: an installation
returns `Package Store busy: lock acquisition exceeded 500 ms`. Coverage report
upload is skipped after that test failure; this is not an upload/network error.
The store correctly refuses contention at its existing operator bound. The test
incorrectly requires both concurrent installations to succeed within that bound.

The fixture now accepts only the exact bounded-busy reply, verifies no catalog
entry or materialized revision for the refused package, and explicitly retries
after the contenders finish with a fresh request ID. Admission consumes failed
request IDs too, so reusing the original ID remains an error. Catalog equality and
both entries remain required. A second case reuses the existing owned lock-holder
helper: both requests finish busy while the peer still owns the lock, then the
holder exits normally before either retry. The five-second observation ceiling
allows thread scheduling; the production acquisition limit stays 500 ms. No
production Package behavior, CI exclusion, skip or timeout override is introduced.
The other Package tests have no matching unconditional two-thread success
assumption; existing explicit lock-budget/short-contention tests remain intact.

Both Hosts' complete Service Manager suite passes **157 unit + 2 integration**
with zero failures; both all-target/all-feature Clippy checks pass with warnings
denied. Local actual LLVM coverage instrumentation passes the same concurrency
test, including the held-peer/busy/no-installation/fresh-request retry branch
(**0.85 seconds**, observed peer-owned busy completion at **502.95 ms**).
The original CI failure and local duplicate-ID/command-filter correction logs are
retained. A fresh normal commit gate and exact-head complete CI still follow.
The native Rustup production hashes/ELF and its 1431/0/1 receipt remain unchanged;
this follow-up changes only the Package test and evidence. No model slots are added.
