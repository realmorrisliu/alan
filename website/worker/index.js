const K = '\x1b[38;2;0;47;167m'; // Yves Klein Blue (#002FA7)
const R = '\x1b[38;2;224;62;54m'; // Signal Vermilion LED (#E03E36)
const B = '\x1b[1m';
const D = '\x1b[2m';
const X = '\x1b[0m';

const ANSI_BANNER = `
       ${K}╭───╮${X}        ┌──────────────┐
      ${K}╭╯${X} ${K}◉${X} ${K}╰╮${X}       │ ${K}┌──────────┐${X} │   ${B}${K}ALAN(1)${X}
     ╭┤(   )├╮      │ ${K}│ /agent/  │${X} │   ${B}Everything is a file.${X}
      │  o  │       │ ${K}│   root   │${X} │   ${K}Even an agent.${X}
      │     ╰──╮    │ ${K}└──────────┘${X} │
     ╭╯        │    │ ${R}●${X}        ${D}═══${X} │   ${D}alan9 · aP/1 · Rust 2024${X}
     ╰─╮     ╭─╯    └─┬──────────┬─┘   ${D}https://alanworks.app${X}
      ${K}╭┴─────┴╮${X}  ┌────┴──────────┴─┐   ${K}╭───╮${X}
      ${K}╰───┬───╯${X}  └─────────────────┴───${K}╯ ○ ╰╮${X}
          ${K}│${X}       ${D}· · · · · · · · · · ·${X}${K}╰───╯${X}

${B}STANDARD NAMESPACE${X}
  ${K}/proc${X}    Process lifecycle truth
  ${K}/agent${X}   AgentFS views of Agent Processes (/agent/<pid>)
  ${K}/srv${X}     Mountable file-server handles (aP protocol)
  ${K}/bin${X}     Tools & Agent Executables (/bin/alan, /bin/q)
  ${K}/lib${X}     Immutable projected Skill packages
  ${K}/man${X}     Manual pages
  ${K}/mnt${X}     Explicitly authorized Host Mounts

${B}QUICKSTART${X}
  git clone https://github.com/realmorrisliu/alan.git
  cd alan && just install
  alan

${D}https://alanworks.app · https://github.com/realmorrisliu/alan · Apache-2.0${X}
`;

export default {
  async fetch(request, env) {
    const ua = (request.headers.get('user-agent') || '').toLowerCase();
    const url = new URL(request.url);
    if (
      (url.pathname === '/' || url.pathname === '') &&
      (ua.includes('curl') || ua.includes('wget') || ua.includes('httpie'))
    ) {
      return new Response(ANSI_BANNER, {
        headers: {
          'content-type': 'text/plain; charset=utf-8',
          'cache-control': 'public, max-age=300',
        },
      });
    }
    return env.ASSETS.fetch(request);
  },
};
