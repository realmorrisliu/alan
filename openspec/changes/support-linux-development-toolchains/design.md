## Context

Main baseline: `faf747c2`, including terminal UI #1044 and selected-directory
context #1043. The separate post-merge documentation closure will archive both.
Existing native Host adapters supply `SandboxSpec`; `reified_namespace/plan.rs`
keeps authority, read-only substrate and private tmp distinct. Backend readiness
requires actual runner smoke plus a user-PATH visibility check. Setup helpers
are resolved from trusted absolute paths; the user command gets a fixed PATH
and a cleared environment. Existing Process execution owns cancellation.

Fresh read-only inventory on 2026-10-10: OrbStack ubuntu, aarch64, UID 501,
kernel 7.0.14-orbstack-00380-ga7e0a2dc9535. Shell/C compiler and Rust 1.97 are
installed; git is absent. Cargo/rustc resolve through `/home/morris/.cargo/bin`
to rustup. PATH includes OrbStack and user executable directories outside the
fixed substrate. The prior namespace-creation probe is not full runner evidence.
Receipt: output-presentation worktree `target/linux-environment-20261010.json`.

See proposal.md for motivation and the capability spec owners for requirements.

## Goals / Non-Goals

Goals: real shell/git/Rust development within the existing enforcing backend,
with exact tool selection and explicit project/dependency authority.
Non-goals: new sandbox backend, arbitrary language installers, whole-home mounts,
credentials import, changed network/approval rules, global execution state,
automatic routing or general autonomous code-authorship qualification.

## Decisions

### Extend the existing plan and native Host adapter

Native selection and projection remain Host-private inputs behind the current
Tool adapter. Use the existing plan's substrate, declared mounts and scratch
rather than adding a toolchain manager or a second authority registry. Preserve
current sensitive-path checks, mount overlap/escape validation and live grant
reconciliation. Machine, Kernel, AgentFS and rollout do not receive raw backing
roots. Existing Process launch/cancellation remains lifecycle truth.

Implement in small verified slices: first repair inherited-submount projection
and verify recursive read-only enforcement; then carry supported PATH unchanged into
user execution while keeping trusted setup PATH separate; then project selected
Rust runtime and isolated writable homes. Keep the final contract open until
user-installed Rust, git and a real dependency build all qualify. A system-PATH
slice is intermediate progress, not completion of toolchain support.

### Freeze selection before constructing the view

Preserve ordered absolute PATH entries for supported executable inputs; reject
unset, empty/current-directory, relative, non-representable or escaping entries.
Do not omit a conflicting provider and continue with a later command. Resolve
installed selected Rust identity/version using Host adapter inspection, not model
text or global default rewrites. Validate all required projected runtime content
and recheck before user execution. Use arguments/environment values rather than
interpolating user paths into trusted setup shell text.

The existing trusted `unshare`, shell, mount, chroot and setpriv selection remains
independent of user PATH. Missing runtime content fails before the setup marker
admits user execution. Backend selection and per-command planning must agree;
a startup probe cannot authorize a changed environment later.

The first PATH slice retains exact entry order, duplicates and alias spelling.
Unknown entries fail even when absent or currently empty: a delegated mount or
private scratch could make the same namespace path visible. Absent directories
inside a known substrate remain harmless search entries. Dangling aliases fail.
Relative directory aliases are supported; absolute aliases within a remapped
substrate are unavailable because Host and namespace resolution can differ.
Identical Host/namespace roots retain absolute aliases confined to that root;
parent-traversing and externally chained targets stay unavailable. Bounded alias
traversal checks chains and trailing separators without changing the returned
PATH. The runner also refuses changed substrate roots and non-representable
execution paths before command effects. This is not installed Rust qualification.

### Keep runtime read-only and caches private

Only supported executable/runtime inputs enter the read-only substrate. Never
mount a home, complete Cargo home or Rust management tree simply to make a command
work. Cargo home can contain registry credentials and global config; use private
`CARGO_HOME` and private output where project access is read-only. Rustup settings,
proxies and selected runtime files need separate validated projection; the selected
version must remain exact and absent toolchains must not auto-install. Unknown
wrappers or runtime dependencies remain unavailable until explicitly supported.

Reuse private runner scratch for per-command home/cache/output. Keep scratch
owned through the current cancellable process lifetime and descendants. Cross-turn
cache reuse is not required for this delivery; if later measured tasks need it,
use an existing service-owned cache rather than ambient home writes. Network-free
local dependency qualification comes first; network retrieval keeps current policy.

The private environment is a non-overlapping child of the existing tmpfs scratch,
mode 0700, with separate home/Cargo/rustup/tmp/XDG cache/build directories. Explicit
mounts below scratch retain their original contents and access. Read-only cwd
execution exports private `CARGO_TARGET_DIR`; writable project execution retains
Cargo's project output/configuration behavior. Auto-install is disabled. Each
command gets fresh scratch; no private cache is carried to a later command.

