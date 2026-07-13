# Skills and Tools

Tools execute actions; Skills provide knowledge. They are separate Alan OS
concepts even though the current Agent Execution Engine presents both to the
model.

## Tools

`alan-agent-engine` defines the Tool trait, registry, context, policy boundary,
and execution orchestration. `alan-tools` provides the current builtins:

| Profile | Tools |
| --- | --- |
| core | `read_file`, `write_file`, `edit_file`, `bash` |
| read-only | `read_file`, `grep`, `glob`, `list_dir` |
| all | all seven builtins |

Each invocation carries an explicit cwd and passes policy plus the selected
execution backend. Durable Tool payloads are redacted, bounded, and linked to
rollout effect evidence.

Agent Processes also receive virtual transition helpers where supported:

- `request_confirmation`;
- `request_user_input`;
- `update_plan`;
- delegated Skill invocation and child termination for Process-local child
  execution.

Live child lifecycle is read through `/proc` and `/agent`; bounded child launch
metadata is not a lifecycle authority.

## Skills

A Skill is a directory with a required `SKILL.md` and optional resources:

```text
my-skill/
├── SKILL.md
├── skill.yaml
├── package.yaml
├── bin/
├── scripts/
├── references/
├── assets/
├── evals/
└── agents/
```

Installed packages, explicit Skill or Agent Definition descriptors, and
builtin packages are resolved into one capability view. Host directories are
ordinary mounted content until explicitly imported or passed by descriptor.
`skill_overrides` controls whether a Skill is enabled and whether implicit
invocation is allowed.

Delegated Skills launch a bounded child Agent Process from an explicit package
target. The result may contain inline text, structured output, and a namespace
file reference for larger evidence.

## Direct CLI

Skill authoring operates on an explicit Host directory; installation is an
explicit Host import:

```bash
alan skills init /path/to/my-skill
alan skills validate /path/to/my-skill
alan skills eval /path/to/my-skill --output-dir target/skills-eval/my-skill
alan host legacy-state import skill /path/to/my-skill --name my-skill
```

## Builtin packages

Current first-party packages include memory, plan, shell control, Skill
creation, repo-coding, and SWE-bench operator support. Their source lives under
`crates/agent-engine/skills/`.

An explicitly bound Memory Store appears at a namespace path such as `/memory`:

```text
/memory/
├── USER.md
├── MEMORY.md
├── handoffs/LATEST.md
├── daily/
├── episodic/
├── working/
├── topics/
└── inbox/
```

## Contract source

Normative behavior lives in the OpenSpec `skill-system-contract`,
`governance-tooling-contract`, `tool-result-presentation`, and related
capabilities.
