//! Standard Namespace (`/man`) UNIX manual pages for Alan and alan9.

/// One read-only UNIX manual page projected under `/man/<section>/<name>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManPage {
    pub section: &'static str,
    pub name: &'static str,
    pub path: &'static str,
    pub summary: &'static str,
    pub content: &'static str,
}

/// Canonical manual pages mounted at `/man` and exposed by `alan man`.
pub const PAGES: &[ManPage] = &[
    ManPage {
        section: "1",
        name: "alan",
        path: "1/alan",
        summary: "programmable personal computing environment and foreground shell",
        content: ALAN_1,
    },
    ManPage {
        section: "1",
        name: "q",
        path: "1/q",
        summary: "Quartermaster package command over alan9 namespace paths",
        content: Q_1,
    },
    ManPage {
        section: "1",
        name: "connection",
        path: "1/connection",
        summary: "manage provider connection profiles and Host Store secrets",
        content: CONNECTION_1,
    },
    ManPage {
        section: "1",
        name: "update",
        path: "1/update",
        summary: "check for and install standalone alan CLI releases",
        content: UPDATE_1,
    },
    ManPage {
        section: "4",
        name: "agentfs",
        path: "4/agentfs",
        summary: "file-server layout projected at /agent/<pid> and /agent/root",
        content: AGENTFS_4,
    },
    ManPage {
        section: "7",
        name: "alan9",
        path: "7/alan9",
        summary: "Standard Namespace, OS-boundary confinement, and AI Turing Machine",
        content: ALAN9_7,
    },
];

/// Return `(relative_path, utf8_bytes)` entries for mounting `MemFs::with_read_only_tree` at `/man`.
pub fn tree_entries() -> Vec<(String, Vec<u8>)> {
    PAGES
        .iter()
        .map(|page| (page.path.to_string(), page.content.as_bytes().to_vec()))
        .collect()
}

/// Look up a manual page by topic (`alan`, `1/alan`, `alan(1)`, `/man/1/alan`, `alan9`, etc.).
pub fn lookup(topic: &str) -> Option<&'static ManPage> {
    let trimmed = topic.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = trimmed
        .strip_prefix("/man/")
        .unwrap_or(trimmed)
        .trim_matches('/');
    let lower = normalized.to_ascii_lowercase();
    let without_section_suffix = lower
        .rsplit_once('(')
        .and_then(|(name, rest)| rest.ends_with(')').then_some(name))
        .unwrap_or(lower.as_str());

    PAGES.iter().find(|page| {
        page.path == without_section_suffix
            || page.name == without_section_suffix
            || format!("{}({})", page.name, page.section) == lower
    })
}

/// Render a concise directory index of all available manual pages.
pub fn index_text() -> String {
    let mut lines = vec![
        "ALAN(1)                   Manual Index                   ALAN(1)".to_string(),
        String::new(),
        "AVAILABLE MANUAL PAGES (/man)".to_string(),
    ];
    for page in PAGES {
        lines.push(format!(
            "  /man/{:<14} {:<16} {}",
            page.path,
            format!("{}({})", page.name, page.section),
            page.summary
        ));
    }
    lines.push(String::new());
    lines.push("USAGE".to_string());
    lines.push("  alan man <topic>      read a manual page from the CLI".to_string());
    lines.push("  /man <topic>          read a manual page inside the TUI".to_string());
    lines.join("\n")
}

const ALAN_1: &str = "\
ALAN(1)                  Alan Manual                  ALAN(1)

NAME
  alan - programmable personal computing environment on alan9

SYNOPSIS
  alan [OPTIONS]
  alan -p, --print <PROMPT> [--json] [--image <PATH>...]
  alan [--package <ID>]... [--fileserver <ID>[:ro|rw][:net]]...
  echo <TASK> | alan [--json]
  alan about | alan man [TOPIC]
  alan login [PROVIDER]
  alan connection <SUBCOMMAND>
  alan skills <SUBCOMMAND>
  alan q <install|enable|disable|list|inspect|upgrade|uninstall|sync>
  alan workflow <init|init-self-iterate|init-release-prep|watch|run|list|logs>
  alan update [OPTIONS]

DESCRIPTION
  Each bare `alan` invocation boots one foreground alan9 instance and
  attaches its file-backed TUI to /agent/root. When stdin is piped or
  `-p/--print` is supplied, `alan` submits one task and writes the answer
  (or NDJSON event stream with `--json`) to stdout.

  Alan targets standard POSIX/VT terminals (Ghostty, iTerm2, Kitty,
  WezTerm, Terminal.app, tmux, SSH) with zero host-specific lock-in.

