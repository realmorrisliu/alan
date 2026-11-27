import { For, Show } from 'solid-js'

import type { DocPage } from '../../site-data'
import { Footer } from '../sections/Footer'
import { SiteNav } from '../sections/Hero'
import { site } from '../site'

function DocsShell(props: { title: string; current?: string; children: any }) {
  return (
    <>
      <a class="skip" href="#doc">
        Skip to content
      </a>
      <div class="docs-top">
        <SiteNav />
      </div>
      <main id="doc" class="docs wrap">
        <nav class="docs__nav" aria-label="Manual pages">
          <p class="docs__navlabel">
            <a href="/docs">Alan System Manual</a>
          </p>
          <ul>
            <For each={site.docs}>
              {(doc) => (
                <li>
                  <a href={`/docs/${doc.name}`} aria-current={props.current === doc.name ? 'page' : undefined}>
                    {doc.name}({doc.section})
                  </a>
                </li>
              )}
            </For>
          </ul>
        </nav>
        <div class="docs__page">
          <div class="docs__bar" aria-hidden="true">
            <span>{props.title}</span>
            <span>Alan System Manual</span>
            <span>{props.title}</span>
          </div>
          {props.children}
        </div>
      </main>
      <Footer />
    </>
  )
}

export function DocsIndex() {
  return (
    <DocsShell title="INTRO(0)">
      <h1 class="display docs__title">Alan System Manual</h1>
      <p class="docs__intro">
        Five pages for Alan {site.version}, kept in the Alan repository and set as manual pages.
        Inside a terminal, <code>alan man</code> opens the built-in manual.
      </p>
      <dl class="whatis">
        <For each={site.docs}>
          {(doc) => (
            <>
              <dt>
                <a href={`/docs/${doc.name}`}>
                  {doc.name}({doc.section})
                </a>
              </dt>
              <dd>{doc.whatis.replace(/^[^-]+-\s*/, '')}</dd>
            </>
          )}
        </For>
      </dl>
    </DocsShell>
  )
}

export function DocView(props: { doc: DocPage }) {
  const index = site.docs.findIndex((d) => d.name === props.doc.name)
  const next = site.docs[index + 1]
  return (
    <DocsShell title={props.doc.title} current={props.doc.name}>
      <h1 class="sr-only">{props.doc.title}</h1>
      <article class="manpage" innerHTML={props.doc.html} />
      <Show when={next}>
        <p class="docs__next">
          Next: <a href={`/docs/${next!.name}`}>{next!.name}({next!.section})</a> — {next!.whatis.replace(/^[^-]+-\s*/, '')}
        </p>
      </Show>
    </DocsShell>
  )
}
