/**
 * Virtual alan9 namespace & alan-shell interpreter for the Alan landing page.
 * Grounded in the canonical alan9 architecture (/proc, /agent, /srv, /bin, /lib, /man, /mnt).
 */

export type OutputTone = 'default' | 'muted' | 'ok' | 'err';

export interface CommandResult {
  text: string;
  tone?: OutputTone;
  clear?: boolean;
  theme?: 'paper' | 'midnight' | 'dumb';
  procState?: string;
}

interface FsNode {
  type: 'dir' | 'file';
  content?: string;
  children?: Record<string, FsNode>;
}

const INITIAL_TAPE = [
  '0001  [in]         "inspect /srv and summarize active services"',
  '0002  [transition] eval -> deterministic_read(/srv)',
  '0003  [tool]       ls /srv -> agent-runtime connections memory package',
  '0004  [checkpoint] ckpt_0004  (persisted to System Store)',
  '0005  [out]        "4 file-server services mounted under /srv."',
].join('\n');

export class Alan9Shell {
  private root: FsNode;
  private cwd = '/';
  private tapeSeq = 5;
  private installedSkills = ['code-review', 'openspec-check'];
  private history: string[] = [];
  private historyIdx = -1;
  private listeners: Array<() => void> = [];

  constructor() {
    this.root = this.buildTree();
  }

  onChange(fn: () => void): void {
    this.listeners.push(fn);
  }

  private notify(): void {
    for (const fn of this.listeners) fn();
  }

  readFile(path: string): string | null {
    const node = this.getNode(path);
    return node?.type === 'file' ? (node.content ?? '') : null;
  }

  listDir(path: string): string[] {
    const node = this.getNode(path);
    if (node?.type !== 'dir' || !node.children) return [];
    return Object.entries(node.children).map(([name, child]) =>
      child.type === 'dir' ? `${name}/` : name,
    );
  }

