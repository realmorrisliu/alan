# Standalone CLI Distribution

The supported Alan product entry point is the standalone `alan` executable,
which links the foreground alan9 composition. There is one product installation. The normative distribution contract is the
[canonical standalone CLI specification](../openspec/specs/standalone-cli-distribution/spec.md).
The active unified-input change tracks the delivered migration and remaining
lifecycle work, including explicit recovery selection.

## Local install

```bash
just install
```

The installer defaults to `~/.local/bin`; set `ALAN_CLI_INSTALL_DIR` to an
explicit alternative. The installer installs `alan`. It does not install or start a separate Host
executable, registers launchd services, edits shell startup files, or touches
System/Host Store data.

Build and installation reuse Cargo's configured target directory (the checkout's
`target` by default). `ALAN_STANDALONE_TARGET_DIR` explicitly overrides it.
The installer selects the executable reported by that Cargo build, including
configured cross-target output. Controlled verification callers that set
`ALAN_SKIP_BUILD=1` must also supply `ALAN_CLI_SOURCE` from their verified build;
the installer does not guess a binary path. These scripts require `python3`.

On upgrade, the installer checks `.alan-cli-manifest` and both historical
`.alan-cli-manifest-stable`/`.alan-cli-manifest-dev` receipts before changing
any executable. Matching owned `alan-dev`, `alan-os-host` and `alan-os-host-dev`
files and old receipts are retired; modified, unowned, symlinked or malformed
inputs stop the operation. The new `.alan-cli-manifest` owns only `alan`.
Handled INT/HUP/TERM interruption restores the prior executables and receipts.
Concurrent installers/uninstallers are excluded by `.alan-cli-install.lock`.
After an unhandled termination, stop any remaining installer and inspect retained
`.alan-cli-install.*` recovery files before removing the stale lock directory.

Owned command files can be removed without removing stores:

```bash
just uninstall
```

## Release archive

```bash
just release
```

The archive contains `alan` and a manifest, with a separate SHA-256 checksum. Set `ALAN_TARGET`, `ALAN_RELEASE_VERSION`, or `ALAN_RELEASE_OUT_DIR`
to select the target, version label, or output directory.

## Adopt an old installation explicitly

Unset the retired `ALAN_INSTALL_CHANNEL` variable and invoke `alan`. Help and
version remain available without opening product data. On macOS the current
roots are `~/Library/Application Support/Alan/{System Store,Host Store}/`;
on Linux they are beneath `${XDG_DATA_HOME:-$HOME/.local/share}/Alan/`.
Historical stable/dev directories are inputs only. Even one old pair requires
an explicit source choice; the two pairs are never merged.

Stop all old Alan processes and other consumers of the selected stores first.
Inspection prints paths and presence, never credentials. Choose **either**
`stable` or `dev` based on the data you intend to keep:

```bash
alan legacy-state inspect --json
alan legacy-state migrate-installation --from stable --dry-run
alan legacy-state migrate-installation --from stable
```

The dry run creates no stores, locks or migration journal. Apply validates
metadata, credential references, packages and durable execution files offline,
then publishes the selected pair under a transaction journal. It does not call
providers, refresh credentials or replay work. The old source remains intact;
generated runtime cache/tmp is excluded. Populated canonical destinations,
unknown layouts, active writers and unverified source changes refuse adoption.
Native Keychain fallback is no longer consulted; missing credential material
must be resolved explicitly rather than silently borrowing another source.

If publication is interrupted, normal data access is blocked. Repeat the same
apply command to resume, or explicitly roll back:

```bash
alan legacy-state migrate-installation --from stable --rollback
```

Rollback removes only the verified published copy and staging. It refuses once
canonical data has changed or new canonical work exists, and preserves the old
source. A committed retry verifies every published component against the receipt;
missing or changed content/permissions returns an error without importing or
overwriting anything. Keep the migration receipt/recovery
inventory until the retained-source and rollback policy has been resolved;
do not treat them as build cache.

Older `.alan` authored-state inspection/cleanup also requires a historical
source (`alan legacy-state inspect --from stable`,
`alan legacy-state cleanup --from stable`). This separate cleanup can migrate
old connection files and remove recognized generated state; review inspection
first. Authored imports require an explicit source directory. Installing or
uninstalling the executable never performs either data migration.

## Verification

```bash
just standalone-distribution-test
just quality
```

The check starts only `alan --version` and validates the installer, safe legacy
upgrade, and CLI-only archive. Each ordinary invocation owns its foreground
instance; selecting durable work for recovery is explicit and does not happen on ordinary startup. Alan.app, Sparkle, appcast, cask, and embedded-CLI workflows
are retired. Desktop source has been removed; macOS credentials, Host Mounts
and sandboxing remain owned by Rust runtime adapters.
