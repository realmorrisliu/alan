## 1. Backend abstraction

- [x] 1.1 Define `SandboxBackendKind` (`Seatbelt`/`Landlock`/`WorkspacePathGuard`) with stable names and an `is_os_enforced` predicate (`sandbox_backend.rs`)
- [x] 1.2 Implement runtime capability detection (`detect_backend`): macOS→Seatbelt when `sandbox-exec` present, Linux→Landlock when the LSM is advertised, else fallback
- [x] 1.3 Keep `workspace_path_guard` as the explicit degraded fallback
- [x] 1.4 Tests: detection yields a known kind per platform; fallback path
- [x] 1.5 Route bash execution through backend selection (`Sandbox::build_confined_command`): Seatbelt-wrapped on macOS, direct under the path guard otherwise

## 2. macOS Seatbelt backend

- [x] 2.1 Generate a Seatbelt (SBPL) profile confining writes to the (canonicalized) workspace + temp and denying network (`seatbelt_profile`)
- [x] 2.2 Wire bash execution to spawn under `sandbox-exec` with the profile when Seatbelt is active
- [x] 2.3 On-machine test: in-workspace write allowed; out-of-workspace (HOME) write kernel-blocked (`seatbelt_enforces_workspace_write_boundary_on_macos`)

## 3. Linux backend

- [x] 3.1a Detection scaffold for Landlock availability (LSM advertised in `/sys/kernel/security/lsm`)
- [x] 3.1 Implement filesystem confinement via Landlock (`apply_landlock`, applied in `pre_exec`); validated on an Ubuntu kernel 7.0.11 VM
- [x] 3.2 Network control via Landlock ABI v4 net rules (deny all TCP bind/connect) in `apply_landlock`; `confines_network()` detection; validated on the Ubuntu VM (`landlock_confines_network_on_linux`)
- [x] 3.3 Capability detection + degradation on insufficient kernel (returns path-guard when Landlock LSM absent)
- [x] 3.4 On-machine test mirroring macOS: in-workspace write allowed, HOME write kernel-blocked (`landlock_enforces_workspace_write_boundary_on_linux`)

## 4. Safe degradation + boundary widening

- [x] 4.1 Safe-degradation rule encoded: `allows_unattended_bash_and_network` is true only for OS-enforced backends (no backend ⇒ escalate); unit-tested
- [ ] 4.2 When a backend is active, widen the auto-approve boundary (slice C) to sandboxed bash/policy-permitted network; keep sandbox-escaping effects escalating — needs the trait wiring from 1.5
- [ ] 4.3 Report active backend / degraded state in the decision audit (currently reports the executing path-guard)

## 5. Retire heuristic enforcement

- [ ] 5.1 Demote the bash command-shape parser to advisory pre-flight once OS backends execute confinement
- [ ] 5.2 Remove the parser's enforcement role after backends cover supported platforms and are validated

## 6. Verification

- [x] 6.1 `just verify` green with the backend foundation (abstraction, detection, profile, degradation rule)
- [ ] 6.2 Manual on-machine smoke (macOS + Linux): sandboxed bash confined; escapes escalate; no-backend host escalates bash/network
