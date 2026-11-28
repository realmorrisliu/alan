# Standalone CLI Distribution

The supported Alan product entry point is the standalone `alan` executable,
which links the foreground alan9 composition. There is one product installation. The normative distribution contract is the
[canonical standalone CLI specification](../openspec/specs/standalone-cli-distribution/spec.md).
The active unified-input change tracks the delivered migration and remaining
lifecycle work, including explicit recovery selection.

## Install prebuilt release binaries

Stable channel (default, tagged `v*` releases):

```bash
curl -fsSL https://alanworks.app/install.sh | sh
```

Beta channel (rolling pre-release builds from `main`, Apple Developer ID signed & notarized):

```bash
curl -fsSL https://alanworks.app/install.sh | sh -s -- --channel beta
```

The remote installer (`scripts/install.sh`, hosted at `https://alanworks.app/install.sh`)
is pure POSIX `sh`, defaults to `~/.local/bin`, verifies the tarball SHA-256
before unpacking or replacing any executable, and installs only the standalone
`alan` binary (never touching System/Host Store data or starting background
services). After installation:

```bash
alan login   # connect ChatGPT, OpenRouter, Vertex AI, Magpie, or OpenAI
alan         # launch the foreground alan9 TUI
```

Supported release targets:

| Target triple | Platform | Signing & verification |
| --- | --- | --- |
| `aarch64-apple-darwin` | macOS Apple Silicon | Developer ID signed & notarized (`codesign --verify`), SHA-256 |
| `x86_64-apple-darwin` | macOS Intel | Developer ID signed & notarized (`codesign --verify`), SHA-256 |
| `x86_64-unknown-linux-gnu` | Linux x86_64 (glibc) | SHA-256 verified |
| `aarch64-unknown-linux-gnu` | Linux arm64 (glibc) | SHA-256 verified |

Installer flags and environment variables:

- `--version <VER_OR_TAG>` (`ALAN_VERSION`): install a specific version (`0.1.0` or `v0.1.0`).
- `--channel <stable|beta>` (`ALAN_CHANNEL`): select release channel (default `stable`; `beta` selects `https://alanworks.app/beta.json`).
- `--install-dir <DIR>` (`ALAN_INSTALL_DIR`): override destination directory (default `~/.local/bin`).
- `--manifest-url <URL>` (`ALAN_MANIFEST_URL`): override release manifest URL (`latest.json` or `beta.json`).
- `--force` (`ALAN_INSTALL_FORCE=1`): overwrite an existing unowned or modified `$INSTALL_DIR/alan`.
- `--dry-run`: validate platform, manifest, and ownership preflight without writing to disk.
- `--no-modify-path` (`ALAN_NO_MODIFY_PATH=1`): skip adding `$INSTALL_DIR` to shell startup files.
- `ALAN_RELEASE_BASE_URL`: override release download base URL for `--version` installs.

## Self-update (`alan update`)

Installed binaries include built-in self-update over `latest.json` (`stable`)
or `beta.json` (`beta`) and the same `.alan-cli-manifest` ownership contract:

```bash
alan update --check             # check whether a newer release is available on the selected channel
alan update --check --json      # machine-readable status check
alan update                     # download, verify SHA-256, and replace atomically
alan update channel             # show persisted channel (`stable` or `beta`) and active binary
alan update channel beta        # save `beta` preference (or `/settings` -> `Release Channel` in TUI)
alan update channel stable      # save `stable` preference for the next `alan update`
alan update --channel beta      # one-shot override to install rolling beta builds from main
alan update --channel stable    # one-shot override to switch back from beta to stable releases
alan update --version 0.1.0     # target a specific release (--force to downgrade)
```

