# Homebrew Cask Packaging

Alan's Homebrew distribution is app-first. The cask installs the signed and
notarized `Alan.app` artifact, then exposes the `alan` command from inside the app
bundle through Homebrew's `bin` directory.

Canonical install command:

```bash
brew install --cask alan
```

Canonical update command:

```bash
brew upgrade --cask alan
```

Homebrew-managed app bundles must not be replaced by Sparkle. Alan's app-side
update policy detects app bundles that live under, or resolve through,
Homebrew's cask storage, then directs users back to Homebrew for updates. A
Homebrew-prefix command link is not treated as ownership by itself because
direct app installs can create the same `alan` symlink.

The cask must link the embedded command:

```ruby
app "Alan.app"
binary "#{appdir}/Alan.app/Contents/Resources/bin/alan", target: "alan"
```

Update flow:

1. Run `just release-check` to validate signing and notarization credentials.
   When `.env` includes `ALAN_NOTARY_KEYCHAIN_PROFILE` plus Apple ID
   app-specific password credentials, the check creates or refreshes the
   notary keychain profile automatically.
   Start from `.env.example` and keep the real `.env` ignored.
2. Build the release app with `just release`.
   The script loads allowlisted signing/notarization settings from
   `ALAN_RELEASE_ENV_FILE` when set, otherwise from repo-local release env files
   such as `.env.release.local`, `.env.local`, and `.env`, then
   `~/.alan/release.env`.
3. Upload the generated `alan-<version>-macos.zip` artifact to the GitHub
   Release `v<version>`.
4. Copy the generated SHA-256 checksum from
   `target/release-artifacts/alan-<version>-macos.zip.sha256`.
5. Generate and deploy `appcast.xml` to `alanworks.app`; do not copy the zip
   into Cloudflare Pages.
6. Update `Casks/alan.rb.template` or the tap cask with the version, URL, and
   checksum.
7. Run `./scripts/validate-homebrew-cask.sh`.
