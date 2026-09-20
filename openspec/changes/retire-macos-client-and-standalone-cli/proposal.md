# Proposal

## Why

Alan for macOS is a retired product direction. The repository still treats the
Swift/Ghostty desktop app, its FFI workspace model, Sparkle release pipeline and
app-embedded CLI as current delivery surfaces, which keeps a second terminal and
execution owner alive after Herdr became the preferred host. This change removes
those product obligations while preserving Alan OS Host lifecycle, credentials,
Host Mounts, sandboxing, System/Host Stores and the terminal-neutral Rust CLI.

## What Changes

- **BREAKING** Remove the Alan for macOS app-bundle, Sparkle, appcast,
  release-signing, dev-channel app, and app-embedded CLI delivery paths; no new
  desktop update or windowing behavior is introduced.
- Keep the existing Apple source and shell-core crates as explicitly isolated
  maintenance-only source for this slice. A later source-removal change must
  first re-audit any platform, safety, credential, store, or test consumer;
  this change does not delete those trees merely because the product direction
  is retired.
- Add a standalone CLI/Host distribution contract and repository-local install
  path that installs `alan`, `alan-os-host`, and `alan-os-host-dev` without an
  app bundle or launchd product registration.
- Keep `alan`'s direct Host, Connection, Skills and Shell commands, channel-
  isolated System/Host Store paths, native macOS Host credential loading,
  Host Mount authority and OS sandbox adapters.
- Replace current Apple-focused quality/test commands with Rust CLI/Host and
  terminal-host acceptance checks; preserve historical macOS archives unchanged.
- Update active guides, ADR links and OpenSpec contracts so deleted consumers do
  not remain normative; retained macOS platform behavior is described separately.

## Capabilities

### New Capabilities

- `standalone-cli-distribution`: The supported distribution contains the
  terminal-neutral CLI and dedicated Alan OS Host executables without a desktop
  app, embedded UI, or app-owned lifecycle.

### Modified Capabilities

- `alan-app-distribution`: Remove the retired app-first distribution contract and
  replace it with the standalone CLI/Host boundary.
- `alan-app-service-integration`: Keep direct file-boundary ownership explicit;
  no desktop app delivery path is a prerequisite for surviving clients.
- `documentation-governance`: Record the completed desktop consumer removal and
  preservation of platform safety/data owners.
- `product-brand-identity`: Remove active app/bundle/update identity claims from
  current distribution guidance; retain Alan and channel-neutral CLI naming.
- `repository-quality-gate`: Remove Apple-app-only gates and add standalone
  CLI/Host distribution checks.

## Impact

Affected areas include app-only release/install scripts, Homebrew/appcast
inputs, `justfile`, CI quality guards, active distribution docs, and the listed
OpenSpec contracts. `clients/apple/`, the Ghostty submodule,
`crates/shell-core`, and `crates/shell-core-ffi` remain present but are marked
maintenance-only until a separately scoped source-removal change completes its
consumer audit. `crates/alan`, `crates/os-host`, provider/credential adapters,
Host Mount services, OS sandbox code, System/Host Store data and Alan Kernel
remain surviving runtime owners. No installed user app, account, credential,
store, or external release feed is mutated by repository changes.
