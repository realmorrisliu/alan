# Agent Executable, Tool, And Skill Taxonomy V1

This taxonomy replaces the previous Agent Capability Descriptor vocabulary.
Agent ability is expressed through executable files, descriptor-passed context,
manual-like Skills, and file-backed results.

## Agent Executables

Agent Executables create Agent Processes when spawned. They are installed into
normal command namespaces, usually by binding their source directories into
`/bin`.

| Executable | Purpose | Typical descriptors | Output |
| --- | --- | --- | --- |
| `review` | Review code, docs, diffs, or plans. | target files/dirs, repo descriptors, review policy, review skills. | `/agent/<pid>/io/output`, result file, action proposals. |
| `summarize` | Summarize bounded files, streams, or activity. | content descriptors, memory descriptors, summary policy. | summary result file and evidence references. |
| `plan` | Produce a plan without executing external effects. | goal file/input, target descriptors, policy descriptors. | plan result and optional proposed actions. |
| `delegate` | Spawn child Agent Processes for bounded subwork. | parent context descriptors, child skill descriptors, policy. | child Agent Process references and aggregate result. |

Root Agent has an executable image, but it is started as a Service Manager boot
unit and is not exposed as a normal user command by default.

## Tools

Tools are reusable executables. Each Tool must provide:

- `/bin/<tool>` executable
- `/bin/<tool> --help` quick help
- `/man/1/<tool>` stable manual page
- `/lib/exec/<tool>/manifest` machine-readable contract
- optional `/lib/exec/<tool>/examples`

Tool manifests should describe argv, stdin/stdout/result conventions, required
descriptors, effect class, exit statuses, sandbox hints, and examples. Tool
manifests inform agents but do not grant authority.

## Skills

Skills are manual-like knowledge packages. They are installed as file trees and
passed to Agent Processes by descriptor.

Recommended layout:

```text
/lib/skill/<name>
  /README
  /manual
  /examples
  /references

/man/skill/<name>
```

Shell sugar such as `review --skill repo-coding .` resolves the Skill path,
opens a descriptor, and passes it to the Agent Process. The Agent Process does
not receive authority from the Skill; authority comes from descriptors and
access rights.

## Initial Migration Targets

| Current concept | Target |
| --- | --- |
| Built-in tool registry | Executables in `/bin` with `/man/1` and `/lib/exec` metadata |
| Virtual confirmation/input tools | AgentFS request files under `/agent/<pid>/requests` |
| Delegated skill invocation | Child Agent Process spawned from an Agent Executable |
| `SKILL.md` packages | `/lib/skill/<name>` and `/man/skill/<name>` file trees |
| Tool schemas | Tool manifests plus manuals |
