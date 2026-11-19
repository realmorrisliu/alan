import './styles/tokens.css';
import './styles/main.css';
import './styles/dumb.css';

import { Alan9Shell, type OutputTone } from './alan9fs';
import { ANATOMY_NODES } from './anatomy';

type ThemeMode = 'paper' | 'midnight' | 'dumb';

const STORAGE_KEY = 'alan-theme';
const shell = new Alan9Shell();

/* ---------- Theme Controller (View Transitions + <meta color-scheme> sync) ---------- */
function readStoredTheme(): ThemeMode {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === 'paper' || v === 'midnight' || v === 'dumb') return v;
  } catch {
    // Ignore storage access errors in restricted browsing contexts
  }
  return 'paper';
}

function applyThemeDom(mode: ThemeMode): void {
  const root = document.documentElement;
  root.dataset.theme = mode;

  const meta = document.querySelector<HTMLMetaElement>('meta[name="color-scheme"]');
  if (meta) {
    meta.content = mode === 'midnight' ? 'dark' : 'light';
  }

  document.querySelectorAll<HTMLButtonElement>('[data-theme-btn]').forEach((btn) => {
    btn.setAttribute('aria-pressed', String(btn.dataset.themeBtn === mode));
  });

  if (mode === 'dumb') {
    setProcStatus('TERM=dumb', 'dumb');
  } else {
    setProcStatus(mode === 'midnight' ? 'midnight' : activeSectionLabel, 'run');
  }
}

/* ---------- Live /proc/0001 Status Pill ---------- */
let procResetTimer: number | undefined;
let activeSectionLabel = 'idle';

function setProcStatus(label: string, state: 'run' | 'exploring' | 'dumb' = 'exploring'): void {
  const pill = document.getElementById('proc-pill');
  const text = document.getElementById('proc-pill-text');
  if (!pill || !text) return;
  pill.dataset.state = state;
  text.textContent = `/proc/0001  ${label}`;

  if (state === 'exploring') {
    window.clearTimeout(procResetTimer);
    procResetTimer = window.setTimeout(() => {
      if (document.documentElement.dataset.theme !== 'dumb') {
        pill.dataset.state = 'run';
        text.textContent = `/proc/0001  ${activeSectionLabel}`;
      }
    }, 2400);
  }
}

function setTheme(mode: ThemeMode, persist = true): void {
  const update = () => applyThemeDom(mode);

  if (document.startViewTransition) {
    document.startViewTransition(update);
  } else {
    update();
  }

  if (persist) {
    try {
      localStorage.setItem(STORAGE_KEY, mode);
    } catch {
      // Theme still applied even if storage is unavailable
    }
  }
}

/* ---------- Floating Non-Blocking Acme Mini-Shell Drawer ---------- */
function showMiniDrawer(command: string, text: string, tone: OutputTone): void {
  const drawer = document.getElementById('acme-drawer');
  const cmdEl = document.getElementById('acme-drawer-cmd');
  const bodyEl = document.getElementById('acme-drawer-body');
  if (!drawer || !cmdEl || !bodyEl) return;

  cmdEl.textContent = `${shell.getPrompt()} ${command}`;
  bodyEl.className = `acme-drawer__body${tone !== 'default' ? ` term-out--${tone}` : ''}`;
  bodyEl.textContent = text || '(ok)';
  drawer.inert = false;
  drawer.dataset.open = 'true';
}

function closeMiniDrawer(): void {
  const drawer = document.getElementById('acme-drawer');
  if (!drawer) return;
  drawer.dataset.open = 'false';
  drawer.inert = true;
}

function isHeroTerminalInView(): boolean {
  const el = document.getElementById('hero-terminal');
  if (!el) return false;
  const r = el.getBoundingClientRect();
  return r.bottom > 120 && r.top < window.innerHeight - 120;
}

