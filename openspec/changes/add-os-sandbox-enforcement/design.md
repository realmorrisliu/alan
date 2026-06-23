## Context

`crates/runtime/src/tools/sandbox.rs` (~2354 lines) is a workspace path guard that statically analyzes bash command strings. Its module doc states it does no OS-level sandboxing and enumerates classes of commands it cannot reason about. It already exposes a backend abstraction (`backend_name()` / `SANDBOX_BACKEND_WORKSPACE_PATH_GUARD`), so additional backends fit the existing seam. The repo has cross-platform shell crates (`shell-core`, `shell-core-ffi`).

Slice C ships an auto-approve posture that deliberately keeps bash/network on escalation precisely because there is no OS sandbox. This slice provides that missing enforcement.

## Goals / Non-Goals

**Goals:**
- Kernel-enforced confinement of tool execution (filesystem to workspace; controlled network), independent of command syntax.
- First-class macOS and Linux support via a backend abstraction.
- Safe degradation: no auto-approval of bash/network without an active backend.
- Retire the heuristic command-shape parser as the enforcement mechanism.

**Non-Goals:**
- A fully general multi-tenant container runtime.
- Windows support in this slice.
- Re-litigating the auto-approve UX (slice C); this slice only widens the boundary.

## Decisions

### D1. Backend abstraction with platform/capability selection
Define a `SandboxBackend` trait (prepare profile, spawn confined process, report capabilities) and select an implementation by platform and runtime capability detection:
- `seatbelt` (macOS) — Seatbelt profile via `sandbox-exec`-style confinement.
- Linux — Landlock for filesystem confinement plus seccomp or a network namespace for network control; `bubblewrap` considered as an alternative if it simplifies both.
- `workspace_path_guard` — the existing guard as the degraded fallback only.

Rationale: the seam already exists; modeling backends keeps platform specifics isolated and testable.

### D2. Kernel enforcement replaces string parsing
Confinement is enforced by the OS for the spawned process, so the result is correct regardless of how a command writes (covers the cases the heuristic admits it misses). The bash-string parser is demoted to optional advisory pre-flight and scheduled for removal once backends cover the supported platforms.

### D3. Safe degradation is mandatory
Backend availability is detected at runtime (OS, kernel version, permissions, container constraints). If no enforcing backend is available, the policy MUST treat bash/network as escalate (slice C's pre-sandbox behavior). Sandbox-unavailable MUST NOT be interpreted as sandbox-disabled-so-allow.

### D4. Boundary widening is gated on an active backend
When an enforcing backend is active, the auto-approve boundary (slice C) widens: sandboxed bash and policy-permitted network proceed without prompting; effects that escape the sandbox (out-of-workspace writes, disallowed network) still escalate.

## Risks / Trade-offs

- [Seatbelt is Apple-deprecated] → it remains the de-facto macOS sandbox (also used by peer tools); isolate it behind the backend trait so it can be swapped if Apple provides a replacement.
- [Landlock requires a recent kernel and lacks network control] → pair with seccomp/namespace; detect capability and degrade to escalation on older kernels.
- [Running inside containers/CI may forbid sandbox syscalls] → capability detection + safe degradation path; surface the degraded state.
- [Confinement breaks legitimate tools that need broader access] → such operations escalate (human-in-the-end) rather than failing silently; profile is workspace-scoped with explicit, auditable exceptions.
- [Removing the heuristic prematurely] → keep it as fallback until backends cover supported platforms and are validated; remove only then.

## Migration Plan

1. Land the backend trait and macOS Seatbelt backend; wire capability detection and safe degradation; keep `workspace_path_guard` as fallback.
2. Add the Linux backend (Landlock + seccomp/namespace or bubblewrap).
3. Widen slice C's auto-approve boundary to sandboxed bash/network when a backend is active.
4. Demote then remove the bash-string command-shape parser's enforcement role.

Rollback: disable the new backends (selection falls back to `workspace_path_guard`), which also reverts the boundary to escalate bash/network.

## Open Questions

- Linux: Landlock+seccomp vs bubblewrap as the primary backend.
- Whether network is denied wholesale under the sandbox or allowed to an explicit policy-defined allowlist.
- Minimum supported Linux kernel and the messaging when the host is below it.
