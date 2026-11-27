/**
 * Minimal anonymous analytics, shared by name with slowpage.app and M3:
 * `$pageview` (automatic), `install_copy`, `download_click`, `github_click`
 * and `scroll_75`. The PostHog loader (`/ph.js`) is injected after load, when
 * the main thread is idle, so analytics never competes with first paint.
 */
type PostHog = { capture: (event: string, props?: Record<string, unknown>) => void }

const pending: Array<[string, Record<string, unknown> | undefined]> = []

function ph(): PostHog | undefined {
  return (window as unknown as { posthog?: PostHog }).posthog
}

export function capture(event: string, props?: Record<string, unknown>) {
  const client = ph()
  if (client) client.capture(event, props)
  else pending.push([event, props])
}

function loadPostHog() {
  const s = document.createElement('script')
  s.src = '/ph.js'
  s.async = true
  s.onload = () => {
    const client = ph()
    if (!client) return
    for (const [event, props] of pending.splice(0)) client.capture(event, props)
  }
  document.head.appendChild(s)
}

export function installAnalytics() {
  if (import.meta.env.DEV) return
  const idle = (cb: () => void) =>
    'requestIdleCallback' in window ? requestIdleCallback(cb, { timeout: 4000 }) : setTimeout(cb, 1500)
  if (document.readyState === 'complete') idle(loadPostHog)
  else window.addEventListener('load', () => idle(loadPostHog), { once: true })

  document.addEventListener(
    'click',
    (event) => {
      const el = (event.target as Element | null)?.closest<HTMLElement>('[data-track]')
      if (!el) return
      capture(el.dataset.track!, { href: el.getAttribute('href') ?? undefined })
    },
    { capture: true },
  )

  let sent = false
  const onScroll = () => {
    if (sent) return
    const doc = document.documentElement
    if ((window.scrollY + window.innerHeight) / doc.scrollHeight >= 0.75) {
      sent = true
      capture('scroll_75')
      window.removeEventListener('scroll', onScroll)
    }
  }
  window.addEventListener('scroll', onScroll, { passive: true })
}
