# macOS Auto Update

Alan uses Sparkle 2 for direct macOS app updates. The stable app feed is:

```text
https://alanworks.app/appcast.xml
```

Directly installed `Alan.app` bundles expose **Check for Updates...** in the
native app menu. The first Sparkle-enabled release keeps updates
user-initiated: automatic checks and silent automatic installation are disabled
in `Info.plist`.

Homebrew-managed installs are owned by Homebrew. When Alan detects the app is
under a Homebrew cask path, or that the running app bundle resolves through a
Homebrew cask path, Sparkle replacement is disabled and the user-facing update
path is below. A command link under a Homebrew prefix is not enough by itself because
direct app installs can create the same `alan` symlink:

```bash
brew upgrade --cask alan
```

## Release Ownership

- GitHub Releases own `alan-<version>-macos.zip` and its checksum.
- `alanworks.app` owns the website root and `appcast.xml`.
- Cloudflare Pages must not contain release zip files.
- Sparkle verifies the update archive through `sparkle:edSignature` in the
  appcast. The private signing key stays outside git under
  `release-secrets/sparkle_ed25519_private.pem` or an equivalent CI secret.

## Release Flow

1. Run `just release-check`.
2. Build, sign, notarize, staple, and archive with `just release`.
3. Upload `target/release-artifacts/alan-<version>-macos.zip` and the matching
   `.sha256` to the GitHub Release `v<version>`.
4. Generate the appcast:

   ```bash
   ALAN_RELEASE_ARCHIVE=target/release-artifacts/alan-<version>-macos.zip \
   ALAN_RELEASE_ARCHIVE_URL=https://github.com/realmorrisliu/alan/releases/download/v<version>/alan-<version>-macos.zip \
   ALAN_RELEASE_TAG=v<version> \
   ALAN_PREVIOUS_SPARKLE_VERSION=<previous-build-number> \
   ALAN_SPARKLE_ED_SIGNATURE=<signature-from-sparkle-sign_update> \
   scripts/generate-appcast.sh
   ```

5. Validate version metadata and the generated appcast:

   ```bash
   ALAN_RELEASE_TAG=v<version> \
   ALAN_RELEASE_ARCHIVE=target/release-artifacts/alan-<version>-macos.zip \
   ALAN_APPCAST_PATH=target/release-artifacts/appcast.xml \
   ALAN_PREVIOUS_SPARKLE_VERSION=<previous-build-number> \
   scripts/validate-release-version-metadata.sh

   scripts/validate-appcast.sh target/release-artifacts/appcast.xml
   ```

6. Copy `target/release-artifacts/appcast.xml` into the Cloudflare Pages deploy
   input and deploy `alanworks.app`.
7. Verify deployed headers:

   ```bash
   scripts/check-deployed-appcast.sh https://alanworks.app/appcast.xml
   ```

8. For a staged old-to-new update test, provide an older signed app, the newer
   signed app, and the matching appcast. The smoke preflight rejects appcasts
   whose Sparkle short version or build does not match the newer app bundle:

   ```bash
   ALAN_OLD_APP=/Applications/Alan.app \
   ALAN_NEW_APP=target/xcode-derived/Build/Products/Release/Alan.app \
   ALAN_APPCAST_PATH=target/release-artifacts/appcast.xml \
   scripts/smoke-macos-auto-update.sh
   ```

   Set `ALAN_ALLOW_INTERACTIVE_UPDATE_SMOKE=1` only when you are ready for the
   script to launch the old app and perform the visible Sparkle update check.

## Checks

Use these focused checks while changing auto-update behavior:

```bash
just apple-auto-update-tests
just guard-macos-auto-update
bash clients/apple/scripts/check-shell-contracts.sh
```
