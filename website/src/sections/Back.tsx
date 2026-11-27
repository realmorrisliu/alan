import { For } from 'solid-js'

import { AcmeWindow } from '../components/Plan9'
import { InstallWindow } from '../components/InstallWindow'
import { REPO, site } from '../site'

export function PlateII() {
  return (
    <section id="plate-ii" class="insert" aria-labelledby="plate-ii-cap">
      <div class="insert__inner wrap">
        <p class="insert__label" aria-hidden="true">
          Insert
        </p>
        <figure class="plate plate--insert">
          <img
            class="plate__img plate__img--multiply"
            src="/plates/bell-840.webp"
            srcset="/plates/bell-560.webp 560w, /plates/bell-840.webp 840w, /plates/bell-1120.webp 1120w"
            sizes="(min-width: 1024px) 460px, 86vw"
            width="560"
            height="708"
            alt="Engraving of a 1970s minicomputer cabinet with tape reels beside a teletype on a stand, paper tape curling across the floor."
            loading="lazy"
            decoding="async"
          />
          <figcaption id="plate-ii-cap" class="plate__cap">
            Plate II. — The machine room.
          </figcaption>
        </figure>
        <blockquote class="insert__quote">
          <p>Process owns lifecycle and identity.</p>
          <p>Agent Machine owns tape and transition-local state.</p>
          <p>AgentFS owns agent IO, requests, actions, and machine files.</p>
          <p>Memory Stores own continuity across Agent Processes.</p>
          <footer>— from the architecture rules in the Alan repository</footer>
        </blockquote>
      </div>
    </section>
  )
}

const APPS = [
  {
    name: 'Slowpage',
    url: 'https://slowpage.app',
    host: 'slowpage.app',
    what: 'An EPUB and PDF reader for macOS, with book-grade typesetting and notes in the margin.',
    alan: 'Its built-in AI reading companion, @alan, is powered by Alan.',
  },
  {
    name: 'M3',
    url: 'https://m3.thinksmart.systems',
    host: 'm3.thinksmart.systems',
    what: 'A local-first desktop and CLI research instrument for patient investors, built on primary SEC filings.',
    alan: 'Its opt-in deep research runs on built-in Alan, under hard monthly budget caps.',
  },
]

export function BuiltOn() {
  return (
    <section id="built-on-alan" class="apps" aria-labelledby="apps-title">
      <div class="wrap">
        <p class="article__kicker">
          <span>Applications</span>
        </p>
        <h2 id="apps-title" class="display article__title">
          Built on Alan.
        </h2>
        <p class="apps__lede">
          Two applications by the same author carry Alan inside them.
        </p>
        <ul class="apps__list">
          <For each={APPS}>
            {(app) => (
              <li>
                <AcmeWindow
                  path={`/mnt/${app.name.toLowerCase()}/`}
                  label={app.name}
                  tagSlot={
                    <a class="acme__cmd acme__cmd--live" href={app.url} aria-label={`Visit ${app.name} at ${app.host}`}>
                      {app.host}
                    </a>
                  }
                >
                  <div class="app">
                    <h3 class="app__name">{app.name}</h3>
                    <p>{app.what}</p>
                    <p class="app__alan">{app.alan}</p>
                  </div>
                </AcmeWindow>
              </li>
            )}
          </For>
        </ul>
      </div>
    </section>
  )
}

const QUICKSTART = [
  ['curl -fsSL https://alanworks.app/install.sh | sh', 'install the signed binary'],
  ['alan login', 'chatgpt, openrouter, vertex, magpie or openai'],
  ['alan', 'boot a foreground alan9 instance and its TUI'],
  ['git diff | alan', 'one task over stdin, answer on stdout'],
  ['alan update', 'update to the latest release'],
]

const PLATFORMS = [
  ['aarch64-apple-darwin', 'macOS, Apple silicon', 'signed, notarized'],
  ['x86_64-apple-darwin', 'macOS, Intel', 'signed, notarized'],
  ['x86_64-unknown-linux-gnu', 'Linux x86_64, glibc', 'SHA-256 verified'],
  ['aarch64-unknown-linux-gnu', 'Linux arm64, glibc', 'SHA-256 verified'],
]

export function Install() {
  return (
    <section id="install" class="appendix" aria-labelledby="install-title">
      <div class="wrap">
        <p class="article__kicker">
          <span>Appendix</span>
        </p>
        <h2 id="install-title" class="display article__title">
          Installation and changes.
        </h2>
        <div class="appendix__grid">
          <div>
            <InstallWindow id="appendix-install" class="install--wide" />
            <AcmeWindow path="/alanworks.app/README" commands={['Get', '| Look']} class="quickstart">
              <pre class="quickstart__pre">
                <For each={QUICKSTART}>
                  {([cmd, note]) => (
                    <>
                      <span class="quickstart__cmd">{cmd}</span>
                      <span class="quickstart__note"># {note}</span>
                      {'\n'}
                    </>
                  )}
                </For>
              </pre>
            </AcmeWindow>
            <table class="platforms">
              <caption>
                Release targets. <code>install.sh</code> verifies SHA-256 before it unpacks anything
                and installs only the <code>alan</code> binary, into <code>~/.local/bin</code>.
              </caption>
              <thead>
                <tr>
                  <th scope="col">Target</th>
                  <th scope="col">Platform</th>
                  <th scope="col">Integrity</th>
                </tr>
              </thead>
              <tbody>
                <For each={PLATFORMS}>
                  {([target, platform, integrity]) => (
                    <tr>
                      <td>
                        <code>{target}</code>
                      </td>
                      <td>{platform}</td>
                      <td>{integrity}</td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
          <div class="changelog">
            <h3 class="changelog__title">Recent changes</h3>
            <ol class="changelog__list">
              <For each={site.releases}>
                {(release) => (
                  <li class="release">
                    <p class="release__head">
                      <a href={`${REPO}/releases/tag/v${release.version}`} data-track="download_click">
                        v{release.version}
                      </a>
                      <time datetime={release.date}>{release.date}</time>
                    </p>
                    <ul class="release__items">
                      <For each={release.items.slice(0, 2)}>
                        {(item) => (
                          <li>
                            <span class="release__kind">{item.kind}</span> <span innerHTML={item.html} />
                          </li>
                        )}
                      </For>
                    </ul>
                  </li>
                )}
              </For>
            </ol>
            <p class="changelog__more">
              <a href={`${REPO}/blob/main/CHANGELOG.md`}>The full changelog</a> ·{' '}
              <a href={`${REPO}/releases`} data-track="download_click">
                All releases
              </a>
            </p>
          </div>
        </div>
      </div>
    </section>
  )
}
