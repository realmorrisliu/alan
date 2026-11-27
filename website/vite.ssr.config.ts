import solid from '@solidjs/vite-plugin'
import { defineConfig } from 'vite'

import { siteData } from './site-data.ts'

// Build-time only: compiles the page tree for Node so scripts/prerender.mjs can
// render `/` and `/docs/*` to static HTML. The output in `.ssr/` is never deployed.
export default defineConfig({
  plugins: [solid({ ssr: true }), siteData()],
  resolve: {
    dedupe: ['solid-js', '@solidjs/web', '@ui-sdk/core', '@ui-sdk/solid'],
  },
  build: {
    ssr: 'src/entry-server.tsx',
    outDir: '.ssr',
    emptyOutDir: true,
    target: 'node22',
    copyPublicDir: false,
  },
  ssr: { noExternal: true },
})
