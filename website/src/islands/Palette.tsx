import { render } from '@solidjs/web'
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandGroupLabel,
  CommandInput,
  CommandItem,
  CommandList,
} from '@ui-sdk/solid/command'
import { Dialog, DialogBackdrop, DialogContent, DialogTitle } from '@ui-sdk/solid/dialog'
import { createSignal, For } from 'solid-js'

import { INSTALL_CMD, REPO } from '../site'

interface Action {
  value: string
  label: string
  hint: string
  run: () => void
}

const onHome = () => location.pathname === '/'
const go = (hash: string) => () => {
  if (onHome()) document.getElementById(hash)?.scrollIntoView()
  else location.href = `/#${hash}`
}
const exec = (cmd: string) => () => {
  if (!onHome()) {
    location.href = '/#processes'
    return
  }
  ;(window as { __alanExec?: string }).__alanExec = cmd
  window.dispatchEvent(new CustomEvent('alan:exec-request'))
  window.dispatchEvent(new CustomEvent('alan:exec'))
}

const GROUPS: Array<{ label: string; actions: Action[] }> = [
  {
    label: 'Go to',
    actions: [
      { value: 'goto-processes', label: ': goto No. 1', hint: 'Agents are processes', run: go('processes') },
      { value: 'goto-files', label: ': goto No. 2', hint: 'State is files', run: go('files') },
      { value: 'goto-boundaries', label: ': goto No. 3', hint: 'Mounts are the boundary', run: go('boundaries') },
      { value: 'goto-apps', label: ': goto built on alan', hint: 'Slowpage, M3', run: go('built-on-alan') },
      { value: 'goto-install', label: ': goto install', hint: 'Installation and changes', run: go('install') },
      { value: 'goto-docs', label: ': open /docs', hint: 'Alan System Manual', run: () => (location.href = '/docs') },
    ],
  },
  {
    label: 'Run in fig. 1',
    actions: [
      { value: 'run-ps', label: '! ps', hint: 'the Process table', run: exec('ps') },
      { value: 'run-ls', label: '! ls /agent/root', hint: 'an Agent Process, as files', run: exec('ls /agent/root') },
      { value: 'run-tape', label: '! cat /agent/root/machine/tape', hint: 'its tape', run: exec('cat /agent/root/machine/tape') },
      {
        value: 'run-write',
        label: '! echo "hello" > /agent/root/io/input',
        hint: 'give it a task',
        run: exec('echo "hello" > /agent/root/io/input'),
      },
      { value: 'run-man', label: '! man alan9', hint: 'the Standard Namespace', run: exec('man alan9') },
    ],
  },
  {
    label: 'Other',
    actions: [
      {
        value: 'dumb',
        label: ': TERM=dumb',
        hint: 'toggle the monochrome terminal',
        run: () => window.dispatchEvent(new CustomEvent('alan:dumb')),
      },
      {
        value: 'snarf',
        label: ': snarf install',
        hint: 'copy the install command',
        run: () => void navigator.clipboard?.writeText(INSTALL_CMD),
      },
      { value: 'github', label: ': open github', hint: 'thinksmart-systems/alan', run: () => (location.href = REPO) },
    ],
  },
]

const ALL = new Map(GROUPS.flatMap((g) => g.actions).map((a) => [a.value, a]))
const [open, setOpen] = createSignal(false)
let mounted = false

function Palette() {
  return (
    <Dialog open={open()} onOpenChange={(next) => setOpen(next)}>
      <DialogBackdrop class="palette__backdrop" />
      <DialogContent class="palette acme">
        <div class="acme__tag">
          <span class="acme__box" aria-hidden="true" />
          <DialogTitle class="acme__path palette__title">/dev/palette</DialogTitle>
          <span class="acme__cmd">Esc to close</span>
        </div>
        <Command
          class="palette__cmd"
          onAction={(value: string) => {
            setOpen(false)
            ALL.get(value)?.run()
          }}
        >
          <div class="palette__row">
            <span aria-hidden="true">:</span>
            <CommandInput class="palette__input" aria-label="Search commands" placeholder="goto, run, TERM=dumb…" />
          </div>
          <CommandEmpty class="palette__empty">No match.</CommandEmpty>
          <CommandList class="palette__list">
            <For each={GROUPS}>
              {(group) => (
                <CommandGroup class="palette__group">
                  <CommandGroupLabel class="palette__glabel">{group.label}</CommandGroupLabel>
                  <For each={group.actions}>
                    {(action) => (
                      <CommandItem class="palette__item" value={action.value} keywords={[action.hint]}>
                        <span>{action.label}</span>
                        <span class="palette__hint">{action.hint}</span>
                      </CommandItem>
                    )}
                  </For>
                </CommandGroup>
              )}
            </For>
          </CommandList>
        </Command>
      </DialogContent>
    </Dialog>
  )
}

export function openPalette() {
  if (!mounted) {
    mounted = true
    const host = document.createElement('div')
    document.body.append(host)
    render(() => <Palette />, host)
  }
  setOpen(true)
}
