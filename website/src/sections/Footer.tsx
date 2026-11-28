import { For } from 'solid-js'

import { AUTHOR, ESSAYS, REPO, site } from '../site'

/** The page ends the way a manual page does. */
export function Footer() {
  return (
    <footer class="footer" aria-label="Site">
      <div class="wrap">
        <div class="footer__bar" aria-hidden="true">
          <span>ALAN(1)</span>
          <span>Alan System Manual</span>
          <span>ALAN(1)</span>
        </div>
        <div class="footer__grid">
          <section class="man-sec">
            <h2 class="man-sec__name">NAME</h2>
            <p class="man-sec__body">alan — a programmable personal computing environment built on alan9</p>
          </section>
          <section class="man-sec">
            <h2 class="man-sec__name">SEE ALSO</h2>
            <p class="man-sec__body">
              <For each={site.docs}>
                {(doc, i) => (
                  <>
                    <a href={`/docs/${doc.name}`}>
                      {doc.name}({doc.section})
                    </a>
                    {i() < site.docs.length - 1 ? ', ' : ''}
                  </>
                )}
              </For>
              <br />
              <a href={REPO} data-track="github_click">
                github.com/thinksmart-systems/alan
              </a>
              <br />
              <a href={ESSAYS}>Essays on Alan</a>
            </p>
          </section>
          <section class="man-sec">
            <h2 class="man-sec__name">PRIVACY</h2>
            <p class="man-sec__body">
              Anonymous, aggregate PostHog counts only: page views, install copies, GitHub and
              download clicks, and reading depth. No session recording and no person profiles.
            </p>
          </section>
          <section class="man-sec">
            <h2 class="man-sec__name">BUGS</h2>
            <p class="man-sec__body">
              Early development; interfaces change without notice. Report problems with{' '}
              <code>/issue</code> inside Alan, or on <a href={`${REPO}/issues`}>GitHub</a>. This page
              answers to <kbd>/</kbd> and respects <code>TERM=dumb</code>.
            </p>
          </section>
          <section class="man-sec">
            <h2 class="man-sec__name">AUTHOR</h2>
            <p class="man-sec__body">
              <a href={AUTHOR}>Morris Liu</a>. Apache License 2.0.
              <br />
              Also by the author: <a href="https://slowpage.app">Slowpage</a> ·{' '}
              <a href="https://m3.thinksmart.systems">M3</a>
            </p>
          </section>
        </div>
        <div class="footer__bar footer__bar--bottom">
          <span>Alan {site.version}</span>
          <span>{site.issueMonth}</span>
          <span aria-hidden="true">ALAN(1)</span>
        </div>
      </div>
    </footer>
  )
}