Channel and endpoint precedence is:
`--manifest-url` > `ALAN_MANIFEST_URL` / `ALAN_RELEASE_MANIFEST_URL` > `--version` > `--channel` / `ALAN_CHANNEL` > persisted channel (`services/updater/channel.json`) > compiled binary channel (`ALAN_RELEASE_CHANNEL`).
Selecting another channel via `alan update channel <target>` (or `/settings` -> `Release Channel`) or passing `--channel <target>` allows switching back from `beta` (`0.1.6-beta.*`)
to `stable` (`0.1.5`) without passing `--version` + `--force`; same-channel
downgrades remain refused unless both `--version` and `--force` are supplied.

Interactive TUI launches also read a cached non-blocking 24-hour release check
(`services/updater/last-check.json` inside the System Store) against the selected
target channel endpoint (`latest.json` or `beta.json`), invalidate the cache
automatically if the target channel changes, and surface a quiet notice
(`Update available: ...`, `Beta update available: ...`, or `Channel switch pending ...`) when action is available. Set `ALAN_NO_UPDATE_CHECK=1` (or `CI=1`) to disable startup checks;
`--ephemeral` and piped/non-TTY runs skip checks automatically.

## Ownership and upgrade safety

`scripts/install.sh`, `alan update`, and `scripts/install-cli.sh` share one
ownership and locking contract:

- Concurrent installs/updates/uninstalls are serialized with `$INSTALL_DIR/.alan-cli-install.lock`.
- Ownership is recorded in `$INSTALL_DIR/.alan-cli-manifest` as `alan|<64-hex-sha256>\n` (checksum of the installed `alan` binary).
- Retired `ALAN_INSTALL_CHANNEL` environment overrides are rejected.
- On upgrade, matching owned legacy `.alan-cli-manifest-stable`/`.alan-cli-manifest-dev` receipts and `alan-dev`, `alan-os-host`, and `alan-os-host-dev` files are verified before retirement; modified, unowned, symlinked, or malformed legacy paths stop the operation.
- An unowned or locally modified `$INSTALL_DIR/alan` is refused unless `--force` (`ALAN_INSTALL_FORCE=1`) is passed explicitly.
- Handled `INT`/`HUP`/`TERM` interruption during publication restores prior executables and receipts. After an unhandled termination, stop any remaining installer and inspect retained `.alan-cli-install.*` recovery files before removing the stale lock directory.

## Apple Developer ID signing and release manifests (`latest.json` / `beta.json`)

macOS release executables (`aarch64-apple-darwin` and `x86_64-apple-darwin`) are
signed with Apple Developer ID Application (`codesign --options runtime --timestamp`)
and notarized via `xcrun notarytool`. Because bare Mach-O executables do not
support `xcrun stapler staple`, releases verify `codesign --verify --strict --verbose=2`
and publish signing proof and notary submission JSON artifacts alongside each
release.

Two release channels share `schema_version: 1`:

- `stable` (`https://alanworks.app/latest.json`, cached `max-age=300`): proxies `https://github.com/thinksmart-systems/alan/releases/latest/download/latest.json` for tagged `v<version>` releases.
- `beta` (`https://alanworks.app/beta.json`, cached `max-age=60`): proxies `https://github.com/thinksmart-systems/alan/releases/download/beta/latest.json` for rolling pre-releases built from `main` (`<next_patch>-beta.<YYYYMMDD>.<run_number>`, rolling `beta` tag, retaining the current + 1 previous archive version).

Release builds inject `ALAN_BUILD_VERSION`, `ALAN_BUILD_CHANNEL`, and
`ALAN_BUILD_COMMIT` at compile time via `crates/alan/build.rs` without mutating
`Cargo.toml`, so `alan --version` displays channel and commit metadata (for
example `0.1.6-beta.20261123.42 (beta, 6c1dfd7)`) while `alan update` compares
pure SemVer (`ALAN_VERSION`). Every release attaches `latest.json`, `checksums.txt`,
and per-archive `.sha256` files:

