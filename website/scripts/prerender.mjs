/**
 * Static prerender. After `vite build` (client + Worker) and the SSR build of
 * the page tree, render every route to HTML inside the built template, inline
 * the stylesheet, and write the files into the Worker's assets directory.
 */
import fs from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const root = path.resolve(__dirname, '..')
const assetsDir = path.join(root, '.cloudflare/output/v1/workers/default/assets')
const SITE = 'https://alanworks.app'

const { renderAll } = await import(pathToFileURL(path.join(root, '.ssr/entry-server.js')).href)

const ROOT_PLACEHOLDER = '<div id="root"></div>'
const STYLESHEET_RE = /<link\s+rel="stylesheet"[^>]*href="(\/assets\/[^"]+\.css)"[^>]*>/

const escapeAttr = (text) =>
  text.replaceAll('&', '&amp;').replaceAll('"', '&quot;').replaceAll('<', '&lt;')

function replaceRequired(source, pattern, replacement, label) {
  if (!new RegExp(pattern).test(source)) throw new Error(`[prerender] failed to find ${label}`)
  return source.replace(pattern, replacement)
}

const rawHtml = await fs.readFile(path.join(assetsDir, 'index.html'), 'utf8')
if (!rawHtml.includes(ROOT_PLACEHOLDER)) throw new Error('[prerender] template lost #root')
const cssMatch = STYLESHEET_RE.exec(rawHtml)
if (!cssMatch) throw new Error('[prerender] built template has no stylesheet <link>')
const css = (await fs.readFile(path.join(assetsDir, cssMatch[1].slice(1)), 'utf8')).replaceAll(
  '</style>',
  '<\\/style>',
)
const template = rawHtml.replace(cssMatch[0], `<style>${css}</style>`)

const pages = renderAll()
for (const page of pages) {
  const url = page.path === '/' ? `${SITE}/` : `${SITE}${page.path}`
  let html = template
  html = replaceRequired(html, /<title>[^<]*<\/title>/, `<title>${escapeAttr(page.title)}</title>`, 'title')
  html = replaceRequired(
    html,
    /(<meta\s+name="description"\s+content=")[^"]*(")/,
    `$1${escapeAttr(page.description)}$2`,
    'description',
  )
  html = replaceRequired(html, /(<link rel="canonical" href=")[^"]*(")/, `$1${url}$2`, 'canonical')
  html = replaceRequired(html, /(<meta property="og:url" content=")[^"]*(")/, `$1${url}$2`, 'og:url')
  html = replaceRequired(html, /(<meta property="og:title" content=")[^"]*(")/, `$1${escapeAttr(page.title)}$2`, 'og:title')
  if (page.path !== '/') {
    // The display face is only above the fold on the home page.
    html = html.replace(/\s*<link rel="preload" href="\/fonts\/LeagueGothic\.woff2"[^>]*>/, '')
  }
  html = html.replace(ROOT_PLACEHOLDER, `<div id="root">${page.html}</div>`)
  const out = path.join(assetsDir, page.file)
  await fs.mkdir(path.dirname(out), { recursive: true })
  await fs.writeFile(out, html, 'utf8')
}

const today = new Date().toISOString().slice(0, 10)
const sitemap = [
  '<?xml version="1.0" encoding="UTF-8"?>',
  '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">',
  ...pages.map(
    (p) => `  <url><loc>${p.path === '/' ? `${SITE}/` : `${SITE}${p.path}`}</loc><lastmod>${today}</lastmod></url>`,
  ),
  '</urlset>',
  '',
].join('\n')
await fs.writeFile(path.join(assetsDir, 'sitemap.xml'), sitemap, 'utf8')

console.log(`[prerender] inlined ${cssMatch[1]}; wrote ${pages.map((p) => p.file).join(', ')} + sitemap.xml`)