/* ---------- Syntax-Inked Terminal Renderer ---------- */
function renderInkedText(container: HTMLElement, text: string): void {
  const tokenRegex = /(\[in\]|\[out\]|\[tool\]|\[checkpoint\]|\[transition\]|\b[a-z0-9_-]+\/)/g;
  let lastIdx = 0;
  let match: RegExpExecArray | null;

  while ((match = tokenRegex.exec(text)) !== null) {
    if (match.index > lastIdx) {
      container.append(document.createTextNode(text.slice(lastIdx, match.index)));
    }
    const tok = match[0];
    const span = document.createElement('span');
    span.textContent = tok;
    if (tok === '[in]' || tok === '[out]' || tok.endsWith('/')) {
      span.className = 'tok-klein';
    } else if (tok === '[tool]') {
      span.className = 'tok-tool';
    } else if (tok === '[checkpoint]') {
      span.className = 'tok-ckpt';
    } else {
      span.className = 'tok-muted';
    }
    container.append(span);
    lastIdx = match.index + tok.length;
  }

  if (lastIdx < text.length) {
    container.append(document.createTextNode(text.slice(lastIdx)));
  }
}

/* ---------- Interactive Rio / alan-shell Terminal ---------- */
function appendTerminalEntry(cmd: string, text: string, tone: OutputTone = 'default'): void {
  const outEl = document.getElementById('term-output');
  if (!outEl) return;

  const wrap = document.createElement('div');
  wrap.className = 'term-line';

  if (cmd) {
    const cmdLine = document.createElement('div');
    cmdLine.className = 'term-cmd';
    const mark = document.createElement('span');
    mark.className = 'term-prompt-mark';
    mark.textContent = shell.getPrompt();
    cmdLine.append(mark, document.createTextNode(cmd));
    wrap.append(cmdLine);
  }

  if (text) {
    const outLine = document.createElement('div');
    outLine.className = `term-out${tone !== 'default' ? ` term-out--${tone}` : ''}`;
    if (tone === 'default') {
      renderInkedText(outLine, text);
    } else {
      outLine.textContent = text;
    }
    wrap.append(outLine);
  }

  outEl.append(wrap);
  outEl.scrollTop = outEl.scrollHeight;
}

function runShellCommand(command: string, preferDrawerWhenScrolled = false): void {
  const promptEl = document.getElementById('term-prompt');
  const outEl = document.getElementById('term-output');
  const res = shell.execute(command);

  if (res.clear && outEl) {
    outEl.innerHTML = '';
  } else {
    appendTerminalEntry(command, res.text, res.tone ?? 'default');
  }

  if (promptEl) {
    promptEl.textContent = shell.getPrompt();
  }

  if (res.theme) {
    setTheme(res.theme);
  } else if (res.procState) {
    setProcStatus(res.procState, 'exploring');
  }

  if (preferDrawerWhenScrolled && !res.clear) {
    if (isHeroTerminalInView()) {
      closeMiniDrawer();
      document.getElementById('term-input')?.focus();
    } else {
      showMiniDrawer(command, res.text, res.tone ?? 'default');
    }
  }
}

function initTerminal(): void {
  const form = document.getElementById('term-form') as HTMLFormElement | null;
  const input = document.getElementById('term-input') as HTMLInputElement | null;
  if (!form || !input) return;

  // Quiet, balanced initial transcript: namespace root + Turing tape
  appendTerminalEntry('ls /', 'proc/   agent/   srv/   bin/   lib/   man/   mnt/', 'default');
  appendTerminalEntry(
    'cat /agent/root/machine/tape',
    shell.readFile('/agent/root/machine/tape') ?? '',
    'default',
  );

  // IME-safe Enter handling + Tab autocomplete + Up/Down history
  input.addEventListener('keydown', (event) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      if (event.isComposing || event.keyCode === 229) return;
      form.requestSubmit();
      return;
    }
    if (event.key === 'Tab' && !event.shiftKey) {
      const completed = shell.complete(input.value);
      if (completed) {
        event.preventDefault();
        input.value = completed;
      }
      return;
    }
    if (event.key === 'Escape') {
      input.blur();
      return;
    }
    if (event.key === 'ArrowUp') {
      event.preventDefault();
      const prev = shell.navigateHistory(-1);
      if (prev !== null) input.value = prev;
    } else if (event.key === 'ArrowDown') {
      event.preventDefault();
      const next = shell.navigateHistory(1);
      if (next !== null) input.value = next;
    }
  });

  form.addEventListener('submit', (event) => {
    event.preventDefault();
    const cmd = input.value.trim();
    if (!cmd) return;
    input.value = '';
    runShellCommand(cmd);
  });

  // Preset chips
  document.querySelectorAll<HTMLButtonElement>('[data-chip-cmd]').forEach((btn) => {
    btn.addEventListener('click', () => {
      const cmd = btn.dataset.chipCmd;
      if (cmd) runShellCommand(cmd);
    });
  });

  // Acme-style clickable paths/commands anywhere in prose ("Text is UI")
  document.querySelectorAll<HTMLButtonElement>('[data-exec]').forEach((btn) => {
    btn.addEventListener('click', () => {
      const cmd = btn.dataset.exec;
      if (cmd) runShellCommand(cmd, true);
    });
  });

  document.getElementById('acme-drawer-close')?.addEventListener('click', closeMiniDrawer);
  document.getElementById('acme-drawer-jump')?.addEventListener('click', () => {
    closeMiniDrawer();
    document.getElementById('hero-terminal')?.scrollIntoView({ behavior: 'smooth', block: 'center' });
    document.getElementById('term-input')?.focus();
  });
}

