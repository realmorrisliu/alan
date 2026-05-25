# PR Notes

- Direct `Alan.app` installs use Sparkle 2 and the stable feed at
  `https://alanworks.app/appcast.xml`.
- GitHub Releases remain the owner for `alan-<version>-macos.zip` and checksum
  assets. The appcast references those assets; it does not move archives to
  Cloudflare.
- Cloudflare Pages owns the website root and `appcast.xml` with XML content type
  and low-cache headers.
- Homebrew cask installs are Homebrew-managed. Alan detects app bundles that
  live under or resolve through Homebrew cask storage and directs users to
  `brew upgrade --cask alan` instead of letting Sparkle replace the app bundle.
  A Homebrew-prefix command link alone is not treated as app ownership because
  direct installs can create the same link.

Remaining live-release closure:

- `just release-check` currently stops because this machine has no
  `ALAN_DEVELOPER_ID_APPLICATION` configured.
- `https://alanworks.app/appcast.xml` is not reachable from this machine yet;
  the Cloudflare Pages deploy remains a release operation.
- The existing GitHub `v0.1.0` release is a draft with no assets, so old-to-new
  Sparkle install verification still needs a signed/notarized old build and a
  deployed newer appcast.
