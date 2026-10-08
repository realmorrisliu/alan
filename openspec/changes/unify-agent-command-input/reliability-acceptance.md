# Reliability boundary acceptance — 2026-10-08

Owner: `unify-agent-command-input`. The user requested boundary qualification,
reuse of existing implementation and fixes limited to observed failures.
Base is merged main `411828d5f3ad1a6a5e60f93bee6a70ac8deadbcb`.
Local checks below do not imply merged delivery or closure of all parent tasks.
The final crate/Cargo/toolchain manifest SHA-256 is
`33a8bc12e3b3b2fb84aa8bf79ccd5609db4eb2688f3822172442fcb8b3c704a8`.
Source identity and final-head review/CI remain separate receipts.

## Reproduced defect and fix

The new `alan-os-host` regression
`revocation_rejects_preexisting_fids_and_discards_pending_save` failed on the base:
a fid opened before revocation still returned `original` instead of `NoAccess`.
Removing the live mount blocks future walks but existing MountFs fids retain their
backing server. That generic Kernel behavior is preserved.

Host Mount Service now invalidates its opaque Host export before publishing
revocation. Native exports retain their existing HostDirFs; that server rejects
all subsequent fid operations and drops every uncommitted write. Its existing
per-export state lock also serializes create/remove/save with revocation, so a
commit cannot publish after revoke returns. The lock is never held across an
await. An already admitted read may finish. Independently authorized exports of
the same backing remain usable; revocation never grants a rollback promise.

The trait requires export invalidation rather than a silent default no-op.
Both service test exports now use the same revocable HostFS boundary. Already
launched native processes retain their bounded execution scope until cancellation
or completion; revocation prevents new Tool launch authority, and owned interrupt
stops active work and descendants. This slice does not add automatic process
termination on grant revocation.

## Clause evidence

| Boundary | Runnable evidence | Asserted outcomes |
| --- | --- | --- |
| Same Agent, distinct clients and identical text | `alan-os-host --test explicit_command`, `multiple_clients::same_agent_clients_keep_results_and_ordered_cwd_after_targeted_cancel`; TUI `stdio_idle_tests` | Separate local aP connections submit the same script under distinct IDs; two once-only appends, distinct Process/Action correlation and independent rereads; separate success/failure results; neither stdio client consumes the other's identical-prompt answer. |
| Ordered cross-client cwd and queue control | Same `multiple_clients` trace | One client queues cd and another queues a relative write behind active work. Targeted cancellation retains both exact queued IDs paused; explicit continue preserves FIFO and grant choice. Later targeted cancellation and discard publish correlated cancelled events, retain prior effects and never spawn the discarded action. |
| User and generated native commands | Expanded `revoked_queue` trace | Generated Bash uses the same Host sandbox as explicit commands; its script-local cd does not change shared cwd. Effects and Action results agree. |
| Noncurrent project backing | Expanded `revoked_queue`; original native interop trace | Structured Agent write reaches a noncurrent delegated grant. Native shell cannot read that inactive grant. Explicit cd selects it; native read/edit and Agent read use the same file. Existing interop also verifies Agent edit → native git diff/read and native edit → Agent read/search. |
| Read-only, symlink and revoked access | Expanded `revoked_queue`; `host_mounts` tests; HostFS tests | Read-only reads succeed and writes fail without changing backing; escaping symlink reads/writes fail; a generated read of a revoked noncurrent grant fails; revocation during active native work plus owned interrupt preserves completed bytes, pauses queued work and blocks later launch without model fallback. |
| Pending save and stale/save failure | New Native Host Mount and HostFS regressions; existing HostFS `clunk_rejects_host_file_replaced_after_open`, `failed_clunk_staging_preserves_original_host_file`; native interop `stale-edit` | Pre-revocation buffered writes are discarded; old fids reject read/write/open/stat/walk/create/remove/clunk; the original file survives. A fresh authorized export works. Existing stale identity, failed staging and expected-content failures remain truthful. |
| Long stdout/stderr, references and follow-up | Original native interop trace, extended | 20,000 bytes on each stream survive Action output and explicit recovery. The subsequent actual generation request contains the correlated ≤8,000-byte projection, exit status and a namespace reference resolving to the full output. Native output creates no automatic explanatory generation. The test separately accounts for the existing message-count compaction call. |
| Missing/expired retained evidence | Existing Engine `evidence_resolution_distinguishes_missing_and_retention_expired`, namespace `expired_action_evidence_stays_expired_across_repeated_recovery`, AgentFS `expired_action_output_returns_structured_retention_record` | Missing and expired references have different structured outcomes; failed expiry journaling preserves bytes; committed expiry stays expired across repeated recovery and does not resurrect the old payload. These are owning retention traces, distinct from native long-output entry acceptance. |
| Linux filesystem/network isolation | Existing `linux_runner`, `tools::reified_namespace`, `sandbox_backend` suites plus native interop under a supported PATH | Actual unprivileged namespace/mount/network execution, undeclared paths, read-only mounts, private tmp, virtual-only declaration exclusion, output capture and descendant cancellation; backend limitations remain explicit. |

