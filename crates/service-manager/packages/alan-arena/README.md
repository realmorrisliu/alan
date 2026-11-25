# Alan Arena (`alan-arena`)

Thin pure `kind = "skill"` Quartermaster package for multi-provider design bakeoffs and root-cause competitions using Alan's parallel `spawn_subagent` execution and an independent `read_only: true` verifier.

## Modes & Zero Ambient Authority

- **Default (`read_only`)**: Installing `alan-arena` registers Skill instructions only. By default it introduces no binaries, no network escalation, and no write privileges; every contender and verifier runs with `read_only = true`.
- **Opt-In (`mode = "executable"`)**: When explicitly requested under an OS-enforced sandbox (`Seatbelt`, `LinuxReifiedNamespace`, or `Landlock`), `alan-arena` runs contenders inside isolated Copy-on-Write clones (`workspace_ref` coordinated via `/mnt/branch`), with single-clone narrowed `read_write` mounts, default-denied network, zero Host Store secrets, a `read_only = true` verifier, immediate purge of losing clones, and exit-time `.patch` + scorecard persistence. Under `HostMountPathGuard`, `mode = "executable"` fails closed before creating any CoW workspace.

## Install

```bash
alan q install crates/service-manager/packages/alan-arena
```

## Composing with Imported Community Playbooks

Install any community engineering playbook bundle (for example `pstack`) via `alan q install` alongside `alan-arena`:

```bash
alan q install /path/to/pstack --name pstack
```

Root can follow the upstream playbook workflow (`playbooks/*.md` under `/lib/pkg/pstack/`) while invoking `$alan-arena` for multi-model comparison and independent verifier adjudication without forking upstream playbooks.
