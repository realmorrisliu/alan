/**
 * A simulated alan9 namespace and a tiny alan-shell interpreter for the
 * website's rio desktop. It runs entirely in the browser: no network, no model.
 *
 * Paths follow the real Standard Namespace (/proc /agent /srv /bin /lib /man
 * /mnt) and the AgentFS layout documented in agentfs(4); file contents are
 * illustrative.
 */

export type OutputTone = 'default' | 'muted' | 'ok' | 'err'

export interface CommandResult {
  text: string
  tone?: OutputTone
  clear?: boolean
  /** Toggle the TERM=dumb easter egg. */
  dumb?: boolean
}

export interface FsNode {
  type: 'dir' | 'file'
  content?: string
  children?: Record<string, FsNode>
}

const file = (content: string): FsNode => ({ type: 'file', content })
const dir = (children: Record<string, FsNode>): FsNode => ({ type: 'dir', children })

const INITIAL_TAPE = [
  '0001  [in]         "inspect /srv and summarize active services"',
  '0002  [tool]       ls /srv -> agent-runtime connections memory package',
  '0003  [checkpoint] ckpt_0003',
  '0004  [out]        "4 file-server services are mounted under /srv."',
].join('\n')

const MAN: Record<string, string> = {
  alan: [
    'ALAN(1)                 Alan System Manual                 ALAN(1)',
    '',
    'NAME',
    '     alan - boot a foreground alan9 instance and open its terminal UI',
    '',
    'SYNOPSIS',
    '     alan [--no-auto-mount] [--trust] [--resume [ID]] [--ephemeral]',
    '     alan -p PROMPT [--json]',
    '     command | alan [--json]',
    '     alan login [provider]',
    '     alan update [--check] [--channel stable|beta]',
    '',
    'DESCRIPTION',
    '     Each bare alan invocation boots and owns one foreground alan9',
    '     instance and its Root Agent Process. With redirected stdin it',
    '     submits one task, writes the answer to stdout and returns the',
    "     task's exit status.",
    '',
    'SEE ALSO',
    '     alan9(7), agentfs(4), q(1), install(8)',
    '     https://alanworks.app/docs',
  ].join('\n'),
  alan9: [
    'ALAN9(7)                Alan System Manual                ALAN9(7)',
    '',
    'STANDARD NAMESPACE',
    '     /proc    Process table and lifecycle truth',
    '     /agent   AgentFS: one directory per Agent Process',
    '     /srv     Service Handle Registry',
    '     /bin     Tools and Agent Executables',
    '     /lib     Package content, projected by explicit reference',
    '     /man     Manual pages',
    '     /mnt     Explicitly granted Host Mounts',
  ].join('\n'),
  agentfs: [
    'AGENTFS(4)              Alan System Manual              AGENTFS(4)',
    '',
    'DESCRIPTION',
    '     Agent-ness is a file layout, never a second Kernel Process',
    '     type. /proc/<pid> is lifecycle truth; /agent/<pid> adds the',
    '     machine, requests and actions of an Agent Process.',
  ].join('\n'),
  q: [
    'Q(1)                    Alan System Manual                    Q(1)',
    '',
    'NAME',
    '     q - Quartermaster, the Package Service command',
    '',
    'SYNOPSIS',
    '     q install --name ID /mnt/import/DIR',
    '     q list | q upgrade ID DIR | q uninstall ID',
    '',
    'NOTES',
    '     Installing changes the catalog only and grants no authority.',
    '     q never reads a raw Host path or fetches a remote URL.',
  ].join('\n'),
}

interface Package {
  id: string
  kind: 'skill' | 'mcp' | 'tool' | 'provider'
}

export class Alan9Shell {
  private root: FsNode
  private cwd = '/'
  private tapeSeq = 4
  private packages: Package[] = [
    { id: 'code-review', kind: 'skill' },
    { id: 'personal-notes-mcp', kind: 'mcp' },
  ]
  private history: string[] = []
  private historyIdx = -1
  private listeners: Array<() => void> = []

  constructor() {
    this.root = this.buildTree()
  }

  onChange(fn: () => void): () => void {
    this.listeners.push(fn)
    return () => (this.listeners = this.listeners.filter((l) => l !== fn))
  }

  private notify(): void {
    for (const fn of this.listeners) fn()
  }

