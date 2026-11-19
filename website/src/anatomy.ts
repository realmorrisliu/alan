export interface AnatomyNodeData {
  id: string;
  tapeIndex: number;
  path: string;
  title: string;
  owner: string;
  excerptTitle: string;
  excerpt: string;
  execCmd: string;
}

export const ANATOMY_NODES: AnatomyNodeData[] = [
  {
    id: 'executable',
    tapeIndex: 0,
    path: '/bin/alan',
    title: 'Agent Executable',
    owner:
      'An executable bound into /bin that creates an Agent Process when spawned. No hidden global thread manager—each bare `alan` invocation boots its own foreground alan9 instance.',
    excerptTitle: 'spawn -> /proc/1042',
    excerpt: `$ alan
# boots foreground alan9 instance
# mounts /proc, /agent, /srv, /bin, /lib, /man, /mnt
# attaches TUI renderer to this instance's /agent/root`,
    execCmd: 'ls /bin',
  },
  {
    id: 'process',
    tapeIndex: 1,
    path: '/proc/1042',
    title: 'Ordinary Process',
    owner:
      'Process owns identity, lifecycle, credentials, descriptors, and exit state. Agent-ness is a file-layout convention—never a second Kernel Process type.',
    excerptTitle: '/proc/1042/status',
    excerpt: `pid:         1042
ppid:        1
state:       idle
credentials: local-owner
descriptors: /agent/root, /srv/package, /memory
exit:        none`,
    execCmd: 'cat /proc/1042/status',
  },
  {
    id: 'agentfs',
    tapeIndex: 2,
    path: '/agent/root',
    title: 'AgentFS View',
    owner:
      'Served by the Agent Runtime Service over aP. Projects live agent IO, requests, actions, and machine files so humans, scripts, and tools steer agents through ordinary file operations.',
    excerptTitle: 'ls /agent/root',
    excerpt: `/agent/root/
├── in           # write task input
├── out          # read structured output
├── status       # current phase & model binding
├── requests/    # pending permission or input requests
├── actions/     # tool spawn audit log
└── machine/     # tape, state, checkpoints`,
    execCmd: 'ls /agent/root',
  },
  {
    id: 'machine',
    tapeIndex: 3,
    path: '/agent/root/machine',
    title: 'AI Turing Machine',
    owner:
      'ADR-0055: Tape + Transition function (deterministic code, typed evaluation, or generation) + Machine state + Side effects through descriptors. Rollout and checkpoint files own durable evidence.',
    excerptTitle: '/agent/root/machine/tape',
    excerpt: `0001  [in]         "inspect /srv and summarize active services"
0002  [transition] eval -> deterministic_read(/srv)
0003  [tool]       /bin/ls /srv -> agent-runtime connections memory package
0004  [checkpoint] ckpt_0004 (durable in System Store)
0005  [out]        "4 file-server services mounted under /srv."`,
    execCmd: 'cat /agent/root/machine/tape',
  },
];
