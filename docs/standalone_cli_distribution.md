# Standalone CLI Distribution

The supported Alan product entry point is the standalone `alan` executable,
which links the foreground alan9 composition. The development channel provides
`alan-dev` as an alias. The normative distribution contract is the
[canonical standalone CLI specification](../openspec/specs/standalone-cli-distribution/spec.md).
The active unified-input change tracks the delivered migration and remaining
lifecycle work, including explicit recovery selection.

## Local install

```bash
just install
just install-dev
```

The installer defaults to `~/.local/bin`; set `ALAN_CLI_INSTALL_DIR` to an
explicit alternative. The stable channel installs `alan`; the development
channel installs `alan-dev`. Neither channel installs or starts a separate Host
executable, registers launchd services, edits shell startup files, or touches
System/Host Store data.

On upgrade, the installer checks the selected channel's existing CLI and any
legacy Host executable against its ownership manifest before changing files.
It removes a legacy Host only when its digest still matches; a modified or
unowned path stops the upgrade and preserves the existing files and manifest.

Owned command files can be removed without removing stores:

```bash
just uninstall
just uninstall-dev
```

## Release archive

```bash
just release
```

The archive contains `alan`, the `alan-dev` symlink, a manifest, and a SHA-256
checksum. Set `ALAN_TARGET`, `ALAN_RELEASE_VERSION`, or `ALAN_RELEASE_OUT_DIR`
to select the target, version label, or output directory.

## Verification

```bash
just standalone-distribution-test
just quality
```

The check starts only `alan --version` and validates the installer, safe legacy
upgrade, and CLI-only archive. Each ordinary invocation owns its foreground
instance; selecting durable work for recovery is explicit and its runtime flow
is still pending. Alan.app, Sparkle, appcast, cask, and embedded-CLI workflows
are retired. Desktop source has been removed; macOS credentials, Host Mounts
and sandboxing remain owned by Rust runtime adapters.
