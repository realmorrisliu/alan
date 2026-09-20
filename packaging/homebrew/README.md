# Homebrew Packaging

The former Alan.app cask and embedded-CLI release path is retired under
ADR-0054. The supported distribution is the standalone CLI/Host archive
described in [the current guide](../../docs/standalone_cli_distribution.md).

Do not add a cask, appcast, Sparkle feed, or app-bundle wrapper as a shortcut
for installing the command. A future Homebrew formula may own the standalone
executables directly, but it requires a separate OpenSpec change and must keep
Host lifecycle and channel stores outside the package manager.
