# macOS Dev Channel Side-by-Side Smoke

> Status: current validation guide for the local-only Alan Dev install channel.

## Purpose

This smoke verifies the local dev install channel after `Alan Dev.app` is
installed on a machine that also has stable `Alan.app` installed.

It covers the OpenSpec side-by-side boundary:

1. stable and dev apps are identifiable by distinct bundle identifiers;
2. launching Alan Dev does not terminate or replace the stable Alan process;
3. repeated Alan Dev launches reuse the dev singleton instead of creating a
   second dev process;
4. dev shell-control state uses the `alan-dev-shell-control` namespace;
5. `alan-dev init` writes workspace runtime state under `.alan/runtime/dev/`
   and does not create legacy stable `.alan/sessions` or `.alan/memory` paths.

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

## Latest Local Evidence

Last run: 2026-05-23.

Observed result:

```text
Dev channel side-by-side smoke passed.
  stable pid(s): 60215
  dev pid(s): 49390
  frontmost before dev launch: com.apple.finder
  frontmost after dev launch: app.alanworks.macos.dev
  dev shell-control: .../T/alan-dev-shell-control
  dev workspace state: .../alan-dev-channel-smoke-workspace.../.alan/runtime/dev
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