/* ---------- Interactive Execution Strip + Live alan9 Sync + Sliding Tape Head ---------- */
function initAnatomy(): void {
  const buttons = document.querySelectorAll<HTMLButtonElement>('[data-anatomy-id]');
  const tapeCells = document.querySelectorAll<HTMLButtonElement>('[data-tape-idx]');
  const headPointer = document.getElementById('tape-head-pointer');
  const ownerEl = document.getElementById('inspector-owner');
  const codeEl = document.getElementById('inspector-code');
  const execBtn = document.getElementById('inspector-exec') as HTMLButtonElement | null;

  let currentId = 'machine';
  let currentTapeIdx = 3;

  const getLiveExcerpt = (id: string, fallback: string): string => {
    if (id === 'machine') {
      return shell.readFile('/agent/root/machine/tape') ?? fallback;
    }
    if (id === 'process') {
      return shell.readFile('/proc/1042/status') ?? fallback;
    }
    if (id === 'agentfs') {
      const status = shell.readFile('/agent/root/status') ?? '';
      return `/agent/root/\n├── in           # write task input\n├── out          # ${shell.readFile('/agent/root/out') ?? ''}\n├── requests/    # pending permission or input requests\n├── actions/     # tool spawn audit log\n└── machine/     # tape, state, checkpoints\n\n# /agent/root/status\n${status}`;
    }
    return fallback;
  };

  const renderCurrent = () => {
    const data = ANATOMY_NODES.find((n) => n.id === currentId) ?? ANATOMY_NODES[0];
    buttons.forEach((b) => b.setAttribute('aria-selected', String(b.dataset.anatomyId === data.id)));
    tapeCells.forEach((c) => {
      c.classList.toggle('tape-cell--active', Number(c.dataset.tapeIdx) === currentTapeIdx);
    });
    if (headPointer) {
      headPointer.style.setProperty('--head-idx', String(currentTapeIdx));
    }
    if (ownerEl) ownerEl.textContent = data.owner;
    if (codeEl) codeEl.textContent = getLiveExcerpt(data.id, data.excerpt);
    if (execBtn) {
      execBtn.dataset.exec = data.execCmd;
      execBtn.textContent = `Run \`${data.execCmd}\` in alan-shell →`;
    }
  };

  buttons.forEach((btn) => {
    btn.addEventListener('click', () => {
      const id = btn.dataset.anatomyId;
      if (!id) return;
      const data = ANATOMY_NODES.find((n) => n.id === id);
      currentId = id;
      currentTapeIdx = data?.tapeIndex ?? 0;
      renderCurrent();
    });
  });

  tapeCells.forEach((cell) => {
    cell.addEventListener('click', () => {
      currentTapeIdx = Number(cell.dataset.tapeIdx ?? 3);
      currentId = cell.dataset.tapeNode ?? 'machine';
      renderCurrent();
    });
  });

  shell.onChange(renderCurrent);
  renderCurrent();
}

