## Why

Auto-approving routine work (slice C) is only safe to extend to `bash` and network if the agent's process execution is constrained by the operating system. Today `crates/runtime/src/tools/sandbox.rs` is a ~2354-line heuristic that parses bash command *strings* to guess whether they write outside the workspace; by its own documentation it cannot catch programs that write via internal logic without an explicit path operand (`git init`, `find -delete`, `sed -f`, build/task runners, etc.). It is a fragile, ever-losing race against what the kernel should enforce. This slice introduces real OS-level enforcement so the auto-approve boundary can widen safely, and retires the heuristic's enforcement role.

This is **slice D of the four-slice TUI parity program**. It depends on slice C (`introduce-auto-approve-policy`); once OS enforcement is in place, slice C's conservative boundary is relaxed to allow sandboxed bash/network.

> **Linux network confinement (task 3.2) is required by `add-autonomous-review-mode`** and scheduled as that change's first step, so both platforms confine filesystem + network and the reviewer applies uniformly (no platform asymmetry).

## What Changes

- **Pluggable sandbox backends** behind the existing `backend_name` abstraction in `sandbox.rs`: `seatbelt` (macOS), a Linux backend (`landlock` for filesystem + `seccomp`/namespace for network, or `bubblewrap`), and the existing `workspace_path_guard` as the degraded fallback.
- **OS-enforced workspace confinement and network control:** tool execution runs under a kernel-enforced profile that confines writes to the workspace and controls network, independent of how the command is written.
- **Cross-platform from the start:** the design treats macOS and Linux as first-class; backend selection is by platform and capability detection (kernel version, permissions).
- **Safe degradation (critical):** when no OS sandbox backend is available on the host, the system SHALL NOT auto-approve bash/network — it falls back to escalation. Sandbox-unavailable is not sandbox-off.
- **Retire the heuristic enforcement role:** with kernel enforcement active, the bash-string command-shape parser is no longer the line of defense; it is reduced to (at most) advisory pre-flight and slated for removal.
- **Loosen the auto-approve boundary:** when a sandbox backend is active, slice C's boundary widens so sandboxed bash and (policy-permitted) network proceed without prompting; out-of-sandbox effects still escalate.

## Capabilities

### New Capabilities
- `os-sandbox-enforcement`: the OS-level sandbox contract — the backend abstraction, macOS Seatbelt and Linux backends, kernel-enforced workspace/network confinement, safe degradation when unavailable, and the relationship to the auto-approve boundary.

### Modified Capabilities
- `auto-approve-policy`: the escalation boundary is widened to allow sandboxed bash/network when an OS sandbox backend is active, and to require escalation when no backend is available.

## Impact

- Code: `crates/runtime/src/tools/sandbox.rs` (backend abstraction + backends), tool execution path, `crates/tools`; platform glue via `shell-core`/`shell-core-ffi`; macOS Seatbelt profile, Linux Landlock/seccomp (or bubblewrap) integration.
- Platform: macOS uses `sandbox-exec`/Seatbelt (note Apple-deprecated but de-facto standard); Linux requires a sufficiently recent kernel for Landlock and adds seccomp/namespace for network.
- Security: replaces fragile string-parsing enforcement with kernel enforcement; explicit safe-degradation behavior.
- Depends on slice C; relaxes slice C's boundary once active.
