# macOS Dev Channel Side-by-Side Smoke

> Status: current validation guide for the local-only Alan Dev install channel.

## Purpose

This smoke verifies the local dev install channel after `Alan Dev.app` is
installed on a machine that also has stable `Alan.app` installed.

It covers the OpenSpec side-by-side boundary:

1. stable and dev apps are identifiable by distinct bundle identifiers;
2. launching Alan Dev does not terminate or replace the stable Alan process;
3. repeated Alan Dev launches activate and reuse the dev singleton instead of
   creating a second dev process;
4. repeated stable Alan launches activate and reuse the stable singleton while
   Alan Dev is running;
5. dev shell-control state is created for the current run in the
   `alan-dev-shell-control` namespace;
6. `alan-dev init` writes the current smoke workspace to the dev registry and
   writes workspace runtime state under `.alan/runtime/dev/` without creating
   legacy stable `.alan/sessions` or `.alan/memory` paths.

## Prerequisites

Install stable Alan through the normal stable path. Then install Alan Dev:

```bash
just install-dev
```

If local release signing configuration points at an unavailable Developer ID
identity, use ad-hoc signing for the local-only dev channel:

```bash
ALAN_DEVELOPER_ID_APPLICATION= \
ALAN_SIGNING_IDENTITY=- \
just install-dev
```

If the default cargo target directory is not writable on a local machine, keep
the build output under an ignored repo-local directory:

```bash
ALAN_DEVELOPER_ID_APPLICATION= \
ALAN_SIGNING_IDENTITY=- \
ALAN_CARGO_TARGET_DIR="$PWD/debug/dev-channel-smoke/cargo-target" \
ALAN_XCODE_DERIVED_DATA="$PWD/debug/dev-channel-smoke/xcode-derived" \
ALAN_RELEASE_ARTIFACT_DIR="$PWD/debug/dev-channel-smoke/release-artifacts" \
ALAN_CLI_INSTALL_DIR="$HOME/.local/bin" \
just install-dev
```

## Run

```bash
just dev-channel-smoke
```

The script uses LaunchServices and System Events, so run it in an interactive
macOS user session. In sandboxed automation, allow the command to run outside
the sandbox.

Quit any already-running `Alan Dev.app` before running the smoke. The script
starts Alan Dev from a clean dev-app state so shell-control namespace checks
prove the current launch created the dev namespace rather than reusing stale
temporary files from an earlier run. Stable `Alan.app` may already be running;
if not, the smoke starts it.

## Latest Local Evidence

Last run: 2026-05-24.

Observed result:

```text
Dev channel side-by-side smoke passed.
  stable pid(s): 60215
  dev pid(s): 10528
  dev pid(s) after duplicate launch: 10528
  stable pid(s) after duplicate launch: 60215
  frontmost before dev launch: com.apple.finder
  frontmost after dev launch: com.apple.finder
  frontmost before duplicate dev launch: com.apple.finder
  frontmost after duplicate dev launch: app.alanworks.macos.dev
  frontmost before duplicate stable launch: com.apple.finder
  frontmost after duplicate stable launch: app.alanworks.macos
  dev shell-control: /var/folders/3v/mr9cv4y12l30h9y_mtc2txx80000gn/T/alan-dev-shell-control
  dev registry: /Users/morris/.alan-dev/registry.json
  dev workspace state: /var/folders/3v/mr9cv4y12l30h9y_mtc2txx80000gn/T/alan-dev-channel-smoke-workspace.V6XdvX/.alan/runtime/dev
```

Additional install evidence from the same run:

- stable remained installed as `$HOME/Applications/Alan.app`;
- stable command links remained `/usr/local/bin/alan` and
  `/usr/local/bin/alan-tui`;
- dev installed as `$HOME/Applications/Alan Dev.app`;
- dev command links were installed as `$HOME/.local/bin/alan-dev` and
  `$HOME/.local/bin/alan-dev-tui`;
- `Alan Dev.app` reported bundle id `app.alanworks.macos.dev` and display name
  `Alan Dev`.
