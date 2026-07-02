## 1. Capability Probe And Backend Reporting

- [x] 1.1 Add a Linux reification capability probe that reports user namespace,
  mount namespace, bind mount, read-only remount, scratch/tmp mount, and network
  confinement support.
- [x] 1.2 Add a distinct `linux_reified_namespace` backend state/name and audit
  fields without selecting it by default.
- [x] 1.3 Add unit tests for capability-report formatting, unavailable reasons,
  and safe fallback ordering to Landlock/path guard.

## 2. Reified Namespace Plan Model

- [x] 2.1 Add a pure `ReifiedNamespacePlan` model that separates declared host
  mounts, read-only execution substrate, cwd, argv, scratch/tmp, and network
  posture.
- [x] 2.2 Build plan derivation from the host-backed mount declaration /
  sandbox authority data while excluding virtual Alan OS mounts.
- [x] 2.3 Add path translation helpers that map projected host paths to reified
  namespace paths when a declared mount matches.
- [x] 2.4 Add unit tests for workspace seed mounts, extra read-write mounts,
  read-only mounts, virtual mount exclusion, cwd translation, and out-of-view
  rejection.

## 3. Linux Runner Slice

- [x] 3.1 Add a `ReifiedNamespaceRunner` trait and keep the existing Landlock
  execution path as fallback.
- [x] 3.2 Implement an opt-in Linux runner that attempts unprivileged user/mount
  namespace creation, bind mounts the plan, applies read-only remounts, and execs
  the requested command.
- [x] 3.3 Preserve safe degradation when namespace setup fails, including explicit
  error/audit reporting and no silent ambient-host execution.

## 4. Enforcement And Policy Integration

- [ ] 4.1 Wire backend selection to prefer `linux_reified_namespace` only when the
  probe and runner smoke checks pass.
- [ ] 4.2 Ensure network-denied commands remain denied under the reified backend,
  route to a human, or fall back to a network-confined backend when degraded;
  never allow autonomous reviewer approval without network confinement.
- [ ] 4.3 Update bash/tool policy audits to distinguish reified namespace paths
  from projected host paths.

## 5. Verification And PRs

- [ ] 5.1 Add Linux-only smoke tests gated by capability detection for visible
  `/mnt/<name>` mounts, absent undeclared home paths, read-only mount mutation
  rejection, writable mount mutation, and network denial.
- [ ] 5.2 Run focused Rust tests for the probe, plan model, path translation, and
  fallback behavior; run Linux smoke tests when host capabilities are available.
- [ ] 5.3 Run clippy for touched crates, OpenSpec strict validate, and diff
  checks.
- [ ] 5.4 Update parent namespace-driven sandbox task state and open stacked PRs
  for each landed implementation slice.