  private buildTree(): FsNode {
    return dir({
      proc: dir({
        '1': dir({ status: file('pid: 1\nppid: 0\nname: service-manager\nstate: running') }),
        '1042': dir({
          status: file('pid: 1042\nppid: 1\nname: root-agent\nstate: idle\nexit: none'),
          ctl: file('# write: interrupt | cancel | signal'),
          ns: file(
            [
              'bind /srv/agent-runtime  /agent',
              'bind /srv/package        /lib/pkg',
              'bind /srv/memory         /memory',
              'bind #host/project       /mnt/project   rw   (launch cwd)',
              '# /, $HOME and /tmp are never auto-mounted.',
            ].join('\n'),
          ),
        }),
      }),
      agent: dir({
        root: dir({
          io: dir({
            input: file('# echo "a task" > /agent/root/io/input'),
            output: file('4 file-server services are mounted under /srv.'),
            events: file('(stream)'),
          }),
          status: file('pid: 1042\nphase: idle\nlast_checkpoint: ckpt_0003'),
          events: file('(aggregate stream)'),
          machine: dir({
            tape: file(INITIAL_TAPE),
            status: file('head: 0004\nhalted: false'),
            ctl: file('# write: compact | rollback <ckpt>'),
            checkpoints: dir({ current: file('ckpt_0003') }),
          }),
          requests: dir({ events: file('(stream)') }),
          actions: dir({
            '0002': dir({
              status: file('exited 0'),
              process: file('/proc/1043'),
              result: file('agent-runtime connections memory package'),
            }),
          }),
          children: dir({}),
        }),
      }),
      srv: dir({
        'agent-runtime': file('service: Agent Runtime Service\nexports: /agent (AgentFS)'),
        connections: file(
          'service: Connection Service\nproviders: magpie, openai_responses, vertex, openrouter\nsecrets: Host Store (never mounted)',
        ),
        memory: file('service: Memory Store\nmount: /memory (by descriptor)'),
        package: file('service: Package Service\nclient: /bin/q (Quartermaster)'),
      }),
      bin: dir({
        alan: file('<Agent Executable>'),
        q: file('<Quartermaster>'),
        ls: file('<Tool>'),
        cat: file('<Tool>'),
      }),
      lib: dir({ pkg: dir({}) }),
      man: dir(
        Object.fromEntries(Object.entries(MAN).map(([name, text]) => [name, file(text)])),
      ),
      mnt: dir({
        project: dir({ 'README.md': file('# your project, granted read-write at launch') }),
        import: dir({ 'my-skills': dir({}) }),
      }),
    })
  }

  getPrompt(): string {
    return `alan9:${this.cwd}$`
  }

  getCwd(): string {
    return this.cwd
  }

  navigateHistory(dir: -1 | 1): string | null {
    if (this.history.length === 0) return null
    this.historyIdx = Math.max(0, Math.min(this.history.length, this.historyIdx + dir))
    return this.history[this.historyIdx] ?? ''
  }

  complete(partial: string): string | null {
    const suggestions = [
      'ls /',
      'ls /agent/root',
      'cat /agent/root/machine/tape',
      'cat /proc/1042/ns',
      'echo "hello" > /agent/root/io/input',
      'q list',
      'q install --name my-skills /mnt/import/my-skills',
      'man alan',
      'man alan9',
      'ps',
      'help',
    ]
    return suggestions.find((s) => s.startsWith(partial) && s !== partial) ?? null
  }

  resolvePath(raw: string): string {
    const base = raw.startsWith('/') ? raw : `${this.cwd === '/' ? '' : this.cwd}/${raw}`
    const stack: string[] = []
    for (const part of base.split('/').filter(Boolean)) {
      if (part === '.') continue
      if (part === '..') stack.pop()
      else stack.push(part)
    }
    return '/' + stack.join('/')
  }

  getNode(path: string): FsNode | null {
    const resolved = this.resolvePath(path)
    if (resolved === '/') return this.root
    let cur: FsNode = this.root
    for (const p of resolved.slice(1).split('/')) {
      const next: FsNode | undefined = cur.children?.[p]
      if (!next) return null
      cur = next
    }
    return cur
  }

  listDir(path: string): string[] {
    const node = this.getNode(path)
    if (node?.type !== 'dir') return []
    return Object.entries(node.children ?? {}).map(([n, c]) => (c.type === 'dir' ? `${n}/` : n))
  }

  readFile(path: string): string | null {
    const node = this.getNode(path)
    return node?.type === 'file' ? (node.content ?? '') : null
  }

  private submitTask(task: string): string {
    const answer = `accepted "${task}"`
    const seqIn = String(++this.tapeSeq).padStart(4, '0')
    const seqOut = String(++this.tapeSeq).padStart(4, '0')
    const tape = this.getNode('/agent/root/machine/tape')
    if (tape) {
      tape.content += `\n${seqIn}  [in]         ${JSON.stringify(task)}\n${seqOut}  [out]        ${JSON.stringify(answer)}`
    }
    const output = this.getNode('/agent/root/io/output')
    if (output) output.content = answer
    const head = this.getNode('/agent/root/machine/status')
    if (head) head.content = `head: ${seqOut}\nhalted: false`
    this.notify()
    return answer
  }

