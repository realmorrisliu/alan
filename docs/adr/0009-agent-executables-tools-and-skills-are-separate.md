# Agent Executables, Tools, And Skills Are Separate

Agent work should not be modeled as a list of RPC-style agent API methods.
An Agent Executable is a command that spawns an Agent Process. A Tool is an
external executable command in `/bin` or bound into `/bin`, with `--help`,
`/man/1/<tool>`, and `/lib/exec/<tool>/manifest`. A Skill is a manual-like
knowledge package under `/lib/skill/<name>` and `/man/skill/<name>`. This split
keeps process creation, external action, and instructional knowledge legible in
UNIX terms while keeping Alan-specific package trees out of top-level roots.
