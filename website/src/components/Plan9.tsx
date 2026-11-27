import type { JSX } from '@solidjs/web'
import { For, Show } from 'solid-js'

/**
 * Plan 9 window chrome, drawn flat: a rio window is a bordered rectangle; an
 * Acme window adds a pale-cyan tag line (path + commands) over a pale-yellow
 * body with a khaki scroll bar on the left.
 */
export function AcmeWindow(props: {
  path: string
  /** Words in the tag after the path; each is plain text unless rendered by `tagSlot`. */
  commands?: string[]
  tagSlot?: JSX.Element
  class?: string
  label?: string
  children: JSX.Element
}) {
  return (
    <div class={`acme ${props.class ?? ''}`} role={props.label ? 'group' : undefined} aria-label={props.label}>
      <div class="acme__tag">
        <span class="acme__box" aria-hidden="true" />
        <span class="acme__path">{props.path}</span>
        <Show when={props.tagSlot} fallback={
          <For each={props.commands ?? []}>{(word) => <span class="acme__cmd">{word}</span>}</For>
        }>
          {props.tagSlot}
        </Show>
      </div>
      <div class="acme__body">
        <span class="acme__scroll" aria-hidden="true" />
        <div class="acme__text">{props.children}</div>
      </div>
    </div>
  )
}

export function RioWindow(props: {
  class?: string
  inactive?: boolean
  label?: string
  children: JSX.Element
}) {
  return (
    <div
      class={`rio-win ${props.inactive ? 'rio-win--inactive' : ''} ${props.class ?? ''}`}
      role={props.label ? 'group' : undefined}
      aria-label={props.label}
    >
      {props.children}
    </div>
  )
}

/** A man-page section: monospaced uppercase heading, indented body. */
export function ManSection(props: { name: string; children: JSX.Element }) {
  return (
    <section class="man-sec">
      <h3 class="man-sec__name">{props.name}</h3>
      <div class="man-sec__body">{props.children}</div>
    </section>
  )
}

/** troff-style footnote reference. */
export function Fn(props: { n: number }) {
  return (
    <sup class="fnref">
      <a href={`#fn-${props.n}`} id={`fnref-${props.n}`} aria-label={`Footnote ${props.n}`}>
        {props.n}
      </a>
    </sup>
  )
}

export function Footnotes(props: { notes: Array<{ n: number; body: JSX.Element }> }) {
  return (
    <ol class="footnotes">
      <For each={props.notes}>
        {(note) => (
          <li id={`fn-${note.n}`} value={note.n}>
            <sup aria-hidden="true">{note.n}</sup> {note.body}{' '}
            <a href={`#fnref-${note.n}`} class="footnotes__back" aria-label={`Back to reference ${note.n}`}>
              ↩
            </a>
          </li>
        )}
      </For>
    </ol>
  )
}

/** Section opener in the journal: "No. 1 — Processes" kicker plus display heading. */
export function Article(props: {
  id: string
  no: number
  kicker: string
  title: JSX.Element
  class?: string
  children: JSX.Element
}) {
  return (
    <article id={props.id} class={`article ${props.class ?? ''}`} aria-labelledby={`${props.id}-title`}>
      <p class="article__kicker">
        <span>No. {props.no}</span>
        <span aria-hidden="true">—</span>
        <span>{props.kicker}</span>
      </p>
      <h2 id={`${props.id}-title`} class="display article__title">
        {props.title}
      </h2>
      {props.children}
    </article>
  )
}
