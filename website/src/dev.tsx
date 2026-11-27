import { render } from '@solidjs/web'

import { DocsIndex, DocView } from './pages/Docs'
import { Home } from './pages/Home'
import { site } from './site'

/** `vite dev` only: the template has no prerendered page, so render it here. */
export function renderDev(root: HTMLElement, path: string) {
  const doc = site.docs.find((d) => `/docs/${d.name}` === path)
  render(() => (path === '/docs' ? <DocsIndex /> : doc ? <DocView doc={doc} /> : <Home />), root)
}
