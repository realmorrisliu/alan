---
name: alan
section: 1
title: "ALAN(1)"
whatis: "alan - boot a foreground alan9 instance and open its terminal UI"
order: 1
---

## NAME

alan - boot a foreground alan9 instance and open its terminal UI

## SYNOPSIS

```text
alan [--cwd PATH] [--no-auto-mount]
     [--smart | --trust [--unsafe-host-account]]
     [--grant GRANT_ID]... [--package PACKAGE_ID]...
     [--fileserver PACKAGE_ID[:ro|rw][:net]]... [--image PATH]...
     [--resume [ROLLOUT_ID]] [--ephemeral]
alan -p PROMPT [--json]
command | alan [--json]

alan login [provider] [--no-browser] [--profile ID]
alan update [--check] [--channel stable|beta] [--version VER] [--json]
alan update channel [stable|beta]
alan connection list | current | show | add | edit | login | logout | test | remove | default
alan q install | list | inspect | enable | disable | upgrade | uninstall | sync | set-secret
alan host status | stop | mount | grant | legacy-state
alan project ...
alan workflow ...
alan memory ...
alan skills list | init | validate | eval
alan doctor [--json]
alan man [topic]
alan about
alan reset [--all] [--yes]
alan legacy-state inspect | migrate-installation | cleanup | import
```

## DESCRIPTION

Each bare `alan` invocation boots and owns one foreground alan9 instance and
its Root Agent Process. With terminal stdin and stdout it opens the file-backed
Agent renderer on that instance's `/agent/root`.

With redirected stdin, or with `-p`, the invocation submits one Agent task,
writes the final answer to stdout, sends diagnostics to stderr, and returns the
task's exit status. `--json` streams that one-shot run as newline-delimited
JSON events.

Exiting Alan stops its instance. Detaching a terminal view (for example in
tmux) is separate from exiting. A new invocation never adopts earlier work on
its own; `--resume` selects durable recovery explicitly, and that recovery flow
is still in active implementation.

Alan runs in any ordinary POSIX/VT terminal. No terminal host is required or
detected.

## OPTIONS

`--cwd PATH`
: Initial project directory candidate.

`--no-auto-mount`
: Do not mount the launch directory. By default an ordinary project directory
  is mounted `ReadWrite`; `/`, `$HOME` and the system temp directory are never
  auto-mounted.

`--smart`
: Keep the OS sandbox active with adaptive smart action review (`confined/smart`).

`--trust`
: Bypass per-action policy and Guardian prompts while keeping the OS sandbox
  active (`confined/trusted`).

`--unsafe-host-account`
: Requires `--trust`. Disables the OS sandbox and runs directly under your Host
  account. No same-account tamper resistance is claimed.

`--grant GRANT_ID`
: Activate a saved Host Mount grant for this invocation.

`--package ID`, `--fileserver ID[:ro|rw][:net]`
: Activate an installed Quartermaster package for this invocation only.

`--resume [ROLLOUT_ID]`, `--ephemeral`
: Select a Root rollout explicitly, or neither read nor overwrite the durable
  root rollout pointer.

## TUI CONTROLS

```text
:  text        Agent input
!  command     request a command through the governed bash Tool
/login         connect a provider
/project       manage Host Mounts
/model         switch model or reasoning effort
/permissions   toggle trust posture (also Shift+Tab)
/new, /fork    start fresh, or edit an earlier prompt
/issue, /bug   draft and confirm a redacted GitHub issue
/status        inspect current state
/settings      settings, reset, release channel
/doctor        read-only diagnostics
/help          list available controls
Ctrl+O         inspect retained Tool detail (Space/b page, Esc back)
Ctrl+Z         suspend
Tab / Enter    insert completion / run selected slash command
```

A leading `!` never executes a Host command directly.

## ENVIRONMENT

`ALAN_CONFIG_PATH`
: Absolute path to an agent configuration file. Configuration selects a
  connection with `connection_profile`; it does not hold secrets.

`ALAN_INSTANCE_RUNTIME_DIR`
: Runtime directory for this instance. Separate directories give separate
  Roots; one directory admits only one owner at a time.

`ALAN_NO_UPDATE_CHECK=1`
: Disable the cached startup release check (also `CI=1`).

`ALAN_NO_SETUP`
: Skip the first-run setup wizard.

`ALAN_NO_RESUME=1`
: Leave the root rollout pointer untouched (same as `--no-resume-update`).

## FILES

```text
macOS  ~/Library/Application Support/Alan/System Store/
       ~/Library/Application Support/Alan/Host Store/
Linux  ${XDG_DATA_HOME:-$HOME/.local/share}/Alan/
```

System Store holds service state: `services/agent-runtime/`,
`services/connections/`, `services/memory/`, `services/packages/`,
`services/updater/`. Host Store holds `credentials/` and `auth.json`. Neither
is ever mounted into a Process.

## EXIT STATUS

0 on success and non-zero on error. One-shot runs return the task's exit
status; values outside 1–255 are reported as 1.

## SEE ALSO

alan9(7), agentfs(4), q(1), install(8)
