# macOS Dev Channel Side-by-Side Smoke

> Status: current validation guide for the local-only Alan Dev install channel.

## Purpose

This smoke verifies the local dev install channel after `Alan Dev.app` is
installed on a machine that also has stable `Alan.app` installed.

It covers the OpenSpec side-by-side boundary:

1. stable and dev apps are identifiable by distinct bundle identifiers;
2. launching Alan Dev does not terminate or replace the stable Alan process;
3. repeated Alan Dev launches activate and reuse the dev singleton instead of
   creating or retaining a second dev process;
4. repeated stable Alan launches activate and reuse the stable singleton while
   Alan Dev is running without creating or retaining a second stable process;
5. dev shell-control state is created for the current run in the
   `alan-dev-shell-control` namespace;
6. an explicit `alan-dev host legacy-state import skill` writes only to the dev System Store,
   does not create stable state, and does not recreate `.alan` or `.alan-dev`;
7. removed `init` and `workspace` commands remain unavailable.

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

## Expected evidence

```text
Dev channel side-by-side smoke passed.
  stable pid(s): …
  dev pid(s): …
  dev shell-control: /var/folders/…/alan-dev-shell-control
  isolated dev System Store fixture: …/Alan/System Store/dev
```
