---
name: install
section: 8
title: "INSTALL(8)"
whatis: "install - install and update the standalone alan binary"
order: 5
---

## NAME

install - install and update the standalone `alan` binary

## SYNOPSIS

```text
curl -fsSL https://alanworks.app/install.sh | sh
curl -fsSL https://alanworks.app/install.sh | sh -s -- [--channel stable|beta]
     [--version VER] [--install-dir DIR] [--force] [--dry-run] [--no-modify-path]

alan update [--check] [--json] [--channel stable|beta] [--version VER] [--force]
alan update channel [stable|beta]
```

## DESCRIPTION

`install.sh` is pure POSIX `sh`. It installs only the standalone `alan` binary,
by default into `~/.local/bin`. It verifies the archive's SHA-256 before
unpacking or replacing anything, never touches System Store or Host Store data,
and starts no background services.

After installing:

```text
alan login    # chatgpt, openrouter, vertex, magpie or openai
alan          # launch the foreground alan9 TUI
```

## PLATFORMS

```text
aarch64-apple-darwin       macOS Apple Silicon   Developer ID signed, notarized
x86_64-apple-darwin        macOS Intel           Developer ID signed, notarized
x86_64-unknown-linux-gnu   Linux x86_64 (glibc)  SHA-256 verified
aarch64-unknown-linux-gnu  Linux arm64 (glibc)   SHA-256 verified
```

macOS executables are signed with `codesign --options runtime --timestamp` and
notarized with `xcrun notarytool`. Bare Mach-O files cannot be stapled, so
releases verify with `codesign --verify --strict` and publish the signing proof
and notary submission alongside.

## CHANNELS

```text
stable   https://alanworks.app/latest.json   tagged v* releases
beta     https://alanworks.app/beta.json     rolling builds from main
```

Both manifests use `schema_version: 1` and list, per target, the asset URL,
`sha256`, size, and `signed` / `notarized` flags. Each archive
`alan-<version>-<target>.tar.gz` contains `./alan` and `./manifest.json`.

## UPDATING

`alan update` downloads, verifies SHA-256 and replaces the binary atomically.
`alan update channel beta` saves a channel preference; `--channel` overrides it
once. Switching channels does not need `--force`; a same-channel downgrade needs
both `--version` and `--force`.

Interactive launches read a cached 24-hour release check and show a quiet notice
when an update is available. Piped runs and `--ephemeral` skip it.

## OPTIONS

`--channel stable|beta` (`ALAN_CHANNEL`)
: Release channel. Default `stable`.

`--version VER` (`ALAN_VERSION`)
: Install a specific version, `0.1.0` or `v0.1.0`.

`--install-dir DIR` (`ALAN_INSTALL_DIR`)
: Destination directory.

`--force` (`ALAN_INSTALL_FORCE=1`)
: Overwrite an unowned or modified `alan`.

`--dry-run`
: Check platform, manifest and ownership without writing.

`--no-modify-path` (`ALAN_NO_MODIFY_PATH=1`)
: Leave shell startup files alone.

## ENVIRONMENT

`ALAN_NO_UPDATE_CHECK=1` or `CI=1` disables the startup check.
`ALAN_MANIFEST_URL` overrides the release manifest URL.

## FILES

```text
$INSTALL_DIR/.alan-cli-manifest       ownership: alan|<sha256>
$INSTALL_DIR/.alan-cli-install.lock   serializes install/update
<System Store>/services/updater/channel.json
<System Store>/services/updater/last-check.json
```

`install.sh`, `alan update` and the source installer share one ownership and
locking contract. An unowned or locally modified `alan` is refused without
`--force`. An interrupted install restores the prior executable.

## FROM SOURCE

```text
just install     # build and install from a checkout
just uninstall   # remove owned command files; stores stay
```

## SEE ALSO

alan(1)
