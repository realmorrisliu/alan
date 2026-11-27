import { cloudflare } from '@cloudflare/vite-plugin'
import solid from '@solidjs/vite-plugin'
import tailwindcss from '@tailwindcss/vite'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vite'

import { siteData } from './site-data.ts'

const __dirname = path.dirname(fileURLToPath(import.meta.url))

export default defineConfig({
  plugins: [solid(), tailwindcss(), siteData(), cloudflare()],
  resolve: {
    dedupe: ['solid-js', '@solidjs/web', '@ui-sdk/core', '@ui-sdk/solid'],
  },
  build: { target: 'es2022' },
  server: {
    fs: { allow: [__dirname, path.resolve(__dirname, '..')] },
  },
})