Rustup needs `/proc/self/exe`. Mount a fresh read-only procfs for the command's
private PID namespace, never the Host proc tree. Replace the trusted outer setup
process with chroot via `exec` before admitting user effects, so procfs has no
outside-root PID 1 helper. Conflicting native `/proc` projections fail explicitly;
Alan's virtual Process files are still not native Host mounts. Existing Process
cancellation owns descendants and tmpfs lifetime. System `cc` requires the
read-only `/etc/alternatives` links and `/usr/libexec` GCC runtime directory.
These prerequisites do not complete automatic installed-Rust projection.

### Inspect standard installed Rustup inputs at the Host boundary

The Host adapter recognizes standard ELF Rustup proxy directories in the original
PATH. It preserves their namespace spelling/order, validates the bounded known
proxy names and their shared executable identity. Per-command inspection adds only
the standard runtime roots actually selected by its bounded possible invocations
to the read-only substrate; commands without a Rust runtime selection mount none.
Startup catalog inspection does not grant every installed runtime to execution. The catalog is bounded to
64 roots; metadata reads are bounded to 64 KiB and each executable digest to
256 MiB. Unsupported extra proxy entries, wrappers, linked/custom runtime roots
and escaping aliases remain explicitly unavailable. No complete Cargo home or
Rustup management directory is mounted.

Private Rustup scratch receives only whitelisted settings and directory overrides
relevant to delegated projects, plus links to the independently projected runtimes.
Preserve the original explicit `RUSTUP_TOOLCHAIN`; ordinary command `+selector`
takes priority over that override. Otherwise inspect the nearest directory override
or project toolchain file before the original default. At the same directory the
override takes priority, and the legacy file wins over the TOML file. A project
toolchain file outside delegated authority does not infer another grant. Existing
Linux shell-shape, protected-path, approval and network rules remain unchanged;
this slice does not qualify arbitrary shell evaluators or custom runtimes.

Inspect literal Rust invocations using the existing command parser and normalized
command/wrapper view. Honor the last literal command-local Rustup selector beneath
an explicit `+selector`, ahead of inherited/project selection. Refuse changes to
PATH/Rustup home, persistent shell selection mutations and environment-reset
wrappers before user effects instead of executing against an uninspected view.
Ordinary content and unrelated wrapper options are not environment mutations.
This does not relax the existing Linux full command-shape guard: native shell
wrappers remain rejected before effects. Normalized wrapper inspection is tested
independently and does not imply newly admitted wrapper execution.

A compound shell `cd` changes a child-local directory, not the Agent Process
binding. Reuse the normalized command reader and inspect both the existing and
canonical literal target directories, retaining both possible success/failure
paths. Limit the set to 64 directories and require each target to remain inside
explicit Host mount authority before reading project selection. Inspect every
possible implicit runtime/component/helper selection and retain each selection
file hash for runner revalidation. This preserves ordinary literal subdirectory
builds without pretending a conditional `cd` necessarily succeeded.

Directory options, missing/escaping targets, home expansion or excessive state
make implicit selection unavailable before effects; fixed CLI/inline/inherited
selection remains independent of that unknown cwd. The existing standalone
Process directory change remains available. No shell control-flow interpreter,
extra mount authority or autonomous Linux bash is introduced.

Missing selectors/components/targets fail before user effects. Selected fmt/Clippy helpers
must be executable ELF files contained in their runtime. Freeze selection metadata,
component manifests and executable contents, then recheck identity, containment and
hashes in runner preparation. Component-manifest parent aliases must stay inside
the runtime before any metadata read. Host metadata/hash inspection runs on a
blocking worker; cancellation cannot leave a user command executing there.

The pure plan stays free of Host inspection. The Host adapter chooses another
non-overlapping scratch root when an installed Rust input occupies default scratch.
The native namespace backend can execute from a read-only project with private
output; weaker backends retain their existing writable-cwd requirement. Both bash
preflight and command execution use that shared cwd decision.

### Preserve public boundaries in native diagnostics

Host-private output projection remains in NativeHostMountExportAdapter, shared by
stdout, stderr and Tool error results. Match complete path-component prefixes,
prefer the most specific known mount, and project the original text once so a
replacement cannot be rewritten by another native prefix. Known mounted paths
remain shell-usable; normalized paths outside known mounts redact private backing
ancestors with the existing explicit `<unmapped-host-path>` marker and retain the
relative diagnostic suffix. This never infers a grant or invents a usable namespace
for an unavailable dependency. Top-level OS paths remain ordinary text; literal
role lookalikes, comparisons and Markdown quotes retain their content.

### Use live project mounts for local dependencies