COMPOSER & COMMANDS
  : <task>              send a task to the Root Agent Process
  ! <command>           run an explicit command through alan-shell
  /new  /model  /tape   reset view, switch model/effort, inspect Turing Tape
  /login                connect or authenticate an LLM provider in place
  /man [topic]          read /man manual pages inside the TUI
  /project [revoke]     explicitly grant or revoke a Host Mount
  /issue [title]  /bug  draft and confirm a redacted GitHub issue
  /compact  /rollback   summarize context or undo the last turn
  /status  /help  /quit inspect status, show shortcuts, or exit

ENVIRONMENT
  ALAN_THEME            auto | midnight | paper | ansi | dumb
  NO_COLOR              disable colors and text attributes when non-empty
  TERM=dumb             disable colors, attributes, and Kitty/OSC controls
  ALAN_NO_WELCOME=1     suppress the empty-session welcome header
  ALAN_NO_RESUME=1      skip updating the global resume pointer (`--no-resume-update`)
  ALAN_NO_UPDATE_CHECK=1
                        skip passive 24h update check on interactive boot
  ALAN_MANIFEST_URL     override release manifest URL (`latest.json`)
  ALAN_CONFIG_PATH      absolute path to an explicit agent.toml override

SEE ALSO
  alan9(7), agentfs(4), q(1), connection(1), update(1)
";

const UPDATE_1: &str = "\
UPDATE(1)                 Alan Manual                 UPDATE(1)

NAME
  alan update - check for and install standalone alan CLI releases

SYNOPSIS
  alan update [--check] [--json] [--version <VER_OR_TAG>] [--force]
              [--install-dir <DIR>] [--manifest-url <URL>] [--quiet]

DESCRIPTION
  Fetches signed release metadata (`latest.json`), compares semantic
  versions against the running `alan` binary, verifies the downloaded
  archive SHA-256 digest before extracting `./alan`, and atomically
  replaces the owned installation while leaving System Store and Host
  Store data untouched.

OPTIONS
  --check               report update status without installing
  --json                emit structured JSON output
  --version <VER>       target a specific version or `v` tag
  --force               allow replacing unowned binaries or downgrades
  --install-dir <DIR>   override target installation directory
  --manifest-url <URL>  override release manifest URL (`latest.json`)
  --quiet               suppress routine status output

SUPPORTED TARGETS
  aarch64-apple-darwin       x86_64-apple-darwin
  x86_64-unknown-linux-gnu   aarch64-unknown-linux-gnu

OWNERSHIP & PASSIVE CHECK
  Coordinates updates via `<install_dir>/.alan-cli-install.lock` and
  `<install_dir>/.alan-cli-manifest` (`alan|<sha256>`), retiring legacy
  `.alan-cli-manifest-stable` / `.alan-cli-manifest-dev` entries when
  upgrading an owned installation.

  Interactive TUI startup caches 24h passive check results in System
  Store at `services/updater/last-check.json` and skips checks when
  `ALAN_NO_UPDATE_CHECK=1`, `CI=1`, `--ephemeral`, or non-TTY stdio.

ENVIRONMENT
  ALAN_MANIFEST_URL          override default release manifest URL
  ALAN_RELEASE_MANIFEST_URL  fallback manifest URL override
  ALAN_NO_UPDATE_CHECK=1     disable passive 24h startup check

SEE ALSO
  alan(1), alan9(7), connection(1)
";

const Q_1: &str = "\
Q(1)                    Quartermaster                   Q(1)

NAME
  q - Package Service client (/bin/q -> /srv/package)

SYNOPSIS
  alan q install [--name <PACKAGE_ID>] [--catalog-only] <PATH>
  alan q enable <PACKAGE_ID> [--access read-only|read-write] [--allow-network] [--skills-only]
  alan q disable <PACKAGE_ID> [--keep-skills]
  alan q list | alan q inspect <PACKAGE_ID>
  alan q upgrade <PACKAGE_ID> <PATH> | alan q uninstall <PACKAGE_ID>
  /bin/q list
  /bin/q install [--name <PACKAGE_ID>] </mnt/...>
  /bin/q uninstall <PACKAGE_ID>

DESCRIPTION
  Quartermaster (`q`) manages immutable extension packages stored in the
  Alan System Store and projected read-only at `/lib/pkg/<id>` only when
  a Process holds an explicit Root binding or descriptor reference.
  Non-skill packages (`tool`, `mcp`, `provider`, `fileserver`) may bundle
  read-only Companion Skills under `skills/`.

ROOT LIFECYCLE (`skills_only` vs `active`)
  `alan q install` catalogs the revision and attaches read-only `skills/`
  (`skills_only`) unless `--catalog-only` is passed. Runtime tools, MCP
  servers, and `fileserver` mounts stay inactive until `alan q enable`
  (or per-invocation `--package` / `--fileserver` flags).