```json
{
  "schema_version": 1,
  "product": "alan",
  "channel": "stable",
  "version": "0.1.0",
  "tag": "v0.1.0",
  "commit": "6c1dfd7",
  "published_at": "2026-11-20T00:00:00Z",
  "release_url": "https://github.com/thinksmart-systems/alan/releases/tag/v0.1.0",
  "notes_url": "https://github.com/thinksmart-systems/alan/releases/tag/v0.1.0",
  "installer_url": "https://alanworks.app/install.sh",
  "targets": {
    "aarch64-apple-darwin": {
      "asset": "alan-0.1.0-aarch64-apple-darwin.tar.gz",
      "url": "https://github.com/thinksmart-systems/alan/releases/download/v0.1.0/alan-0.1.0-aarch64-apple-darwin.tar.gz",
      "sha256": "<64-hex>",
      "size": 12345678,
      "signed": true,
      "notarized": true
    }
  }
}
```

Each release tarball `alan-<version>-<target>.tar.gz` contains `./alan` and
`./manifest.json` (`"binaries": ["alan"]`), with no separate `alan-dev` or
`alan-os-host` executables.

## Local source build (developer workflow)

To build and install directly from a local git checkout:

```bash
just install
```

The source installer (`scripts/install-cli.sh`) defaults to `~/.local/bin`; set
`ALAN_CLI_INSTALL_DIR` to an explicit alternative. It installs `alan`, never
touches shell startup files or System/Host Store data, and participates in the
same `.alan-cli-manifest` ownership contract as `install.sh` and `alan update`.

Build and installation reuse Cargo's configured target directory (the checkout's
`target` by default). `ALAN_STANDALONE_TARGET_DIR` explicitly overrides it.
The installer selects the executable reported by that Cargo build, including
configured cross-target output. Controlled verification callers that set
`ALAN_SKIP_BUILD=1` must also supply `ALAN_CLI_SOURCE` from their verified build;
the installer does not guess a binary path. These scripts require `python3`.

Owned command files can be removed without removing stores:

```bash
just uninstall
```

### Local release archive assembly

```bash
just release
```

`scripts/assemble-cli-release.sh` builds `alan-<version>-<target>.tar.gz`
containing `./alan` and `./manifest.json`, along with `<archive>.sha256`. Set
`ALAN_TARGET`, `ALAN_RELEASE_VERSION`, or `ALAN_RELEASE_OUT_DIR` to select the
target, version label, or output directory. When macOS signing credentials
(`ALAN_SIGN_IDENTITY`, optional notary configuration) are configured on
`*-apple-darwin`, `scripts/sign-notarize-macos.sh` signs and notarizes `./alan`
before packing.

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
An existing native-owned payload must have its writer lock; a missing lock
refuses adoption without creating one in the old source. Keep all old writers
stopped throughout migration, including owners without native file locks.
Native Keychain fallback is no longer consulted; missing credential material
must be resolved explicitly rather than silently borrowing another source.

If publication is interrupted, normal data access is blocked. Repeat the same
apply command to resume, or explicitly roll back:

```bash
alan legacy-state migrate-installation --from stable --rollback
```

Rollback removes only the verified published copy and staging. It refuses once
canonical data has changed or new canonical work exists, and preserves the old
source. A committed retry excludes current store consumers while verifying every
published durable component against the receipt; regenerated Services cache/tmp
is excluded from this verification. Missing or changed durable content/permissions
returns an error without importing or overwriting anything. Retry dry-run requires
the existing installation lock and never recreates it if missing. Keep the migration receipt/recovery
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
bash scripts/test-remote-installer.sh
just standalone-distribution-test
just quality
```

The checks start only `alan --version` and validate both the remote POSIX curl
installer (`latest.json`, SHA-256 verification, `.alan-cli-manifest` ownership,
`--force`, `--dry-run`) and the local source-build installer, safe legacy
upgrade, and CLI-only archive. Each ordinary invocation owns its foreground
instance; selecting durable work for recovery is explicit and does not happen on
ordinary startup. `Alan.app`, Sparkle, appcast, cask, and embedded-CLI workflows
are retired. Desktop source has been removed; macOS credentials, Host Mounts
and sandboxing remain owned by Rust runtime adapters.
