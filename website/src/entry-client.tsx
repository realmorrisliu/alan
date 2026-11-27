import './styles/app.css'
import { capture, installAnalytics } from './analytics'

/* Snarf: Plan 9 for copy. */
document.addEventListener('click', async (event) => {
  const button = (event.target as Element | null)?.closest<HTMLButtonElement>('[data-snarf]')
  if (!button) return
  const status = button.parentElement?.querySelector<HTMLElement>('.snarf-status')
  try {
    await navigator.clipboard.writeText(button.dataset.snarf ?? '')
    button.dataset.snarfed = ''
    if (status) status.textContent = 'snarfed'
  } catch {
    if (status) status.textContent = 'select and copy'
  }
  window.setTimeout(() => {
    delete button.dataset.snarfed
    if (status) status.textContent = ''
  }, 1800)
})

/* TERM=dumb: the monochrome easter egg, from the palette, fig. 1 or ?TERM=dumb. */
function toggleDumb(force?: boolean) {
  const on = document.documentElement.classList.toggle('dumb', force)
  document.querySelector('meta[name="theme-color"]')?.setAttribute('content', on ? '#ffffff' : '#002fa7')
  if (on) capture('term_dumb')
}
window.addEventListener('alan:dumb', () => toggleDumb())
if (new URLSearchParams(location.search).get('TERM') === 'dumb') toggleDumb(true)

/* Islands: static HTML from the prerender, re-mounted with Solid when needed. */
// Solid itself is only fetched with the first island, so the page's initial
// script stays a few kilobytes.
function mountWhenNear(el: HTMLElement, load: (el: HTMLElement) => Promise<unknown>) {
  let done = false
  const go = () => {
    if (done) return
    done = true
    observer?.disconnect()
    load(el)
  }
  const observer =
    'IntersectionObserver' in window
      ? new IntersectionObserver((entries) => entries.some((e) => e.isIntersecting) && go(), {
          rootMargin: '400px 0px',
        })
      : undefined
  if (observer) observer.observe(el)
  else go()
  // A palette command may target the demo before it scrolled into view.
  window.addEventListener('alan:exec-request', go, { once: true })
}

function mountIslands() {
  const rio = document.querySelector<HTMLElement>('[data-island="rio"]')
  if (rio) {
    mountWhenNear(rio, (el) => import('./islands/mount-rio').then((m) => m.mountRio(el)))
  }
}

/* The `/` command palette, loaded on first use. */
window.addEventListener('keydown', (event) => {
  const target = event.target as HTMLElement | null
  const typing =
    target?.tagName === 'INPUT' || target?.tagName === 'TEXTAREA' || target?.isContentEditable
  if (event.key !== '/' || typing || event.metaKey || event.ctrlKey || event.altKey) return
  event.preventDefault()
  import('./islands/Palette').then((m) => m.openPalette())
})

async function boot() {
  const root = document.getElementById('root')
  if (import.meta.env.DEV && root && !root.firstElementChild) {
    // `vite dev` serves the bare template; render the page client-side there.
    const path = location.pathname.replace(/\/$/, '') || '/'
    const { renderDev } = await import('./dev')
    renderDev(root, path)
  } else {
    mountIslands()
  }
  installAnalytics()
}

boot()