PACKAGE KINDS (ADR-0061)
  skill                 knowledge manuals and workflows (SKILL.md)
  mcp                   Model Context Protocol server packages
  tool                  reusable `/bin` executable Tool packages
  provider              external provider adapters installed as packages
  fileserver            confined external aP file-server plugin packages

BOUNDARIES
  `/bin/q` operates strictly over authorized alan9 namespace paths such as
  `/mnt/import/...`. It never accepts raw Host filesystem paths and never
  performs implicit remote network fetches.

SEE ALSO
  alan(1), alan9(7)
";

const CONNECTION_1: &str = "\
CONNECTION(1)           Connection Service           CONNECTION(1)

NAME
  alan connection - manage provider connection profiles and credentials

SYNOPSIS
  alan login [PROVIDER]
  alan connection list
  alan connection current
  alan connection add <PROVIDER> [--profile <ID>] [--setting key=val] [--default]
  alan connection login <PROFILE_ID> [browser]
  alan connection set-secret <PROFILE_ID>
  alan connection default set <PROFILE_ID>
  alan connection test [PROFILE_ID]
  alan connection remove <PROFILE_ID>

FIRST-PARTY PROVIDERS (ADR-0061)
  magpie                local/self-hosted gateway (keyless by default)
  openai_responses      OpenAI Responses API
  chatgpt               Sign in with ChatGPT (plan & credits OAuth)
  vertex                Google Cloud Vertex AI (Google OAuth / ADC / Express Mode)
  openrouter            OpenRouter PKCE browser OAuth or API key

  Use `alan login` (CLI) or `/login` (inside the TUI) for guided provider setup.

  Anthropic is supported as a protocol adapter / Quartermaster (`q`)
  package rather than a built-in first-party provider.

SECRETS & STORAGE
  Profile metadata lives in the System Store Connection Service subtree.
  Secrets and OAuth credentials stay isolated in the Host Store and are
  never written inline into agent.toml or Host directories.

SEE ALSO
  alan(1), q(1), alan9(7)
";

const AGENTFS_4: &str = "\
AGENTFS(4)               File Servers               AGENTFS(4)

NAME
  agentfs - Agent Process file layout served at /agent

DESCRIPTION
  Agent-ness in alan9 is a file-layout convention, never a second Kernel
  Process type. `/proc/<pid>` is lifecycle truth; `/agent/<pid>` is the
  live AgentFS projection, and `/agent/root` aliases the current Root
  Agent Process.

TREE LAYOUT (/agent/<pid>/)
  io/in                 input submission sink (`UserInputRecord`)
  io/output             live streaming assistant output projection
  events                Process-level queue and action notification stream
  requests/             pending confirmation and structured input yields
  actions/              bounded tool spawn evidence (`name`, `status`, `output`, `result`)
  machine/tape          canonical v1 AI Turing Machine tape (NDJSON)
  machine/state         machine phase and transition state
  machine/checkpoints   durable checkpoint index and current marker
  machine/ctl           control commands (`compact`, `rollback`, `interrupt`, ...)
  machine/ui/           live projections (`activity`, `models`, `queue`, `usage`, `events`)

SEE ALSO
  alan9(7), alan(1)
";

const ALAN9_7: &str = "\
ALAN9(7)                Architecture                ALAN9(7)

NAME
  alan9 - Plan 9 inspired personal computing kernel and file-server OS

STANDARD NAMESPACE
  /proc                 Process lifecycle truth and exit states
  /agent                AgentFS views of Agent Processes (/agent/<pid>)
  /srv                  Mountable file-server handles (aP/1 protocol)
  /bin                  Tools and Agent Executables (/bin/alan, /bin/q)
  /lib                  Immutable projected Skill/package trees (/lib/pkg)
  /man                  Read-only UNIX manual pages
  /mnt                  Explicitly authorized Host Mounts

SECURITY & CONFINEMENT (ADR-0061)
  Launching `alan` never exposes ambient Host directories. File access
  crosses only explicit Host Mount grants under `/mnt`, backed by OS
  confinement (`Seatbelt` on macOS, `Landlock`/reified namespaces on Linux).
  System Store (durable evidence, Memory Stores, package catalog) and Host
  Store (credentials) remain strictly separated from Host repositories.

AI TURING MACHINE (ADR-0055)
  Each Agent Process advances across a durable tape (`machine/tape`),
  explicit machine state, deterministic or typed transitions, bounded
  Tool Process spawns, and persisted System Store checkpoints.

SEE ALSO
  alan(1), agentfs(4), q(1), connection(1)
";