Tasks 2.1, 2.5, 2.12 and 2.14 close local implementation/boundary acceptance:
versioned records and correlated readers, FIFO/cwd and isolated script-local cwd,
Linux/macOS confinement plus explicit degradation, and shared backing/edit paths
including failed save/revocation. Existing protocol, standalone cd, Process runner
and HostFS suites cover their remaining local clauses. Separate invocations retain
#1011's delivered evidence. Final review/CI/merge remain task 4.1; no canonical
sync or archive claim follows from these implementation checkboxes.

The broader tasks 2.4/2.6–2.11/2.13 are not marked complete merely from this
matrix. Complete parent-clause reconciliation and exact delivery receipts remain
part of the final reliability checklist item. No renderer or automatic routing
behavior changes in this slice.

## Linux environments and actual selection

Qualification uses an owned disposable container on the local OrbStack Linux
kernel, image `rust:1.97.0-slim-bookworm` with image digest
`sha256:6d220bf85c74e842a79da63997af8d2e74455c0b8847d8bb3a5888572334991d`.
Tests run as UID 1000, with no privileged flag, SYS_ADMIN capability, Host PID
namespace or credentials. The container's outer seccomp/AppArmor profiles are
unconfined to permit unprivileged namespaces; Alan's inner confinement remains
unchanged. Source is a read-only bind; Cargo cache/target are container-local.
This qualifies the stated environment, not every default Docker installation.
Git was installed only inside that owned container for the existing git-diff check.

Two public-entry runs are required and kept separately:

- Default image PATH includes `/usr/local/cargo/bin`. The existing toolchain
  readiness guard cannot preserve that executable directory inside the reified
  substrate, selects `host_mount_path_guard` and rejects native work with inactive grants using
  `This backend cannot isolate inactive Host Mount reads`. Tests assert denial,
  no unauthorized effects and unchanged files; later single-grant cancellation
  remains executable. This is a degradation pass, not full multi-grant support.
- The same test binary is launched with explicit
  `/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin`, matching the
  supported shell/git fixture. It selects `linux_reified_namespace` and exercises
  the positive cross-grant native cases. No runtime backend override or policy
  bypass is added. General Rust toolchain PATH support remains outside this receipt.

The first direct Ubuntu/OrbStack VM attempt also remains recorded: a locked
read-only kernel-modules submount below `/usr/lib` prevents the runner's plain
bind. Five smoke checks skipped, and selection degraded. Those skips are not
Linux isolation evidence. No recursive mount/policy workaround was introduced.

## Reproducible checks and retained receipts

macOS native target is `aarch64-apple-darwin`, Rust 1.97.0:

- `cargo test --locked -p alan-hostfs --lib`: 23 passing, including full old-fid
  rejection, buffered-save disposal and independent reauthorization.
- `cargo test --locked -p alan-hostfs -p alan-service-manager -p alan-os-host --lib`:
  initial fixed-source run 200 passing and two explicit TypeSafe credential probes ignored; the additional HostFS owner regression passed separately.
- `cargo test --locked -p alan-os-host --test explicit_command`: 3 passing on
  the final boundary assertions.
- `cargo test --locked -p alan-agent-engine -p alan-agentfs -p alan-tools -p alan-terminal-ui`:
  1,958 passing, one explicit live-provider test ignored for missing opt-in credentials.

Linux: HostFS/service/Host libraries 200 passing, two explicit TypeSafe credential probes ignored;
native integration 3 passing with default PATH and 3 with supported PATH.
Existing runner 12, reified plan/runner/toolchain 42 and backend 21 pass; runner
smokes actually execute without skips in this container.

Local logs and the file-by-file source manifest are retained under
`~/Library/Caches/Alan/` as `reliability-*.log` and
`reliability-linux-source-manifest.json`; no secrets are in the exported source.
The source manifest binds all crate files, Cargo manifests/lock and pinned toolchain.
`just quality` passed: source-size and architecture ratchets, Clippy/rustdoc,
Host source/current OpenSpec guards and standalone CLI installer/release checks.
Pinned OpenSpec 1.4.1 strict validation passes all 65 current surfaces. Exact-head
PR review/CI and merge remain pending; local results are not merge receipts.
