## 1. Implement Package Service ownership

- [ ] 1.1 Add `alan-package-service` with aP catalog, transaction, event, and lifecycle files
- [ ] 1.2 Add the versioned `alan-package.yaml` parser with package-id, Skill export, and Tool export validation
- [ ] 1.3 Persist catalog, content, provenance, digests, and transaction recovery only through Package Service's System Store binding
- [ ] 1.4 Validate canonical paths, reject escaping symlinks and export collisions, and compute content digests inside the service boundary
- [ ] 1.5 Implement atomic install, update-with-expected-digest, abort, and exact remove

## 2. Boot and namespace integration

- [ ] 2.1 Add Package Service as a required Service Manager boot unit publishing `/srv/packages` and mount its client tree at `/mnt/packages`
- [ ] 2.2 Pass the channel Package Service backing binding from `alan-os-host` without exposing the raw path to clients
- [ ] 2.3 Project only Process-selected package content read-only at `/lib/pkg/<package-id>`
- [ ] 2.4 Bind only explicit selected Tool exports into `/bin`, reject command collisions, and leave package-local helpers unpromoted
- [ ] 2.5 Install required first-party Skill packages through the ordinary Package Service transaction path before readiness

## 3. Shell and Skill integration

- [ ] 3.1 Add the base-system `/bin/pkg` Tool with `install`, `list`, `show`, `update`, and `remove` against `/mnt/packages`
- [ ] 3.2 Upload only an explicitly named namespace-readable source tree; do not add Git, registry, Host-path, or implicit-directory discovery
- [ ] 3.3 Resolve installed Skill exports through Package Service and pass selected Skills to Agent Processes by descriptor
- [ ] 3.4 Delete direct Agent Execution Engine package-root scanning and any Quartermaster/provider compatibility path

## 4. Verification and cleanup

- [ ] 4.1 Test malformed manifests, traversal, escaping symlinks, id and Tool collisions, digest mismatch, interrupted transactions, update races, and exact removal
- [ ] 4.2 Test stable/dev System Store isolation, restart recovery, required-service failure, `/srv` invalidation, and no raw Host path exposure
- [ ] 4.3 Test per-Process `/lib/pkg` and `/bin` selection, descriptor-only Skill exposure, and fail-closed behavior without Host scanning
- [ ] 4.4 Test first-party and synthetic multi-Skill packages through the same install and resolution path
- [ ] 4.5 Delete obsolete code, fixtures, docs, and vocabulary; run repository checks and strict OpenSpec validation
