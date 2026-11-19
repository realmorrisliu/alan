const K = '\x1b[38;2;76;126;255m';
const B = '\x1b[1m';
const D = '\x1b[2m';
const X = '\x1b[0m';

const BRAILLE_MASCOT = "                  \u001b[38;2;214;212;204m\u2880\u28e0\u28e4\u28e4\u28e4\u2840\u001b[0m\n                 \u001b[38;2;214;212;204m\u2880\u28de\u28b5\u2803\u2808\u28bf\u28f7\u2844\u001b[0m     \u001b[38;2;214;212;204m\u287e\u2889\u001b[38;2;76;126;255m\u28c9\u28c9\u28c9\u28c9\u28c9\u28c9\u28c9\u28c9\u28c9\u28c9\u28c9\u28c9\u001b[38;2;214;212;204m\u28c9\u2809\u28df\u28f7\u2826\u28c4\u2840\u001b[0m\n                 \u001b[38;2;214;212;204m\u28b8\u001b[38;2;120;156;235m\u2895\u001b[38;2;214;212;204m\u28ff\u2864\u2824\u283c\u28ff\u28f7\u001b[0m     \u001b[38;2;214;212;204m\u2847\u001b[38;2;76;126;255m\u28f8\u28ff\u289b\u28df\u28dd\u28dd\u28dd\u28dd\u28dd\u28dd\u28cd\u28df\u289f\u001b[38;2;214;212;204m\u28ff\u28a0\u28d7\u284f\u001b[0m  \u001b[38;2;214;212;204m\u2809\u2812\u2824\u2840\u001b[0m\n                \u001b[38;2;214;212;204m\u2860\u283a\u28f5\u001b[38;2;76;126;255m\u28ff\u2841\u001b[0m   \u001b[38;2;214;212;204m\u2808\u2833\u28c4\u001b[0m   \u001b[38;2;214;212;204m\u2847\u001b[38;2;76;126;255m\u28ff\u28df\u28fd\u2812\u2812\u2896\u2896\u289e\u289f\u289f\u289f\u28bf\u001b[38;2;120;156;235m\u2885\u001b[38;2;214;212;204m\u284f\u28b8\u28d5\u2847\u001b[0m     \u001b[38;2;214;212;204m\u28bb\u001b[0m\n               \u001b[38;2;214;212;204m\u28b8\u2807\u001b[0m \u001b[38;2;214;212;204m\u28e7\u28ff\u28ff\u001b[38;2;76;126;255m\u28f7\u001b[38;2;214;212;204m\u2840\u28c0\u2840\u001b[0m \u001b[38;2;214;212;204m\u2808\u28c6\u001b[0m \u001b[38;2;214;212;204m\u28b8\u2801\u001b[38;2;76;126;255m\u28ff\u001b[38;2;214;212;204m\u28d7\u001b[38;2;76;126;255m\u28df\u001b[38;2;120;156;235m\u2895\u2895\u2895\u2895\u2895\u2895\u2895\u2895\u001b[38;2;76;126;255m\u28fd\u001b[38;2;120;156;235m\u2811\u001b[38;2;214;212;204m\u2847\u28b8\u28bd\u2847\u001b[0m     \u001b[38;2;214;212;204m\u28b8\u001b[0m\n              \u001b[38;2;214;212;204m\u28a0\u280f\u001b[0m \u001b[38;2;214;212;204m\u28b8\u28f5\u28bf\u001b[38;2;76;126;255m\u28ff\u28ff\u001b[38;2;214;212;204m\u2847\u281b\u281b\u001b[0m  \u001b[38;2;214;212;204m\u2838\u2844\u28b8\u28a0\u001b[38;2;76;126;255m\u28ff\u001b[38;2;120;156;235m\u2895\u001b[38;2;76;126;255m\u28d7\u001b[38;2;120;156;235m\u2895\u2895\u2895\u2895\u2895\u2895\u2895\u2815\u001b[38;2;76;126;255m\u28df\u001b[38;2;214;212;204m\u2884\u2847\u28fc\u28bd\u2801\u001b[0m     \u001b[38;2;214;212;204m\u28b8\u001b[0m\n             \u001b[38;2;214;212;204m\u2880\u284f\u001b[0m   \u001b[38;2;214;212;204m\u28bf\u28ff\u001b[38;2;76;126;255m\u28ff\u281f\u001b[0m     \u001b[38;2;214;212;204m\u28b3\u2807\u28b8\u28b8\u001b[38;2;76;126;255m\u28ff\u28b5\u28d7\u001b[38;2;120;156;235m\u2895\u2895\u2895\u2895\u2895\u2815\u2895\u2884\u001b[38;2;76;126;255m\u28d7\u001b[38;2;214;212;204m\u28bd\u2801\u28df\u28bd\u001b[0m      \u001b[38;2;214;212;204m\u28b8\u001b[0m\n             \u001b[38;2;214;212;204m\u28b8\u2801\u001b[0m   \u001b[38;2;214;212;204m\u28f8\u2808\u28a7\u28a4\u28c0\u28c0\u001b[38;2;120;156;235m\u2880\u001b[38;2;214;212;204m\u2864\u2816\u2809\u001b[0m \u001b[38;2;214;212;204m\u285f\u001b[38;2;76;126;255m\u28b8\u28ff\u28bd\u28b7\u28b5\u28b5\u28b5\u2835\u28b5\u28a4\u28f4\u28f5\u2897\u001b[38;2;214;212;204m\u28b9\u001b[0m \u001b[38;2;214;212;204m\u28d7\u28ff\u001b[0m      \u001b[38;2;214;212;204m\u28b8\u001b[0m\n            \u001b[38;2;214;212;204m\u2880\u28be\u001b[0m   \u001b[38;2;214;212;204m\u2880\u28ff\u001b[0m  \u001b[38;2;214;212;204m\u2811\u28a6\u2840\u001b[38;2;120;156;235m\u2801\u001b[38;2;214;212;204m\u28c7\u001b[0m   \u001b[38;2;214;212;204m\u2847\u001b[38;2;242;82;72m\u28b8\u286f\u001b[38;2;214;212;204m\u283d\u2837\u2837\u2837\u2837\u28a7\u28e5\u28f5\u28e5\u28e5\u28f5\u287b\u28a0\u28c7\u284f\u001b[0m      \u001b[38;2;214;212;204m\u287c\u001b[0m\n           \u001b[38;2;214;212;204m\u28e0\u280b\u28b8\u2844\u001b[0m \u001b[38;2;214;212;204m\u28e0\u281e\u001b[38;2;120;156;235m\u2801\u001b[0m    \u001b[38;2;214;212;204m\u2808\u2893\u28fd\u001b[0m \u001b[38;2;214;212;204m\u2880\u28c0\u28e3\u001b[38;2;242;82;72m\u28ec\u28a5\u001b[38;2;76;126;255m\u28e4\u28e4\u28e4\u28c0\u28c0\u28c0\u001b[38;2;214;212;204m\u28c0\u28c0\u28c0\u2850\u2836\u2806\u28b8\u2897\u28c7\u28e4\u001b[38;2;76;126;255m\u28e4\u28f4\u28f6\u28bf\u001b[38;2;214;212;204m\u2889\u285f\u28f9\u289f\u289d\u28a6\u001b[0m\n          \u001b[38;2;214;212;204m\u28a0\u2803\u001b[0m  \u001b[38;2;214;212;204m\u2819\u281b\u2801\u001b[38;2;120;156;235m\u2801\u001b[0m      \u001b[38;2;214;212;204m\u2808\u283b\u283e\u283f\u283f\u28ff\u28ff\u28ff\u28ff\u28ff\u28ff\u28ff\u28ff\u28ff\u28ff\u28df\u001b[38;2;76;126;255m\u28fb\u28ff\u287f\u283f\u001b[38;2;214;212;204m\u28ff\u001b[38;2;76;126;255m\u28ff\u28ff\u28ff\u28ff\u001b[38;2;214;212;204m\u28ff\u28ff\u28ff\u28fc\u2828\u28df\u001b[38;2;76;126;255m\u28fd\u001b[38;2;214;212;204m\u28f7\u28fd\u001b[0m\n   \u001b[38;2;214;212;204m\u2840\u001b[0m     \u001b[38;2;214;212;204m\u28a0\u2807\u001b[0m          \u001b[38;2;214;212;204m\u2811\u28c4\u2840\u001b[0m   \u001b[38;2;214;212;204m\u2880\u28e4\u28fc\u28ff\u28ff\u28ff\u287f\u283f\u283f\u28b3\u28ff\u28ff\u287f\u28df\u28ef\u28b6\u28ff\u28ff\u28db\u283b\u28ff\u001b[38;2;76;126;255m\u28ff\u001b[38;2;214;212;204m\u287f\u001b[38;2;76;126;255m\u283f\u289b\u001b[38;2;214;212;204m\u284f\u28b9\u28d7\u289d\u288f\u28fc\u001b[38;2;76;126;255m\u28c0\u001b[0m\n   \u001b[38;2;214;212;204m\u28c9\u28c9\u28c9\u2809\u281b\u2812\u28ba\u001b[0m          \u001b[38;2;214;212;204m\u28e0\u2824\u2824\u283d\u28ff\u001b[38;2;76;126;255m\u28bf\u28ff\u28ff\u28e4\u001b[38;2;214;212;204m\u28ed\u001b[38;2;76;126;255m\u28c9\u28c9\u001b[38;2;214;212;204m\u28c9\u28c9\u28d3\u2812\u2812\u2812\u28d6\u28bf\u28d5\u001b[38;2;76;126;255m\u28f5\u28f5\u28fd\u001b[38;2;214;212;204m\u283f\u001b[38;2;76;126;255m\u281f\u281b\u001b[38;2;214;212;204m\u28a4\u2873\u28c4\u28e0\u281e\u28f2\u001b[38;2;76;126;255m\u28ff\u283f\u001b[38;2;214;212;204m\u2837\u281b\u001b[38;2;76;126;255m\u280b\u2809\u001b[0m\n     \u001b[38;2;214;212;204m\u2809\u2809\u2809\u2809\u28bf\u001b[0m               \u001b[38;2;214;212;204m\u2839\u2857\u2812\u2812\u281a\u283f\u001b[38;2;76;126;255m\u282d\u282d\u282d\u28bf\u28df\u28db\u001b[38;2;214;212;204m\u28db\u28db\u001b[38;2;76;126;255m\u285b\u280b\u2809\u001b[0m      \u001b[38;2;214;212;204m\u2809\u2809\u2809\u2809\u2801\u001b[0m\n         \u001b[38;2;214;212;204m\u28f8\u28c0\u001b[0m               \u001b[38;2;214;212;204m\u28bb\u2819\u28d3\u2852\u2812\u2812\u2826\u2824\u2824\u2824\u28e4\u28c0\u28c0\u28c9\u28c9\u2849\u2809\u2819\u2802\u001b[0m\n        \u001b[38;2;214;212;204m\u28b8\u001b[0m  \u001b[38;2;214;212;204m\u2801\u001b[0m              \u001b[38;2;214;212;204m\u287c\u280a\u2819\u28fb\u001b[0m          \u001b[38;2;214;212;204m\u2808\u2809\u2809\u2801\u001b[0m\n        \u001b[38;2;214;212;204m\u2818\u28c6\u28c0\u28c0\u288c\u28c4\u001b[0m            \u001b[38;2;214;212;204m\u2803\u28e0\u281e\u2801\u001b[0m\n        \u001b[38;2;214;212;204m\u28b8\u282a\u001b[38;2;76;126;255m\u283f\u001b[38;2;214;212;204m\u28df\u001b[38;2;76;126;255m\u28db\u28db\u28db\u28ff\u28f6\u28f6\u28f6\u28f6\u28f6\u001b[38;2;214;212;204m\u28fb\u287f\u283f\u28f7\u2824\u281e\u2801\u001b[0m\n        \u001b[38;2;214;212;204m\u2818\u2832\u2824\u28e4\u28c0\u28c0\u28c0\u28c0\u28c0\u28c0\u28c0\u28c0\u28c0\u28e0\u2824\u2814\u280b\u001b[0m\n               \u001b[38;2;76;126;255m\u28b8\u283f\u001b[38;2;214;212;204m\u28ff\u001b[0m\n               \u001b[38;2;214;212;204m\u28b8\u001b[0m \u001b[38;2;214;212;204m\u28ff\u001b[0m\n               \u001b[38;2;214;212;204m\u2818\u001b[0m \u001b[38;2;214;212;204m\u281b\u001b[0m";

const ANSI_BANNER = `
${BRAILLE_MASCOT}

  ${B}${K}ALAN(1)${X}  —  Programmable personal computing environment on ${K}alan9${X}
  Everything is a file. Even an agent.

  ${B}STANDARD NAMESPACE${X}
    ${K}/proc${X}    Process lifecycle truth
    ${K}/agent${X}   AgentFS views of Agent Processes (/agent/<pid>)
    ${K}/srv${X}     Mountable file-server handles (aP protocol)
    ${K}/bin${X}     Tools & Agent Executables (/bin/alan, /bin/q)
    ${K}/lib${X}     Immutable Quartermaster packages (skill, mcp, tool, provider)
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
