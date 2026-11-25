# Alan Arena (`alan-arena`)

Thin pure `kind = "skill"` Quartermaster package for multi-provider read-only design bakeoffs and root-cause competitions using Alan's parallel `spawn_subagent` execution and an independent read-only verifier.

## Zero Ambient Authority

Installing `alan-arena` registers read-only Skill instructions only. It introduces no binaries, no network escalation, and no write privileges; every contender and verifier runs with `read_only = true`.

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
