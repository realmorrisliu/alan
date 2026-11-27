import { render } from '@solidjs/web'

import { RioDesktop } from './RioDesktop'

/** Replace the prerendered fig. 1 with the live one (identical markup, no shift). */
export function mountRio(el: HTMLElement) {
  el.replaceChildren()
  render(() => <RioDesktop />, el)
}
