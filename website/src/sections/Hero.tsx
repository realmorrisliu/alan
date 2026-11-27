import { InstallWindow } from '../components/InstallWindow'
import { REPO, site } from '../site'

export function SiteNav(props: { home?: boolean }) {
  const at = (hash: string) => (props.home ? hash : `/${hash}`)
  return (
    <header class="masthead">
      <nav class="nav wrap" aria-label="Primary">
        <a class="nav__link nav__link--hide-sm" href="/docs">
          docs
        </a>
        <a class="nav__link nav__link--hide-sm" href={at('#processes')}>
          how it works
        </a>
        <a class="nav__wordmark" href="/" aria-label="Alan, home">
          Alan
        </a>
        <a class="nav__link nav__link--hide-sm nav__link--end" href={at('#built-on-alan')}>
          built on alan
        </a>
        <span class="nav__end">
          <a class="nav__link nav__link--show-sm" href="/docs">
            docs
          </a>
          <a class="nav__link" href={REPO} data-track="github_click">
            github
          </a>
          <a class="nav__cta" href={at('#install')}>
            install
          </a>
        </span>
      </nav>
      <div class="manhead wrap" aria-hidden="true">
        <span>ALAN(1)</span>
        <span>Alan System Manual</span>
        <span>ALAN(1)</span>
      </div>
    </header>
  )
}

export function Hero() {
  return (
    <section class="hero" aria-labelledby="hero-title">
      <SiteNav home />
      <div class="hero__grid wrap">
        <div class="hero__copy">
          <h1 id="hero-title" class="display hero__title">
            <span>Everything</span> <span>is a file.</span> <span>Even an agent.</span>
          </h1>
          <p class="hero__lede">
            Alan is a programmable personal computing environment built on alan9. Agents run as
            ordinary processes: inspect them with <code>ls</code>, pipe work through them with{' '}
            <code>|</code>, contain them with explicit mounts, in the terminal you already use.
          </p>
          <InstallWindow id="hero-install" />
          <p class="hero__fn">
            Click Snarf<sup>1</sup> to copy. macOS and Linux · Apache-2.0 · v{site.version}
            <br />
            <sup>1</sup> Plan 9 for “copy”. Look shows the script before you run it.
          </p>
        </div>
        <figure class="plate plate--hero">
          <img
            class="plate__img plate__img--screen"
            src="/plates/rabbit-840.webp"
            srcset="/plates/rabbit-560.webp 560w, /plates/rabbit-840.webp 840w, /plates/rabbit-1120.webp 1120w"
            sizes="(min-width: 1024px) 500px, (min-width: 640px) 60vw, 86vw"
            width="560"
            height="702"
            alt="Engraving of a rabbit in headphones typing at a teletype while punched paper tape spills onto the floor."
            fetchpriority="high"
            decoding="async"
          />
          <figcaption class="plate__cap">Plate I. — The operator at the teletype.</figcaption>
        </figure>
      </div>
    </section>
  )
}
