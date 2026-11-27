import { render } from '@solidjs/web'
import { createSignal, For } from 'solid-js'

/** Review-only switch between the hero display-face candidates (?fonts). */
const FACES = [
  { id: 'league', label: 'League Gothic' },
  { id: 'oswald', label: 'Oswald' },
  { id: 'archivo', label: 'Archivo ExtraCondensed' },
]

function FontToggle() {
  const [face, setFace] = createSignal(document.documentElement.dataset.display ?? 'league')
  const pick = (id: string) => {
    setFace(id)
    document.documentElement.dataset.display = id
    const url = new URL(location.href)
    url.searchParams.set('display', id)
    history.replaceState(null, '', url)
  }
  return (
    <fieldset class="fonttoggle">
      <legend>Display face</legend>
      <For each={FACES}>
        {(f) => (
          <label>
            <input type="radio" name="display-face" checked={face() === f.id} onChange={() => pick(f.id)} />
            {f.label}
          </label>
        )}
      </For>
    </fieldset>
  )
}

export function mountFontToggle() {
  const host = document.createElement('div')
  document.body.append(host)
  render(() => <FontToggle />, host)
}