  private buildTree(): FsNode {
    return {
      type: 'dir',
      children: {
        proc: {
          type: 'dir',
          children: {
            '1042': {
              type: 'dir',
              children: {
                status: {
                  type: 'file',
                  content:
                    'pid: 1042\nppid: 1\nname: root-agent\nstate: idle\ncredentials: local-owner\ndescriptors: 7 open (/agent/root, /srv/package, /memory, ...)\nexit: none',
                },
                cmdline: {
                  type: 'file',
                  content: '/bin/alan --root',
                },
                ns: {
                  type: 'file',
                  content:
                    'bind /srv/agent-runtime /agent\nbind /srv/package /lib/pkg\nbind /srv/memory /memory\n# Host directories are never mounted implicitly.',
                },
              },
            },
          },
        },
        agent: {
          type: 'dir',
          children: {
            root: {
              type: 'dir',
              children: {
                in: {
                  type: 'file',
                  content: '# Write a task here: echo "hello" > /agent/root/in',
                },
                out: {
                  type: 'file',
                  content: '4 file-server services mounted under /srv.',
                },
                status: {
                  type: 'file',
                  content:
                    'pid: 1042\nphase: idle\nmodel: connection:chatgpt-main\nturns: 1\nlast_checkpoint: ckpt_0004',
                },
                machine: {
                  type: 'dir',
                  children: {
                    tape: {
                      type: 'file',
                      content: INITIAL_TAPE,
                    },
                    state: {
                      type: 'file',
                      content:
                        'machine: ai-turing-machine (ADR-0055)\nmode: generation + typed-shadow-advice\nhead: 0005\nhalted: false',
                    },
                    checkpoints: {
                      type: 'file',
                      content: 'ckpt_0001  boot\nckpt_0004  post-tool-read  (durable)',
                    },
                  },
                },
                requests: {
                  type: 'dir',
                  children: {
                    pending: { type: 'file', content: '(none)' },
                  },
                },
                actions: {
                  type: 'dir',
                  children: {
                    log: {
                      type: 'file',
                      content: '[0003] spawn /bin/ls /srv -> exit 0 (2ms)',
                    },
                  },
                },
              },
            },
          },
        },
        srv: {
          type: 'dir',
          children: {
            'agent-runtime': {
              type: 'file',
              content: 'service: Agent Runtime Service\nexports: /agent (AgentFS)\nprotocol: aP/1',
            },
            package: {
              type: 'file',
              content: 'service: Package Service\nclient: /bin/q (Quartermaster)\nprotocol: aP/1',
            },
            connections: {
              type: 'file',
              content:
                'service: Connection Service\ndefault_profile: chatgpt-main\nsecrets: isolated in Host Store',
            },
            memory: {
              type: 'file',
              content: 'service: Memory Store\nmount: /memory (descriptor-gated)',
            },
          },
        },
        bin: {
          type: 'dir',
          children: {
            alan: { type: 'file', content: '<ELF/Mach-O: Alan Shell & foreground instance host>' },
            q: { type: 'file', content: '<alan9 executable: Quartermaster package command>' },
            ls: { type: 'file', content: '<aP builtin: list namespace entries>' },
            cat: { type: 'file', content: '<aP builtin: read file descriptors>' },
            ps: { type: 'file', content: '<aP builtin: inspect /proc table>' },
          },
        },
        lib: {
          type: 'dir',
          children: {
            pkg: {
              type: 'dir',
              children: {
                'code-review': {
                  type: 'file',
                  content: 'skill: code-review (immutable projection, explicit descriptor)',
                },
                'openspec-check': {
                  type: 'file',
                  content: 'skill: openspec-check (immutable projection, explicit descriptor)',
                },
              },
            },
          },
        },
        man: {
          type: 'dir',
          children: {
            '1': {
              type: 'dir',
              children: {
                alan: {
                  type: 'file',
                  content:
                    'ALAN(1)                  Alan Manual                  ALAN(1)\n\nNAME\n  alan - programmable personal computing environment\n\nSYNOPSIS\n  alan                      start foreground instance & TUI\n  cat task | alan           submit one task, write answer to stdout\n  alan connection list      inspect connection profiles\n\nDESCRIPTION\n  Each bare `alan` invocation owns one foreground alan9 instance\n  and attaches its terminal renderer to /agent/root.',
                },
                q: {
                  type: 'file',
                  content:
                    'Q(1)                    Quartermaster                   Q(1)\n\nNAME\n  q - manage Skill packages over alan9 namespace paths\n\nSYNOPSIS\n  q list\n  q install --name <id> /mnt/import/<dir>\n  q uninstall <id>\n\nNOTES\n  q never receives raw Host paths or fetches remote URLs.',
                },
              },
            },
            '4': {
              type: 'dir',
              children: {
                agentfs: {
                  type: 'file',
                  content:
                    'AGENTFS(4)               File Servers               AGENTFS(4)\n\nDESCRIPTION\n  Agent-ness is a file-layout convention, never a second Kernel\n  Process type. /proc/<pid> is lifecycle truth; /agent/<pid>\n  projects tape, requests, actions, and machine state.',
                },
              },
            },
            '7': {
              type: 'dir',
              children: {
                alan9: {
                  type: 'file',
                  content:
                    'ALAN9(7)                Architecture                 ALAN9(7)\n\nSTANDARD NAMESPACE\n  /proc   Process lifecycle truth\n  /agent  AgentFS views of Agent Processes\n  /srv    Mountable file-server handles\n  /bin    Tools & Agent Executables\n  /lib    Immutable projected Skill packages\n  /man    Manual pages\n  /mnt    Explicitly authorized Host Mounts',
                },
              },
            },
          },
        },
        mnt: {
          type: 'dir',
          children: {
            import: {
              type: 'dir',
              children: {
                'my-skills': {
                  type: 'file',
                  content: 'authorized Host Mount ready for `q install --name my-skills /mnt/import/my-skills`',
                },
              },
            },
          },
        },
      },
    };
  }

  getPrompt(): string {
    return `alan9:${this.cwd}$`;
  }

  pushHistory(cmd: string): void {
    const trimmed = cmd.trim();
    if (!trimmed) return;
    if (this.history[this.history.length - 1] !== trimmed) {
      this.history.push(trimmed);
    }
    this.historyIdx = this.history.length;
  }

  navigateHistory(dir: -1 | 1): string | null {
    if (this.history.length === 0) return null;
    this.historyIdx = Math.max(0, Math.min(this.history.length, this.historyIdx + dir));
    return this.history[this.historyIdx] ?? '';
  }

  complete(partial: string): string | null {
    const builtins = [
      'ls /',
      'ls /agent/root',
      'cat /agent/root/status',
      'cat /agent/root/machine/tape',
      'cat /proc/1042/status',
      'cat /srv/package',
      'ps',
      'q list',
      'q install --name my-skills /mnt/import/my-skills',
      'man alan9',
      'man agentfs',
      'man q',
      'theme paper',
      'theme midnight',
      'theme dumb',
      'echo "audit /proc/1042" | alan',
      'help',
      'clear',
    ];
    const match = builtins.find((b) => b.startsWith(partial) && b !== partial);
    return match ?? null;
  }

