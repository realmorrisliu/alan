/**
 * Build-time site data, exposed to the page tree as `virtual:site-data`.
 *
 * Everything the page says about releases and manual pages is read from the
 * alan repository itself: `../CHANGELOG.md` for the version, masthead date and
 * the changelog excerpt, and `content/docs/*.md` for the man pages under
 * `/docs`. Nothing here is fetched at runtime.
 */
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { Marked } from 'marked'
import type { Plugin } from 'vite'

const here = path.dirname(fileURLToPath(import.meta.url))
const CHANGELOG = path.resolve(here, '../CHANGELOG.md')
const DOCS_DIR = path.resolve(here, 'content/docs')
const REPO = 'https://github.com/thinksmart-systems/alan'

export interface ReleaseItem {
  kind: string
  html: string
}
export interface Release {
  version: string
  date: string
  items: ReleaseItem[]
}
export interface DocPage {
  name: string
  section: string
  title: string
  whatis: string
  order: number
  html: string
}
export interface SiteData {
  version: string
  releaseDate: string
  issueMonth: string
  releases: Release[]
  docs: DocPage[]
}

function escapeHtml(text: string): string {
  return text.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;')
}

/** Inline markdown for changelog bullets: code spans and PR references only. */
function inline(text: string): string {
  return escapeHtml(text)
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\(#(\d+)\)/g, `(<a href="${REPO}/pull/$1">#$1</a>)`)
}

/** Shorten a long changelog bullet at a word boundary, keeping PR refs. */
function shorten(text: string, max = 168): string {
  const refs = text.match(/\(#\d+\)\.?$/)?.[0]?.replace(/\.$/, '') ?? ''
  let body = text.replace(/\s*\(#\d+\)\.?$/, '').replace(/\.$/, '')
  if (body.length > max) {
    let cut = body.slice(0, max)
    // never end inside a code span
    if ((cut.match(/`/g) ?? []).length % 2 === 1) cut = cut.slice(0, cut.lastIndexOf('`'))
    body = cut.slice(0, cut.lastIndexOf(' ')).replace(/[,;:(\s]+$/, '') + '…'
  }
  return refs ? `${body} ${refs}` : body
}

function parseChangelog(source: string): Release[] {
  const releases: Release[] = []
  let current: Release | undefined
  let kind = ''
  for (const line of source.split('\n')) {
    const head = /^## \[(\d+\.\d+\.\d+)\] - (\d{4}-\d{2}-\d{2})/.exec(line)
    if (head) {
      current = { version: head[1], date: head[2], items: [] }
      releases.push(current)
      continue
    }
    if (/^## /.test(line)) {
      current = undefined
      continue
    }
    const sub = /^### (\w+)/.exec(line)
    if (sub) {
      kind = sub[1]
      continue
    }
    const bullet = /^- (.+)$/.exec(line)
    if (bullet && current) current.items.push({ kind, html: inline(shorten(bullet[1].trim())) })
  }
  return releases
}

function parseFrontmatter(source: string): { meta: Record<string, string>; body: string } {
  const match = /^---\n([\s\S]*?)\n---\n?/.exec(source)
  if (!match) throw new Error('[site-data] doc page without frontmatter')
  const meta: Record<string, string> = {}
  for (const line of match[1].split('\n')) {
    const kv = /^(\w+):\s*(.*)$/.exec(line)
    if (kv) meta[kv[1]] = kv[2].replace(/^"(.*)"$/, '$1')
  }
  return { meta, body: source.slice(match[0].length) }
}

function loadDocs(): DocPage[] {
  const marked = new Marked({ gfm: true })
  return fs
    .readdirSync(DOCS_DIR)
    .filter((file) => file.endsWith('.md'))
    .map((file) => {
      const { meta, body } = parseFrontmatter(fs.readFileSync(path.join(DOCS_DIR, file), 'utf8'))
      const html = (marked.parse(body) as string)
        // man-page cross references, e.g. alan9(7), become links to sibling pages
        .replace(/\b(alan|alan9|agentfs|q|install)\((\d)\)(?![^<]*<\/code>)/g, '<a href="/docs/$1">$1($2)</a>')
        // `--flag`\n: description  ->  a definition list, as in an OPTIONS section
        .replace(/<p>([\s\S]*?)<\/p>/g, (full, inner: string) => {
          const opt = /^(<code>[^\n]*)\n: ([\s\S]*)$/.exec(inner)
          return opt ? `<dl class="opt"><dt>${opt[1]}</dt><dd>${opt[2]}</dd></dl>` : full
        })
      return {
        name: meta.name,
        section: meta.section,
        title: meta.title,
        whatis: meta.whatis,
        order: Number(meta.order ?? 99),
        html,
      }
    })
    .sort((a, b) => a.order - b.order)
}

export function loadSiteData(): SiteData {
  const releases = parseChangelog(fs.readFileSync(CHANGELOG, 'utf8').replaceAll('\r\n', '\n'))
  const latest = releases[0]
  if (!latest) throw new Error('[site-data] no released version in CHANGELOG.md')
  const issueMonth = new Date(`${latest.date}T00:00:00Z`).toLocaleDateString('en-US', {
    month: 'long',
    year: 'numeric',
    timeZone: 'UTC',
  })
  return {
    version: latest.version,
    releaseDate: latest.date,
    issueMonth,
    releases: releases.slice(0, 5),
    docs: loadDocs(),
  }
}

const VIRTUAL_ID = 'virtual:site-data'
const RESOLVED_ID = '\0' + VIRTUAL_ID

export function siteData(): Plugin {
  return {
    name: 'alan-site-data',
    resolveId(id) {
      return id === VIRTUAL_ID ? RESOLVED_ID : undefined
    },
    load(id) {
      if (id !== RESOLVED_ID) return undefined
      this.addWatchFile(CHANGELOG)
      for (const file of fs.readdirSync(DOCS_DIR)) this.addWatchFile(path.join(DOCS_DIR, file))
      return `export default ${JSON.stringify(loadSiteData())}`
    },
  }
}
