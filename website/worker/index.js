const ANSI_BANNER = `\x1b[38;5;20m
   ▄▄▄       ██▓    ▄▄▄       ███▄    █
  ▒████▄    ▓██▒   ▒████▄     ██ ▀█   █
  ▒██  ▀█▄  ▒██░   ▒██  ▀█▄  ▓██  ▀█ ██▒
  ░██▄▄▄▄██ ▒██░   ░██▄▄▄▄██ ▓██▒  ▐▌██▒
   ▓█   ▓██▒░██████▒▓█   ▓██▒▒██░   ▓██░
\x1b[0m
\x1b[1mALAN(1)\x1b[0m  —  Programmable personal computing environment on \x1b[38;5;20malan9\x1b[0m
Everything is a file. Even an agent.

\x1b[1mSTANDARD NAMESPACE\x1b[0m
  /proc    Process lifecycle truth
  /agent   AgentFS views of Agent Processes (/agent/<pid>)
  /srv     Mountable file-server handles (aP protocol)
  /bin     Tools & Agent Executables (/bin/alan, /bin/q)
  /lib     Immutable projected Skill packages
  /man     Manual pages
  /mnt     Explicitly authorized Host Mounts

\x1b[1mQUICKSTART\x1b[0m
  git clone https://github.com/realmorrisliu/alan.git
  cd alan && just install
  alan

\x1b[2mhttps://alanworks.app · https://github.com/realmorrisliu/alan · Apache-2.0 · Rust 2024\x1b[0m
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
