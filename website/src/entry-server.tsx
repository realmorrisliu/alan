import { renderToString } from '@solidjs/web'

import { DocsIndex, DocView } from './pages/Docs'
import { Home } from './pages/Home'
import { site } from './site'

export interface RenderedPage {
  path: string
  /** Output file relative to the assets directory. */
  file: string
  title: string
  description: string
  html: string
}

const HOME_DESCRIPTION =
  'Alan is a programmable personal computing environment built on alan9. Agents are ordinary processes, their state is files, and explicit mounts are the boundary.'

/** Hydration markers are useless here: islands re-mount instead of hydrating. */
function clean(html: string) {
  return html.replace(/ _hk=[^\s>]+/g, '').replace(/<!--\/?\$-->|<!--\/-->/g, '')
}

export function renderAll(): RenderedPage[] {
  const pages: RenderedPage[] = [
    {
      path: '/',
      file: 'index.html',
      title: 'Alan — Everything is a file. Even an agent.',
      description: HOME_DESCRIPTION,
      html: clean(renderToString(() => <Home />)),
    },
    {
      path: '/docs',
      file: 'docs.html',
      title: 'Alan System Manual — Alan',
      description: `Manual pages for Alan ${site.version}: ${site.docs.map((d) => `${d.name}(${d.section})`).join(', ')}.`,
      html: clean(renderToString(() => <DocsIndex />)),
    },
  ]
  for (const doc of site.docs) {
    pages.push({
      path: `/docs/${doc.name}`,
      file: `docs/${doc.name}.html`,
      title: `${doc.name}(${doc.section}) — Alan System Manual`,
      description: doc.whatis,
      html: clean(renderToString(() => <DocView doc={doc} />)),
    })
  }
  return pages
}
