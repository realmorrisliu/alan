import { For } from 'solid-js'

import { AcmeWindow, Article, Fn, Footnotes, ManSection, RioWindow } from '../components/Plan9'
import { RioDesktop } from '../islands/RioDesktop'
import { issue, site } from '../site'

const CONTENTS = [
  { href: '#processes', no: 'No. 1', title: 'Agents are processes' },
  { href: '#files', no: 'No. 2', title: 'State is files' },
  { href: '#boundaries', no: 'No. 3', title: 'Mounts are the boundary' },
  { href: '#plate-ii', no: 'Plate II', title: 'The machine room' },
  { href: '#built-on-alan', no: 'Applications', title: 'Built on Alan' },
  { href: '#install', no: 'Appendix', title: 'Installation and changes' },
]

export function Masthead() {
  const { vol, no } = issue(site.version)
  return (
    <header class="journal-head">
      <div class="journal-head__row">
        <p class="journal-head__name">The Alan System Technical Journal</p>
        <p class="journal-head__issue">
          Vol. {vol} <span aria-hidden="true">·</span> No. {no} <span aria-hidden="true">·</span>{' '}
          {site.issueMonth}
        </p>
      </div>
      <hr class="journal-head__rule" />
      <nav class="contents" aria-label="In this issue">
        <p class="contents__label">In this issue</p>
        <ol class="contents__list">
          <For each={CONTENTS}>
            {(item) => (
              <li>
                <a href={item.href}>
                  <span class="contents__no">{item.no}</span>
                  <span class="contents__title">{item.title}</span>
                </a>
              </li>
            )}
          </For>
        </ol>
      </nav>
    </header>
  )
}

export function Processes() {
  return (
    <Article id="processes" no={1} kicker="Processes" title="Agents are processes.">
      <div class="article__grid">
        <div class="man">
          <ManSection name="NAME">
            <p>alan — run an AI agent as an ordinary Process</p>
          </ManSection>
          <ManSection name="SYNOPSIS">
            <pre class="man-synopsis">
              alan{'\n'}
              <em>command</em> | alan &gt; <em>file</em>
              {'\n'}alan -p <em>prompt</em> [--json]
            </pre>
          </ManSection>
          <ManSection name="DESCRIPTION">
            <p>
              Each bare <code>alan</code> invocation boots and owns one foreground alan9 instance and
              its Root Agent Process. An agent has a PID, a parent, credentials, descriptors and an
              exit status, like any other Process.
              <Fn n={1} /> Its tape, requests, actions and machine state are files under{' '}
              <code>/agent/<em>pid</em></code>, so you read it with <code>cat</code>, list it with{' '}
              <code>ls</code>, and pipe work through it with <code>|</code>.
              <Fn n={2} />
            </p>
            <p>
              Alan runs in any ordinary POSIX/VT terminal — Ghostty, iTerm2, Kitty, WezTerm,
              Terminal.app, tmux or SSH. No terminal host is required or detected.
            </p>
          </ManSection>
          <Footnotes
            notes={[
              {
                n: 1,
                body: (
                  <>
                    Lifecycle truth lives in <code>/proc/<em>pid</em></code>; <code>/agent/<em>pid</em></code>{' '}
                    is its agent view. Agent-ness is a file layout, never a second Kernel Process type.
                  </>
                ),
              },
              {
                n: 2,
                body: (
                  <>
                    With redirected stdin, <code>alan</code> submits one task, writes the answer to
                    stdout, diagnostics to stderr, and returns the task's exit status.
                  </>
                ),
              },
            ]}
          />
        </div>
        <div data-island="rio" class="island-rio">
          <RioDesktop />
        </div>
      </div>
    </Article>
  )
}

const NAMESPACE: Array<[string, string]> = [
  ['/proc', 'Process table and lifecycle truth: PID, parent, descriptors, credentials, exit state.'],
  ['/agent', 'AgentFS. One directory per Agent Process; /agent/root is this invocation’s Root.'],
  ['/srv', 'Service Handle Registry, where file-server services post mountable handles.'],
  ['/bin', 'Tools and Agent Executables, including Quartermaster at /bin/q.'],
  ['/lib', 'Package content, projected at /lib/pkg/<id> only by explicit reference.'],
  ['/man', 'Manual pages for commands, file servers and conventions.'],
  ['/mnt', 'Host Mounts that were granted explicitly. Nothing else of the host appears.'],
]

