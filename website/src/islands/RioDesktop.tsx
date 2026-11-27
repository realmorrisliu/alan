import { createSignal, For, onCleanup, Show } from 'solid-js'

import { Alan9Shell, type OutputTone } from '../lib/alan9fs'

interface Line {
  cmd?: string
  prompt?: string
  text: string
  tone: OutputTone
}

const SUGGESTIONS = [
  'ps',
  'ls /agent/root',
  'cat /agent/root/machine/tape',
  'echo "hello" > /agent/root/io/input',
  'q install --name my-skills /mnt/import/my-skills',
  'cat /proc/1042/ns',
  'man alan9',
]

/**
 * fig. 1: a small rio desktop over a simulated alan9 namespace. A terminal
 * window runs alan-shell commands; an Acme window browses the same tree, so a
 * write to /agent/root/io/input shows up on the tape you have open.
 * Rendered to static HTML at build time and re-mounted in the browser.
 */
export function RioDesktop() {
  const shell = new Alan9Shell()
  const boot: Line[] = []
  for (const cmd of ['ps', 'ls /agent/root']) {
    const prompt = shell.getPrompt()
    const res = shell.execute(cmd)
    boot.push({ cmd, prompt, text: res.text, tone: res.tone ?? 'default' })
  }

  const [lines, setLines] = createSignal<Line[]>(boot)
  const [prompt, setPrompt] = createSignal(shell.getPrompt())
  const [acmePath, setAcmePath] = createSignal('/agent/root')
  const [revision, setRevision] = createSignal(0)
  const [front, setFront] = createSignal<'term' | 'acme'>('acme')
  shell.onChange(() => setRevision((r) => r + 1))

  let outEl: HTMLDivElement | undefined
  let inputEl: HTMLInputElement | undefined

  const view = () => {
    revision()
    const path = acmePath()
    const node = shell.getNode(path)
    if (node?.type === 'file') return { kind: 'file' as const, text: node.content ?? '' }
    return { kind: 'dir' as const, entries: shell.listDir(path) }
  }

  const open = (name: string) => {
    const base = acmePath()
    const next =
      name === '..' ? shell.resolvePath(`${base}/..`) : shell.resolvePath(`${base}/${name.replace(/\/$/, '')}`)
    setAcmePath(next)
    setFront('acme')
  }

  const run = (cmd: string) => {
    const before = shell.getPrompt()
    const res = shell.execute(cmd)
    if (res.clear) setLines([])
    else setLines((prev) => [...prev, { cmd, prompt: before, text: res.text, tone: res.tone ?? 'default' }])
    setPrompt(shell.getPrompt())
    setFront('term')
    if (res.dumb) window.dispatchEvent(new CustomEvent('alan:dumb'))
    requestAnimationFrame(() => {
      if (outEl) outEl.scrollTop = outEl.scrollHeight
    })
  }

  if (typeof window !== 'undefined') {
    // Commands from the `/` palette; one may be waiting from before this mounted.
    const pending = window as { __alanExec?: string }
    const handle = () => {
      const cmd = pending.__alanExec
      if (!cmd) return
      pending.__alanExec = undefined
      document.getElementById('fig-rio')?.scrollIntoView({ block: 'center' })
      run(cmd)
      inputEl?.focus({ preventScroll: true })
    }
    window.addEventListener('alan:exec', handle)
    onCleanup(() => window.removeEventListener('alan:exec', handle))
    queueMicrotask(handle)
  }

  const onKey = (event: KeyboardEvent) => {
    const input = event.currentTarget as HTMLInputElement
    if (event.key === 'Enter') {
      event.preventDefault()
      if (event.isComposing || event.keyCode === 229) return
      const cmd = input.value.trim()
      input.value = ''
      if (cmd) run(cmd)
    } else if (event.key === 'Tab' && !event.shiftKey) {
      const completed = shell.complete(input.value)
      if (completed) {
        event.preventDefault()
        input.value = completed
      }
    } else if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
      event.preventDefault()
      const value = shell.navigateHistory(event.key === 'ArrowUp' ? -1 : 1)
      if (value !== null) input.value = value
    }
  }

  return (
    <figure id="fig-rio" class="rio" aria-labelledby="fig-rio-cap">
      <div class="rio__desk">
        <div
          class={`rio-win rio__term ${front() === 'term' ? 'is-front' : 'rio-win--inactive'}`}
          onPointerDown={() => setFront('term')}
          onFocusIn={() => setFront('term')}
        >
          <div ref={(el) => (outEl = el)} class="rio__out" role="log" aria-label="alan-shell output">
            <For each={lines()}>
              {(line) => (
                <div class="rio__line">
                  <Show when={line.cmd}>
                    <div>
                      <span class="rio__prompt">{line.prompt}</span> {line.cmd}
                    </div>
                  </Show>
                  <Show when={line.text}>
                    <pre class={`rio__res rio__res--${line.tone}`}>{line.text}</pre>
                  </Show>
                </div>
              )}
            </For>
          </div>
          <div class="rio__inputrow">
            <label for="rio-input" class="rio__prompt">
              {prompt()}
              <span class="sr-only"> alan-shell command</span>
            </label>
            <input
              id="rio-input"
              ref={(el) => (inputEl = el)}
              class="rio__input"
              type="text"
              autocomplete="off"
              autocapitalize="off"
              spellcheck={false}
              placeholder="type help"
              onKeyDown={onKey}
            />
          </div>
        </div>

        <div
          class={`acme rio__acme ${front() === 'acme' ? 'is-front' : 'acme--inactive'}`}
          onPointerDown={() => setFront('acme')}
          onFocusIn={() => setFront('acme')}
        >
          <div class="acme__tag">
            <span class="acme__box" aria-hidden="true" />
            <span class="acme__path">
              {acmePath()}
              {view().kind === 'dir' && acmePath() !== '/' ? '/' : ''}
            </span>
            <button type="button" class="acme__cmd acme__cmd--live" onClick={() => setRevision((r) => r + 1)}>
              Get
            </button>
            <button
              type="button"
              class="acme__cmd acme__cmd--live"
              disabled={acmePath() === '/'}
              onClick={() => open('..')}
            >
              ..
            </button>
            <span class="acme__cmd">| Look</span>
          </div>
          <div class="acme__body">
            <span class="acme__scroll" aria-hidden="true" />
            <div class="acme__text rio__acmetext" aria-live="polite">
              <Show
                when={view().kind === 'dir'}
                fallback={<pre class="rio__file">{(view() as { text: string }).text}</pre>}
              >
                <ul class="rio__ls" aria-label={`Contents of ${acmePath()}`}>
                  <For each={(view() as { entries: string[] }).entries}>
                    {(name) => (
                      <li>
                        <button type="button" class="rio__entry" onClick={() => open(name)}>
                          {name}
                        </button>
                      </li>
                    )}
                  </For>
                </ul>
              </Show>
            </div>
          </div>
        </div>
      </div>
      <div class="rio__try">
        <span class="rio__trylabel">try</span>
        <For each={SUGGESTIONS}>
          {(cmd) => (
            <button type="button" class="rio__chip" onClick={() => run(cmd)}>
              {cmd}
            </button>
          )}
        </For>
      </div>
      <figcaption id="fig-rio-cap" class="rio__cap">
        fig. 1 — rio, with an agent in it. A simulated alan9 namespace that runs in your browser; no
        model is attached. Click a name in the Acme window to open it.
      </figcaption>
    </figure>
  )
}
