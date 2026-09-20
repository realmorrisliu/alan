# Standalone CLI/Host Distribution

The supported Alan delivery boundary is the terminal-neutral Rust CLI plus the
Alan OS Host executables. The durable contract is
[`standalone-cli-distribution`](../openspec/specs/standalone-cli-distribution/spec.md)
and its completed distribution migration is recorded in
[`retire-macos-client-and-standalone-cli`](../openspec/changes/archive/2026-09-20-retire-macos-client-and-standalone-cli/).

## Local install

```bash
just install
just install-dev
```

The installer defaults to `~/.local/bin`; set `ALAN_CLI_INSTALL_DIR` to an
explicit alternative. It installs `alan`, both Host executables, and the
`alan-dev` development alias when the dev channel is selected. It does not
start a Host, register launchd services, edit shell startup files, or touch
System/Host Store data.

Owned command files can be removed without removing stores:

```bash
just uninstall
just uninstall-dev
```

## Release archive

```bash
just release
```

The archive contains `alan`, `alan-dev`, `alan-os-host`, `alan-os-host-dev`, a
manifest, and a SHA-256 checksum. Set `ALAN_TARGET`, `ALAN_RELEASE_VERSION`, or
`ALAN_RELEASE_OUT_DIR` to select the target, version label, or output directory.

## Verification

```bash
just standalone-distribution-test
just quality
```

The check starts only `alan --version`; Host lifecycle remains the existing
channel-aware CLI attachment/start path. Alan.app, Sparkle, appcast, cask, and
embedded-CLI workflows are retired. Desktop source has been removed; macOS
credentials, Host Mounts and sandboxing remain owned by Rust runtime adapters.