  private resolvePath(raw: string): string {
    const base = raw.startsWith('/') ? raw : `${this.cwd === '/' ? '' : this.cwd}/${raw}`;
    const parts = base.split('/').filter(Boolean);
    const stack: string[] = [];
    for (const part of parts) {
      if (part === '.') continue;
      if (part === '..') stack.pop();
      else stack.push(part);
    }
    return '/' + stack.join('/');
  }

  private getNode(path: string): FsNode | null {
    const resolved = this.resolvePath(path);
    if (resolved === '/') return this.root;
    const parts = resolved.slice(1).split('/');
    let cur: FsNode = this.root;
    for (const p of parts) {
      if (cur.type !== 'dir' || !cur.children || !(p in cur.children)) {
        return null;
      }
      cur = cur.children[p];
    }
    return cur;
  }

  private appendTape(task: string, answer: string): void {
    const tapeNode = this.getNode('/agent/root/machine/tape');
    const outNode = this.getNode('/agent/root/out');
    const statusNode = this.getNode('/agent/root/status');
    const seqIn = String(++this.tapeSeq).padStart(4, '0');
    const seqOut = String(++this.tapeSeq).padStart(4, '0');
    if (tapeNode && tapeNode.type === 'file') {
      tapeNode.content = `${tapeNode.content}\n${seqIn}  [in]         ${JSON.stringify(task)}\n${seqOut}  [out]        ${JSON.stringify(answer)}`;
    }
    if (outNode && outNode.type === 'file') {
      outNode.content = answer;
    }
    if (statusNode && statusNode.type === 'file') {
      statusNode.content = `pid: 1042\nphase: idle\nmodel: connection:chatgpt-main\nturns: ${Math.floor(this.tapeSeq / 2)}\nlast_checkpoint: ckpt_${seqOut}`;
    }
    this.notify();
  }

