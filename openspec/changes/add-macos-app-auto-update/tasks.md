## 1. Sparkle Integration

- [x] 1.1 Add Sparkle 2 to the macOS Xcode project using the repo's chosen dependency mechanism.
- [x] 1.2 Generate a Sparkle EdDSA key pair and store only the public key in tracked app configuration.
- [x] 1.3 Configure the release app bundle with `SUFeedURL=https://alanworks.app/appcast.xml`.
- [x] 1.4 Configure Sparkle public-key metadata and any required updater bundle metadata in the generated Info.plist surface.
- [x] 1.5 Add a focused project/configuration check that fails when the release app is missing Sparkle feed URL or public-key metadata.

## 2. App Update Behavior

- [ ] 2.1 Add a macOS app updater owner that initializes Sparkle for direct app installs.
- [ ] 2.2 Add a `Check for Updates...` command in the native app menu.
- [ ] 2.3 Detect Homebrew-managed installs using the selected first-pass signal from the design open question.
- [ ] 2.4 Disable Sparkle replacement or show a Homebrew update path when the app is Homebrew-managed.
- [ ] 2.5 Keep the first version user-visible and confirmation-based rather than silent background installation.

## 3. Release And Appcast Pipeline

- [ ] 3.1 Add release validation that checks Cargo workspace version, Xcode `MARKETING_VERSION`, GitHub release tag, zip filename, and appcast short version agree.
- [ ] 3.2 Add release validation that checks Xcode `CURRENT_PROJECT_VERSION` / appcast version is monotonic against the previous stable release.
- [ ] 3.3 Add an appcast generation script that reads the notarized zip, GitHub Release asset URL, archive length, version metadata, and Sparkle EdDSA signature.
- [ ] 3.4 Update release documentation so the zip stays on GitHub Releases while `appcast.xml` is deployed to `alanworks.app`.
- [ ] 3.5 Add Cloudflare Pages static assets or documented deployment inputs for `appcast.xml` and the website root without including release zip files.
- [ ] 3.6 Configure or document Cloudflare Pages headers so `/appcast.xml` is served as XML with low-cache behavior.

## 4. Signing And Packaging Validation

- [ ] 4.1 Update release assembly/signing so Sparkle nested framework/helper code is signed correctly with the final app bundle.
- [ ] 4.2 Extend `scripts/validate-release-app.sh` or adjacent focused checks to verify Sparkle nested code signatures.
- [ ] 4.3 Add an appcast validation check for GitHub Release enclosure URL, EdDSA signature metadata, archive length, version, and short version.
- [ ] 4.4 Add a deployed-feed check for `https://alanworks.app/appcast.xml` content type and cache headers.
- [ ] 4.5 Add a Homebrew-path focused check proving Sparkle does not replace a Homebrew-managed app bundle.

## 5. End-To-End Verification

- [ ] 5.1 Build a signed and notarized release app with Sparkle integration enabled.
- [ ] 5.2 Publish or stage a GitHub Release zip asset and matching checksum for a testable version.
- [ ] 5.3 Deploy the matching `appcast.xml` to `alanworks.app`.
- [ ] 5.4 Verify an older signed and notarized `Alan.app` detects the newer appcast item.
- [ ] 5.5 Verify the update downloads, verifies, installs, and relaunches into the newer version.
- [ ] 5.6 Run existing release, Homebrew cask, Apple contract, and brand validation checks.

## 6. Documentation And Change Closure

- [ ] 6.1 Update README and packaging docs to describe direct-app Sparkle updates and Homebrew `brew upgrade --cask alan` updates separately.
- [x] 6.2 Document the Sparkle private-key handling rule without committing private material.
- [ ] 6.3 Add PR notes summarizing Cloudflare Pages, GitHub Releases, Sparkle, and Homebrew ownership boundaries.
- [ ] 6.4 Run `openspec validate add-macos-app-auto-update --strict`.
- [ ] 6.5 Before archiving, sync accepted delta specs into `openspec/specs/` and run `openspec validate --all --strict`.
