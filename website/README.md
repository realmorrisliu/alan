# Alan Website (`alanworks.app`)

The official website and manual pages for [Alan](https://github.com/thinksmart-systems/alan), designed as a **Plan 9 archive** technical journal issue: flat Klein blue (`#002fa7`) and paper (`#f3f2ec`), the Acme palette (`#ffffea` / `#eaffff` / `#99994c`), Go Sans & Go Mono typography, rio windows + Acme tag bars, man-page structure, troff footnotes, and a Bell System Technical Journal masthead.

## Stack & Family Conventions

Shared across the author's three product sites (`alanworks.app`, `slowpage.app`, `m3.thinksmart.systems`):

- **UI**: Solid 2.0 (`solid-js`, `@solidjs/web`) + `@ui-sdk/solid` (`Dialog`, `Command`) + Vite 8 + Tailwind CSS v4.
- **Prerendering**: `scripts/prerender.mjs` renders `/` and `/docs/*` (`alan(1)`, `alan9(7)`, `agentfs(4)`, `q(1)`, `install(8)`) to static HTML at build time, inlines the built stylesheet, and emits `sitemap.xml`. Interactive islands (`RioDesktop`, `/` command palette) lazy-load Solid on demand so initial page JS stays minimal.
- **Build-time repo data**: `site-data.ts` reads `../CHANGELOG.md` (latest version, issue date, recent releases) and `content/docs/*.md` directly from the repository—nothing is hardcoded or fetched at runtime.
- **Edge Worker**: `worker/index.js` (`cloudflare.config.ts`, `cf` CLI) redirects `www.alanworks.app` → `https://alanworks.app`, serves an ANSI terminal card for `curl` / `wget` / `httpie` on `/`, serves `/install` & `/install.sh`, and proxies `/latest.json` and `/beta.json` release manifests.
- **Anonymous analytics**: Deferred PostHog loader (`public/ph.js`, `src/analytics.ts`) via `https://t.realmorrisliu.com` with `person_profiles: 'identified_only'` and `disable_session_recording: true`. Shared event names across all three sites: `$pageview`, `install_copy`, `download_click`, `github_click`, `scroll_75`.
- **Shared components**: Duplicated per repository rather than cross-repo linked; footer cross-links Slowpage and M3 under *Also by the author*.
- **Easter eggs**: Press `/` for the Acme command palette (`/dev/palette`); pass `?TERM=dumb` (or run `TERM=dumb` in `fig. 1` / palette) for monochrome terminal mode.

## Commands

```bash
pnpm install
pnpm dev         # local Vite + Cloudflare Worker dev server
pnpm typecheck   # tsc --noEmit
pnpm build       # typecheck + client/worker build + SSR build + static prerender
pnpm preview     # preview built Worker + prerendered assets locally
```

## Cloudflare Deployment

### Automatic deploy on `main` (Cloudflare Workers Builds)

`alan-website` is connected directly to **Cloudflare Workers Builds** on `thinksmart-systems/alan` (`main`) — no GitHub deployment secrets are used:

- **Root directory**: `/website`
- **Build command**: `pnpm run build`
- **Deploy command**: `pnpm exec cf deploy --prebuilt --mode production`
- **Watched paths (`path_includes`)**: `website/*`

[`.github/workflows/website.yml`](../.github/workflows/website.yml) runs verification-only builds on pull requests and pushes affecting `website/**` or `CHANGELOG.md`.