/* ---------- Scrollspy & Live /proc/0001 Reading State ---------- */
function initScrollspy(): void {
  const navLinks = Array.from(document.querySelectorAll<HTMLAnchorElement>('.topbar__nav a'));
  const targets = ['top', 'architecture', 'namespace', 'cli', 'manpage']
    .map((id) => document.getElementById(id))
    .filter((el): el is HTMLElement => el !== null);

  if (!('IntersectionObserver' in window) || targets.length === 0) return;

  const labelMap: Record<string, string> = {
    top: 'idle',
    architecture: 'reading /architecture',
    namespace: 'reading /namespace',
    cli: 'reading /cli',
    manpage: 'reading /man',
  };

  const observer = new IntersectionObserver(
    (entries) => {
      const visible = entries
        .filter((e) => e.isIntersecting)
        .sort((a, b) => b.intersectionRatio - a.intersectionRatio)[0];
      if (!visible) return;
      const id = visible.target.id;
      activeSectionLabel = labelMap[id] ?? 'idle';

      navLinks.forEach((a) => {
        if (a.getAttribute('href')?.slice(1) === id) {
          a.setAttribute('aria-current', 'true');
        } else {
          a.removeAttribute('aria-current');
        }
      });

      if (document.documentElement.dataset.theme !== 'dumb') {
        const pill = document.getElementById('proc-pill');
        const text = document.getElementById('proc-pill-text');
        if (pill && text && pill.dataset.state !== 'exploring') {
          text.textContent = `/proc/0001  ${activeSectionLabel}`;
        }
      }
    },
    { rootMargin: '-18% 0px -55% 0px', threshold: [0.1, 0.4] },
  );

  targets.forEach((t) => observer.observe(t));
}

/* ---------- CLI Mode Switcher ---------- */
const CLI_VIEWS = {
  interactive: {
    screen: `$ alan

  project: /mnt/alan   model: chatgpt-main   state: idle
: summarize recent changes in crates/kernel

  ● tool  read_file crates/kernel/src/mountfs/mod.rs   [Ctrl+O details]

  Mount table enforces lock discipline across async select! arms.
  Namespace bindings remain process-local under /proc/<pid>/ns.

  project: /mnt/alan   model: chatgpt-main   state: idle
! cargo test -p alan-kernel --quiet
  ok. 42 passed; 0 failed (0.18s)`,
    caption:
      'Bare `alan` boots one foreground alan9 instance and attaches the file-backed terminal renderer to `/agent/root`, with progressive disclosure on `Ctrl+O`.',
  },
  pipe: {
    screen: `$ git diff HEAD~1 | alan

The diff enforces zero installation-time authority before projecting
Quartermaster packages (skill, mcp, tool, provider) into /lib/pkg/<id>.

No raw Host paths cross the /bin/q boundary.

$ echo $?
0`,
    caption:
      'Redirected stdin (or `alan -p --ephemeral --json`) submits one bounded task, streams the answer or NDJSON events to stdout, and returns the real exit status.',
  },
} as const;