  execute(rawInput: string): CommandResult {
    const input = rawInput.trim();
    if (!input) return { text: '' };
    this.pushHistory(input);

    // Handle redirect into /agent/root/in
    const redirectMatch = input.match(/^echo\s+["']?(.+?)["']?\s*>\s*(\/agent\/root\/in)$/);
    if (redirectMatch) {
      const task = redirectMatch[1];
      const answer = `root-agent(/proc/1042) accepted "${task}" via /agent/root/in -> checkpointed to tape.`;
      this.appendTape(task, answer);
      return {
        text: `[aP WRITE /agent/root/in] -> 200 OK\n${answer}\nInspect with: cat /agent/root/machine/tape`,
        tone: 'ok',
        procState: 'running /agent/root',
      };
    }

    // Handle Unix pipe into `alan`
    const pipeMatch = input.match(/^echo\s+["']?(.+?)["']?\s*\|\s*alan$/);
    if (pipeMatch) {
      const task = pipeMatch[1];
      const answer = `alan9 foreground instance (pid 1042): processed "${task}".\nEverything in alan9 is a file: lifecycle in /proc, state in /agent, services in /srv.`;
      this.appendTape(task, answer);
      return {
        text: `${answer}\n(exit 0 — foreground instance stopped cleanly)`,
        tone: 'ok',
        procState: 'piped | alan',
      };
    }

    const [cmd, ...args] = input.split(/\s+/);

    switch (cmd) {
      case 'help':
        return {
          text: [
            'alan-shell — virtual alan9 namespace explorer',
            '',
            '  ls [path]                        list namespace directory (try: ls /, ls /agent/root)',
            '  cat <path>                       read a virtual file (try: cat /agent/root/machine/tape)',
            '  cd <path> | pwd                  change or print working namespace path',
            '  ps                               inspect /proc process table',
            '  q list                           list installed Skill packages via Quartermaster',
            '  q install --name <id> <path>     install Skill from an authorized /mnt path',
            '  echo "task" > /agent/root/in     write directly to Root Agent Process input file',
            '  echo "task" | alan               run one task over stdin -> stdout',
            '  man <alan|alan9|agentfs|q>       read UNIX-style manual pages',
            '  theme <paper|midnight|dumb>      switch site theme (try: theme dumb)',
            '  clear                            clear terminal scrollback',
          ].join('\n'),
          tone: 'muted',
          procState: 'reading help',
        };

      case 'clear':
        return { text: '', clear: true, procState: 'idle' };

      case 'pwd':
        return { text: this.cwd, procState: `pwd ${this.cwd}` };

      case 'uname':
        return {
          text: 'alan9 0.1.0-early aP/1 rust-2024 (Plan 9 inspired, terminal-first)',
          procState: 'uname',
        };

      case 'whoami':
        return { text: 'local-owner (explicit descriptor authority)', procState: 'whoami' };

      case 'cd': {
        const target = args[0] ?? '/';
        const resolved = this.resolvePath(target);
        const node = this.getNode(resolved);
        if (!node) return { text: `cd: ${target}: no such path in namespace`, tone: 'err' };
        if (node.type !== 'dir') return { text: `cd: ${target}: not a directory`, tone: 'err' };
        this.cwd = resolved;
        return { text: `cwd -> ${this.cwd}`, tone: 'muted', procState: `cd ${this.cwd}` };
      }

      case 'ls': {
        const target = args[0] ?? this.cwd;
        const resolved = this.resolvePath(target);
        const node = this.getNode(resolved);
        if (!node) return { text: `ls: ${target}: no such file or directory`, tone: 'err' };
        if (node.type === 'file') return { text: resolved };
        const entries = Object.entries(node.children ?? {}).map(([name, child]) =>
          child.type === 'dir' ? `${name}/` : name,
        );
        return {
          text: entries.join('   '),
          procState: `ls ${resolved}`,
        };
      }

      case 'cat': {
        const target = args[0];
        if (!target) return { text: 'usage: cat <namespace-path>', tone: 'err' };
        const resolved = this.resolvePath(target);
        const node = this.getNode(resolved);
        if (!node) return { text: `cat: ${target}: no such file in namespace`, tone: 'err' };
        if (node.type === 'dir') return { text: `cat: ${target}: is a directory`, tone: 'err' };
        return {
          text: node.content ?? '',
          procState: `cat ${resolved}`,
        };
      }

      case 'ps':
        return {
          text: [
            'PID   PPID  STATE   KIND           VIEW',
            '1     0     run     service-mgr    /srv',
            '102   1     run     file-server    /srv/agent-runtime',
            '103   1     run     file-server    /srv/package',
            '1042  1     idle    agent-process  /proc/1042 -> /agent/root',
          ].join('\n'),
          procState: 'inspecting /proc',
        };

      case 'q': {
        const sub = args[0];
        if (!sub || sub === 'list') {
          return {
            text: [
              'Quartermaster (/bin/q -> /srv/package):',
              ...this.installedSkills.map((s) => `  - ${s}   (projected at /lib/pkg/${s} on explicit ref)`),
            ].join('\n'),
            procState: 'q list',
          };
        }
        if (sub === 'install') {
          const nameIdx = args.indexOf('--name');
          const pkgName = nameIdx >= 0 ? args[nameIdx + 1] : 'my-skills';
          const srcPath = args[args.length - 1] ?? '';
          if (!srcPath.startsWith('/mnt/')) {
            return {
              text: 'q: refused. Quartermaster only reads authorized alan9 namespace paths (e.g. /mnt/import/...), never raw Host paths or remote URLs.',
              tone: 'err',
            };
          }
          if (!this.installedSkills.includes(pkgName)) {
            this.installedSkills.push(pkgName);
            const libPkg = this.getNode('/lib/pkg');
            if (libPkg?.children) {
              libPkg.children[pkgName] = {
                type: 'file',
                content: `skill: ${pkgName} (installed from ${srcPath})`,
              };
            }
            this.notify();
          }
          return {
            text: `q: catalog updated with "${pkgName}" from ${srcPath}.\nVisible at /lib/pkg/${pkgName} only when a Process carries an explicit package ref.`,
            tone: 'ok',
            procState: `q install ${pkgName}`,
          };
        }
        return { text: 'usage: q list | q install --name <id> /mnt/import/<dir>', tone: 'err' };
      }

      case 'man': {
        const page = (args[0] ?? 'alan').toLowerCase();
        const map: Record<string, string> = {
          alan: '/man/1/alan',
          q: '/man/1/q',
          agentfs: '/man/4/agentfs',
          alan9: '/man/7/alan9',
        };
        const path = map[page];
        if (!path) {
          return { text: `man: no entry for "${page}". Try: man alan, man alan9, man agentfs, man q`, tone: 'err' };
        }
        const node = this.getNode(path);
        return { text: node?.content ?? '', procState: `man ${page}` };
      }

      case 'theme': {
        const target = args[0]?.toLowerCase();
        if (target === 'paper' || target === 'midnight' || target === 'dumb') {
          return {
            text: `switched display profile -> ${target}`,
            tone: 'ok',
            theme: target,
            procState: target === 'dumb' ? 'TERM=dumb' : `theme:${target}`,
          };
        }
        return { text: 'usage: theme paper | midnight | dumb', tone: 'err' };
      }

      default:
        return {
          text: `alan-shell: unknown command "${cmd}". Type "help" for available commands.`,
          tone: 'err',
        };
    }
  }
}