export function Files() {
  return (
    <Article id="files" no={2} kicker="Files" title="State is files." class="article--flip">
      <div class="article__grid">
        <div class="man">
          <ManSection name="NAME">
            <p>alan9 — the Standard Namespace</p>
          </ManSection>
          <ManSection name="DESCRIPTION">
            <p>
              alan9 is the system under Alan: a Kernel of namespaces, mounts, files, descriptors and
              Processes, and the file-server services that Service Manager starts. Every Process sees
              the same seven directories.
              <Fn n={3} />
            </p>
          </ManSection>
          <ManSection name="FILES">
            <dl class="ns">
              <For each={NAMESPACE}>
                {([path, desc]) => (
                  <>
                    <dt>
                      <code>{path}</code>
                    </dt>
                    <dd>{desc}</dd>
                  </>
                )}
              </For>
            </dl>
          </ManSection>
          <Footnotes
            notes={[
              {
                n: 3,
                body: (
                  <>
                    Control is text written to a <code>ctl</code> file on the object that owns its
                    meaning: lifecycle to <code>/proc/<em>pid</em>/ctl</code>, tape and checkpoints to{' '}
                    <code>machine/ctl</code>. There is no catch-all control file.
                  </>
                ),
              },
            ]}
          />
        </div>
        <figure class="fig" aria-labelledby="fig-agentfs-cap">
          <AcmeWindow path="/agent/1042/" commands={['Del', 'Snarf', 'Get', '| Look']} class="fig__acme">
            <pre class="tree">
              {[
                'io/input           stream',
                'io/output          stream, append-only',
                'status',
                'events             aggregate stream',
                'machine/',
                '  tape             append-only',
                '  status           read-only',
                '  ctl              compact, rollback, …',
                '  checkpoints/current',
                'requests/',
                '  <id>/            kind prompt options',
                '                   status response ctl',
                'actions/',
                '  <id>/            status process output',
                '                   result approval',
                'children/          from /proc parentage',
              ].join('\n')}
            </pre>
          </AcmeWindow>
          <figcaption id="fig-agentfs-cap" class="fig__cap">
            fig. 2 — The AgentFS layout of one Agent Process, after agentfs(4).
          </figcaption>
        </figure>
      </div>
    </Article>
  )
}

const POSTURES: Array<[string, string]> = [
  ['governed', 'The default. The OS sandbox is on and effects outside it ask first.'],
  ['confined/smart', 'Adaptive smart review inside the OS sandbox; high-risk actions still ask. --smart.'],
  ['confined/trusted', 'Skips per-action review; the OS sandbox stays on. --trust, Shift+Tab or /permissions.'],
]

export function Boundaries() {
  return (
    <Article id="boundaries" no={3} kicker="Boundaries" title="Mounts are the boundary.">
      <div class="article__grid">
        <div class="man">
          <ManSection name="DESCRIPTION">
            <p>
              The OS sandbox and explicit mount grants are the security boundary. Host files never
              enter alan9 implicitly: your project directory mounts read-write when you launch{' '}
              <code>alan</code> in it — never <code>/</code>, <code>$HOME</code> or the temp
              directory, and <code>--no-auto-mount</code> opts out. Anything else is granted
              explicitly with <code>/project</code>.
            </p>
          </ManSection>
          <ManSection name="SANDBOX">
            <p>
              On macOS, Tools run under Seatbelt with a generated profile: writes are confined to
              Host Mounts and temp, and the network is denied by default. On Linux, a Landlock
              ruleset is applied.
              <Fn n={4} />
            </p>
          </ManSection>
          <ManSection name="TRUST">
            <dl class="ns ns--wide">
              <For each={POSTURES}>
                {([name, desc]) => (
                  <>
                    <dt>
                      <code>{name}</code>
                    </dt>
                    <dd>{desc}</dd>
                  </>
                )}
              </For>
            </dl>
          </ManSection>
          <ManSection name="CREDENTIALS">
            <p>
              Secrets live in the Host Store and are never mounted into the namespace. Installing a
              package with <code>q</code> changes the catalog only: it grants no mount, credential,
              network or approval authority.
              <Fn n={5} />
            </p>
          </ManSection>
          <Footnotes
            notes={[
              {
                n: 4,
                body: (
                  <>
                    Landlock network confinement depends on the kernel’s ABI, so on Linux escalated
                    commands are sent to a human rather than allowed automatically.
                  </>
                ),
              },
              {
                n: 5,
                body: (
                  <>
                    <code>q</code> installs only from granted alan9 paths such as{' '}
                    <code>/mnt/import</code>; it never reads a raw Host path or fetches a remote URL.
                  </>
                ),
              },
            ]}
          />
        </div>
        <figure class="fig" aria-labelledby="fig-ns-cap">
          <RioWindow class="fig__rio" label="Namespace of process 1042">
            <pre class="rio__pre">
              <span class="rio__prompt">alan9:/$</span> cat /proc/1042/ns{'\n'}
              {[
                'bind /srv/agent-runtime  /agent',
                'bind /srv/package        /lib/pkg',
                'bind /srv/memory         /memory',
                'bind #host/project       /mnt/project  rw',
                '# /, $HOME and /tmp are never auto-mounted.',
              ].join('\n')}
              {'\n'}
              <span class="rio__prompt">alan9:/$</span> ls /mnt{'\n'}project/{'\n'}
              <span class="rio__prompt">alan9:/$</span> ls /mnt/home{'\n'}
              <span class="rio__res--err">ls: /mnt/home: no such file or directory</span>
            </pre>
          </RioWindow>
          <figcaption id="fig-ns-cap" class="fig__cap">
            fig. 3 — A namespace, illustrated. What is not mounted does not exist.
          </figcaption>
        </figure>
      </div>
    </Article>
  )
}