function renderInlineCode(parent: HTMLElement, text: string): void {
  const parts = text.split(/(`[^`]+`)/g);
  for (const part of parts) {
    if (part.startsWith('`') && part.endsWith('`') && part.length > 2) {
      const code = document.createElement('code');
      code.textContent = part.slice(1, -1);
      parent.append(code);
    } else if (part) {
      parent.append(document.createTextNode(part));
    }
  }
}

function initCliSwitcher(): void {
  const tabs = document.querySelectorAll<HTMLButtonElement>('[data-cli-tab]');
  const screen = document.getElementById('cli-screen');
  const caption = document.getElementById('cli-caption');

  const show = (mode: 'interactive' | 'pipe') => {
    tabs.forEach((t) => t.setAttribute('aria-selected', String(t.dataset.cliTab === mode)));
    const data = CLI_VIEWS[mode];
    if (screen) screen.textContent = data.screen;
    if (caption) {
      caption.innerHTML = '';
      renderInlineCode(caption, data.caption);
    }
  };

  tabs.forEach((t) => {
    t.addEventListener('click', () => {
      const mode = t.dataset.cliTab as 'interactive' | 'pipe';
      if (mode) show(mode);
    });
  });

  show('interactive');
}

/* ---------- Native <dialog> Modals (/ Command Palette & ? Shortcuts) ---------- */
function setupDialogFallbacks(): void {
  if (!('closedBy' in HTMLDialogElement.prototype)) {
    const outside = (e: PointerEvent, dialog: HTMLDialogElement) => {
      const r = dialog.getBoundingClientRect();
      return e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom;
    };
    document.querySelectorAll<HTMLDialogElement>('dialog[closedby="any"]').forEach((dialog) => {
      let pressedOutside = false;
      dialog.addEventListener('pointerdown', (e) => {
        pressedOutside = outside(e, dialog);
      });
      dialog.addEventListener('pointerup', (e) => {
        if (pressedOutside && outside(e, dialog)) dialog.close();
      });
    });
  }

  document.querySelectorAll<HTMLDialogElement>('dialog').forEach((dialog) => {
    dialog.addEventListener('close', () => {
      dialog.returnValue = '';
    });
  });
}

interface PaletteAction {
  label: string;
  hint: string;
  run: () => void;
}

function initPaletteAndKeys(): void {
  const paletteDialog = document.getElementById('palette-dialog') as HTMLDialogElement | null;
  const keysDialog = document.getElementById('keys-dialog') as HTMLDialogElement | null;
  const paletteInput = document.getElementById('palette-input') as HTMLInputElement | null;
  const paletteForm = document.getElementById('palette-form') as HTMLFormElement | null;
  const paletteList = document.getElementById('palette-list');

  const actions: PaletteAction[] = [
    { label: ': goto /architecture', hint: 'Principles & Turing tape', run: () => document.getElementById('architecture')?.scrollIntoView() },
    { label: ': goto /namespace', hint: 'Standard Namespace & q', run: () => document.getElementById('namespace')?.scrollIntoView() },
    { label: ': goto /cli', hint: 'Interactive & Pipe modes', run: () => document.getElementById('cli')?.scrollIntoView() },
    { label: '! ls /agent/root', hint: 'Inspect AgentFS', run: () => runShellCommand('ls /agent/root', true) },
    { label: '! cat /agent/root/machine/tape', hint: 'Read Turing Tape', run: () => runShellCommand('cat /agent/root/machine/tape', true) },
    { label: '! q list', hint: 'Quartermaster (skill, mcp, tool, provider)', run: () => runShellCommand('q list', true) },
    { label: '! cat /srv/connections', hint: 'First-party & q providers', run: () => runShellCommand('cat /srv/connections', true) },
    { label: '! man alan9', hint: 'Read ALAN9(7)', run: () => runShellCommand('man alan9', true) },
    { label: '! man q', hint: 'Read Q(1)', run: () => runShellCommand('man q', true) },
    { label: ': theme paper', hint: 'Archival Paper (Light)', run: () => setTheme('paper') },
    { label: ': theme midnight', hint: 'Klein Midnight (Dark)', run: () => setTheme('midnight') },
    { label: ': theme dumb', hint: 'TERM=dumb (1978 Monochrome)', run: () => setTheme('dumb') },
  ];

  let filtered = actions;
  let selectedIdx = 0;

  const updateActiveItem = () => {
    if (!paletteList) return;
    const items = paletteList.querySelectorAll<HTMLLIElement>('.palette-item');
    items.forEach((li, idx) => {
      li.classList.toggle('palette-item--active', idx === selectedIdx);
      if (idx === selectedIdx) li.scrollIntoView({ block: 'nearest' });
    });
  };

  const renderList = (query: string) => {
    if (!paletteList) return;
    const q = query.trim().toLowerCase();
    filtered = q
      ? actions.filter(
          (a) => a.label.toLowerCase().includes(q) || a.hint.toLowerCase().includes(q),
        )
      : actions;
    selectedIdx = 0;

    paletteList.innerHTML = '';
    filtered.forEach((action, idx) => {
      const li = document.createElement('li');
      li.className = `palette-item${idx === selectedIdx ? ' palette-item--active' : ''}`;
      const btn = document.createElement('button');
      btn.type = 'button';
      const labelSpan = document.createElement('span');
      labelSpan.textContent = action.label;
      const hintSpan = document.createElement('span');
      hintSpan.className = 'palette-item__hint';
      hintSpan.textContent = action.hint;
      btn.append(labelSpan, hintSpan);
      btn.addEventListener('mouseenter', () => {
        selectedIdx = idx;
        updateActiveItem();
      });
      btn.addEventListener('click', () => {
        paletteDialog?.close();
        action.run();
      });
      li.append(btn);
      paletteList.append(li);
    });
  };

  const openPalette = () => {
    if (!paletteDialog) return;
    if (paletteInput) paletteInput.value = '';
    renderList('');
    paletteDialog.showModal();
    paletteInput?.focus();
  };

  paletteInput?.addEventListener('input', () => renderList(paletteInput.value));
  paletteInput?.addEventListener('keydown', (event) => {
    if (event.key === 'ArrowDown' && filtered.length > 0) {
      event.preventDefault();
      selectedIdx = (selectedIdx + 1) % filtered.length;
      updateActiveItem();
      return;
    }
    if (event.key === 'ArrowUp' && filtered.length > 0) {
      event.preventDefault();
      selectedIdx = (selectedIdx - 1 + filtered.length) % filtered.length;
      updateActiveItem();
      return;
    }
    if (event.key === 'Enter') {
      event.preventDefault();
      if (event.isComposing || event.keyCode === 229) return;
      paletteForm?.requestSubmit();
    }
  });

  paletteForm?.addEventListener('submit', (event) => {
    event.preventDefault();
    const chosen = filtered[selectedIdx];
    if (chosen) {
      paletteDialog?.close();
      chosen.run();
    } else if (paletteInput?.value.trim()) {
      const cmd = paletteInput.value.trim().replace(/^[!:]/, '').trim();
      paletteDialog?.close();
      runShellCommand(cmd, true);
    }
  });

  document.getElementById('open-palette-btn')?.addEventListener('click', openPalette);
  document.getElementById('open-keys-btn')?.addEventListener('click', () => keysDialog?.showModal());

  const sections = ['top', 'architecture', 'namespace', 'cli', 'manpage'];
  let sectionIdx = 0;

  window.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') {
      closeMiniDrawer();
      return;
    }
    const target = event.target as HTMLElement | null;
    const isTyping =
      target?.tagName === 'INPUT' || target?.tagName === 'TEXTAREA' || target?.isContentEditable;
    if (isTyping || event.metaKey || event.ctrlKey || event.altKey) return;

    if (event.key === '/') {
      event.preventDefault();
      openPalette();
    } else if (event.key === '?') {
      event.preventDefault();
      keysDialog?.showModal();
    } else if (event.key === 'd') {
      const cur = document.documentElement.dataset.theme;
      setTheme(cur === 'dumb' ? 'paper' : 'dumb');
    } else if (event.key === 'j') {
      sectionIdx = Math.min(sections.length - 1, sectionIdx + 1);
      document.getElementById(sections[sectionIdx])?.scrollIntoView({ behavior: 'smooth' });
    } else if (event.key === 'k') {
      sectionIdx = Math.max(0, sectionIdx - 1);
      document.getElementById(sections[sectionIdx])?.scrollIntoView({ behavior: 'smooth' });
    }
  });
}

/* ---------- Copy Install Command ---------- */
function initCopyButtons(): void {
  document.querySelectorAll<HTMLButtonElement>('[data-copy]').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const text = btn.dataset.copy;
      if (!text) return;
      try {
        await navigator.clipboard.writeText(text);
        const orig = btn.textContent;
        btn.textContent = 'copied';
        window.setTimeout(() => {
          btn.textContent = orig;
        }, 1500);
      } catch {
        btn.textContent = 'select text';
      }
    });
  });
}

/* ---------- Boot ---------- */
document.addEventListener('DOMContentLoaded', () => {
  applyThemeDom(readStoredTheme());

  document.querySelectorAll<HTMLButtonElement>('[data-theme-btn]').forEach((btn) => {
    btn.addEventListener('click', () => {
      const mode = btn.dataset.themeBtn as ThemeMode;
      if (mode) setTheme(mode);
    });
  });

  window.addEventListener('storage', (e) => {
    if (e.key === STORAGE_KEY) applyThemeDom(readStoredTheme());
  });

  initTerminal();
  initAnatomy();
  initScrollspy();
  initCliSwitcher();
  setupDialogFallbacks();
  initPaletteAndKeys();
  initCopyButtons();
});