Candidate extension for review: derive the shell sandbox from the selected cwd
grant first, followed only by live same-Process read-only projections supplied by
Host Mount Service. Other read-write roots remain excluded, and grants belonging
only to another Process are absent before the Host adapter is called. This retains
selected cwd, writable-root and standard Rustup inspection behavior; no manifest
or previous cache adds authority. Reconciliation happens before each normal Tool
Process launch. ADR-0058's first-slice single-grant decision remains the shipped
baseline until the user adopts this narrowly scoped candidate delivery. Lower
Sandbox positives alone cannot prove this live-service behavior. Validate each
approved canonical backing root and the existing file-server directory handle
identity again before deriving either structured or shell sandbox authority:
missing, non-directory, retargeted or replaced roots fail with the public namespace
only. Reuse HostDirFs retained-handle metadata rather than a new identity registry. This catches changes observable at reconciliation; it does not
claim atomic protection against concurrent Host path replacement after that check.

A manifest does not grant access. Keep project and outside local dependency mounts
separate, with their original effective access. Do not copy a missing dependency
into scratch to bypass revocation or widen a mount to a common parent. Exercise
absolute/relative dependency paths and symlink escapes against actual native
resolution. Read-only builds redirect output; source mutation stays denied.

### Reproducible native fixture before broad qualification

Use a disposable small Rust project plus a separate local dependency, fixed Rust
1.97 and git. Record package/tool identities and changes required to the fixture
separately from product support. Prefer task-owned fixture provisioning; do not
change personal Rust defaults or reuse personal registry credentials.

Run actual backend readiness and require selected `linux_reified_namespace` for
its positive slots. Record Landlock/path-guard fallback separately. Existing tests
that return early after a probe remain useful portable checks but are not native
acceptance. Positive tasks include build, failing/corrected test and git diff;
negative tasks include absent/unsafe PATH, runtime loss, read-only writes, external
secret absence, network denial, dependency revocation and delayed descendant effects.

Actual Bash Git qualification uses an explicitly selected task-owned standard
`/usr/local/lib/alan-qualification/.../git-2.53.0` prefix under the existing
read-only `/usr` substrate. Its executable bytes match the original private
fixture Git; runtime entries, selected PATH and ownership are recorded. APT
simulation would upgrade unrelated curl packages, so no system packages or global
PATH are changed. The original outside-substrate custom Git PATH remains unavailable
to the native namespace and is recorded separately; no custom-root discovery or
Host grant exception is inferred. Only executed Git version/status/diff operations
are qualified; Host baseline setup is not a confined Git commit acceptance.

After this independent delivery passes review/CI/user merge and canonical sync,
freeze the five real-development task families at three repetitions per platform.
Report first attempts and interventions independently. Alan is the product under
test; Codex authors this implementation.

## Risks / Trade-offs

- Wider runtime projection can expose unrelated files: validate bounded inputs,
  reject whole-home/credential projections and test canary absence.
- PATH/rustup aliases can select another version: freeze identity and preserve
  order, then check actual `cargo`/`rustc` execution and missing-runtime failures.
- Private cache setup can hide project configuration: retain authorized project
  configuration and record unsupported global wrappers without silent changes.
- Capability-dependent tests can silently skip: inspect real readiness/backend
  evidence before assigning native acceptance outcomes.
- Environment changes between probe and execution: construct/revalidate the
  per-command view; never treat a cached startup decision as authority.

## Migration Plan

1. Freeze live environment and the exact native acceptance matrix.
   The native baseline exposed locked inherited submounts under `/usr/lib`:
   nonrecursive bind fails before command execution. Recursive binds preserve
   the mounted tree; read-only mounts must also restrict every child. Probe
   actual root and child writes. Preserve the tree with recursive bind, then
   remount each private `/proc/self/mountinfo` entry read-only using existing
   helpers. Decode kernel path escapes with trusted absolute printf, preserving
   trailing newlines; never evaluate paths as shell code. Probe and execution
   share this logic. A failed child remount reports unavailable before effects.
   This avoids relying on `ro=recursive`, which the installed legacy helper
   silently ignores. This repair precedes PATH support.
2. Add supported PATH transport and regressions under current fail-closed rules.
3. Add bounded installed Rust/runtime projection and private writable cache support.
4. Provision the task fixture and verify actual commands plus negative boundaries.
5. Run affected tests, quality, strict OpenSpec, review and current-head CI.
6. User merges; synchronize canonical deltas and archive before wider repetitions.

No personal store migration or durable evidence rewrite. Reverting support returns
to the existing unavailable/fallback behavior and must not remove user artifacts.

## Sources

Primary Rust documentation confirms runtime-home and cache semantics:
[rustup environment](https://rust-lang.github.io/rustup/environment-variables.html),
[rustup selection precedence](https://rust-lang.github.io/rustup/overrides.html),
[Cargo home](https://doc.rust-lang.org/cargo/guide/cargo-home.html), and
[Cargo environment](https://doc.rust-lang.org/cargo/reference/environment-variables.html).
Executable inventory is current fixture evidence, not inferred from these docs.

Primary mount semantics:
[Linux mount errors](https://man7.org/linux/man-pages/man2/mount.2.html) and
[util-linux mount options](https://raw.githubusercontent.com/util-linux/util-linux/master/sys-utils/mount.8.adoc).
