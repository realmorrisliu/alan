## Why

Alan still selects different installation and durable-store identities from `alan` and `alan-dev`, although the desktop product has been retired and every terminal invocation already owns its own instance. Development builds and per-process Skill extraction also leave expensive generated state without an explicit retirement boundary; the 2026-10-09 cleanup found roughly 127 GiB in one externally selected Cargo target.

## What Changes

- **BREAKING**: ship, install, and document one `alan` executable. Remove `install-dev`/`uninstall-dev`, runtime `InstallChannel`, argv-based channel detection, and `ALAN_INSTALL_CHANNEL` selection. Debug/release remain compiler profiles, not product identities.
- **BREAKING**: use one product System Store and one separate Host Store without stable/dev suffixes. Offer explicit, verified, restartable adoption of one old channel's paired stores through the existing `legacy-state` command family; never silently merge identities or credentials.
- Preserve independent invocation/Root/endpoint lifetimes and explicit durable recovery. Tests use injected temporary store bindings rather than a product `test` channel.
- Give development build output a worktree owner and bounded lifetime. Preserve the quality gate's required output isolation; reuse compatible ordinary build/install outputs, and retire one-shot validation output with its task.
- Remove per-PID first-party Skill extraction by feeding embedded entries into existing Package Service snapshot validation and seeding. Bind genuinely temporary service files to scoped owners with observable cleanup failures.
- Keep source worktrees, patches, credentials, rollouts, and installed packages outside automatic cache deletion. Do not introduce a global cache service, mandatory sccache, a background janitor, or a new configuration surface for ordinary users.

## Capabilities

### New Capabilities

- `build-artifact-lifecycle`: ownership, reuse, reporting, and safe retirement of repository build and disposable verification artifacts.

### Modified Capabilities

- `standalone-cli-distribution`: one executable and ownership-checked retirement of old entry points.
- `alan-os-system-store`: channel-free backing, explicit store adoption, and scoped scratch ownership.
- `provider-connection-contract`: one metadata/auth pairing with no runtime legacy fallback.
- `connection-service`: profile metadata belongs to the selected explicit store binding, not an install channel.
- `package-management-contract`: channel-free persistence and embedded first-party seeding without leaked intermediate directories.
- `alan-os-host-lifecycle`: product startup and test injection without install channels.
- `local-alan-os-attachment`: endpoint selection stays invocation-scoped after channel removal.
- `service-manager`: explicit store bindings and recovery selection replace channel labels.
- `repository-quality-gate`: retain isolated, reproducible gate output under the build lifecycle policy.
- `product-brand-identity`: current executable examples use `alan`; historical migration identifiers remain permissible.

## Impact

Touches `justfile`, install/release/uninstall/distribution and quality scripts, CI cache mapping, CLI entry points and legacy-state commands, Host paths/boot/local attachment, Connection and Package Service construction, auth path resolution, first-party Skill assets and callers, fixtures/tests, current docs and AGENTS.md. Kernel Process identity and the aP ownership model do not change. No new runtime dependency is planned.

This change supersedes the active stable/dev clauses of the capabilities above and refines storage wording in `unify-agent-command-input`; it does not reopen that change's input-routing work or parked proposals. Historical ADRs and archived changes remain immutable. This delivery is a design proposal, not completed installation, migration, cleanup automation, or implementation.
