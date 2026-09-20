# alanworks.app Cloudflare Pages Inputs

This directory contains the static website root and headers for
`alanworks.app`. The retired Alan.app appcast and release archive pipeline is
not a current deployment input. Standalone CLI/Host archives are produced by
`scripts/assemble-cli-release.sh` and are not copied into Cloudflare Pages.

Deploy `index.html` (or the generated website root) together with `_headers`.