  execute(rawInput: string): CommandResult {
    const input = rawInput.trim()
    if (!input) return { text: '' }
    if (this.history[this.history.length - 1] !== input) this.history.push(input)
    this.historyIdx = this.history.length

    const write = /^echo\s+["']?(.+?)["']?\s*>\s*(\S+)$/.exec(input)
    if (write) {
      const target = this.resolvePath(write[2])
      if (target !== '/agent/root/io/input') {
        return { text: `echo: ${write[2]}: permission denied`, tone: 'err' }
      }
      this.submitTask(write[1])
      return {
        text: 'written. The Root Agent Process appended two cells to its tape:\n  cat /agent/root/machine/tape',
        tone: 'ok',
      }
    }

    const pipe = /^echo\s+["']?(.+?)["']?\s*\|\s*alan$/.exec(input)
    if (pipe) {
      const answer = this.submitTask(pipe[1])
      return { text: `${answer}\n(exit 0)`, tone: 'ok' }
    }

    const [cmd, ...args] = input.split(/\s+/)
    switch (cmd) {
      case 'help':
        return {
          tone: 'muted',
          text: [
            'A simulated alan9 namespace. It runs in your browser; no model is attached.',
            '',
            '  ls [path]   cat path   cd path   pwd   ps',
            '  echo "task" > /agent/root/io/input   write to the Root Agent Process',
            '  echo "task" | alan                   one task over stdin -> stdout',
            '  q list | q install --name ID /mnt/import/DIR',
            '  man alan | alan9 | agentfs | q',
            '  clear',
          ].join('\n'),
        }
      case 'clear':
        return { text: '', clear: true }
      case 'pwd':
        return { text: this.cwd }
      case 'whoami':
        return { text: 'local-owner' }
      case 'uname':
        return { text: 'alan9' }
      case 'export':
      case 'TERM=dumb':
      case 'theme':
        if (input === 'export TERM=dumb' || input === 'TERM=dumb' || input === 'theme dumb') {
          return { text: 'TERM=dumb', tone: 'muted', dumb: true }
        }
        return { text: `${cmd}: not supported here`, tone: 'err' }
      case 'cd': {
        const target = args[0] ?? '/'
        const node = this.getNode(target)
        if (!node) return { text: `cd: ${target}: no such file or directory`, tone: 'err' }
        if (node.type !== 'dir') return { text: `cd: ${target}: not a directory`, tone: 'err' }
        this.cwd = this.resolvePath(target)
        return { text: '' }
      }
      case 'ls': {
        const target = args[0] ?? this.cwd
        const node = this.getNode(target)
        if (!node) return { text: `ls: ${target}: no such file or directory`, tone: 'err' }
        if (node.type === 'file') return { text: this.resolvePath(target) }
        return { text: this.listDir(target).join('  ') || '(empty)' }
      }
      case 'cat': {
        const target = args[0]
        if (!target) return { text: 'usage: cat path', tone: 'err' }
        const node = this.getNode(target)
        if (!node) return { text: `cat: ${target}: no such file or directory`, tone: 'err' }
        if (node.type === 'dir') return { text: `cat: ${target}: is a directory`, tone: 'err' }
        return { text: node.content ?? '' }
      }
      case 'ps':
        return {
          text: [
            '  PID  PPID  STATE    NAME',
            '    1     0  running  service-manager',
            '  102     1  running  agent-runtime',
            '  103     1  running  package',
            ' 1042     1  idle     root-agent      /agent/root',
          ].join('\n'),
        }
      case 'man': {
        const page = (args[0] ?? 'alan').replace(/\(\d\)$/, '')
        const text = MAN[page]
        if (!text) return { text: `man: no entry for ${page}`, tone: 'err' }
        return { text }
      }
      case 'q': {
        const sub = args[0] ?? 'list'
        if (sub === 'list') {
          return {
            text: this.packages.map((p) => `${p.id.padEnd(22)}${p.kind}`).join('\n'),
          }
        }
        if (sub === 'install') {
          const src = args[args.length - 1] ?? ''
          const nameIdx = args.indexOf('--name')
          const id = nameIdx >= 0 ? (args[nameIdx + 1] ?? '') : (src.split('/').pop() ?? '')
          if (!src.startsWith('/mnt/')) {
            return {
              text: 'q: refused: install from a granted alan9 path such as /mnt/import/DIR; q never reads raw Host paths or URLs.',
              tone: 'err',
            }
          }
          if (!this.getNode(src)) return { text: `q: ${src}: no such directory`, tone: 'err' }
          if (!this.packages.some((p) => p.id === id)) {
            this.packages.push({ id, kind: 'skill' })
            this.notify()
          }
          return {
            text: `q: installed ${id}. The catalog changed; no authority was granted.\nA Process sees /lib/pkg/${id} only when launched with an explicit reference.`,
            tone: 'ok',
          }
        }
        return { text: 'usage: q list | q install --name ID /mnt/import/DIR', tone: 'err' }
      }
      default:
        return { text: `${cmd}: not found. Try help.`, tone: 'err' }
    }
  }
}
