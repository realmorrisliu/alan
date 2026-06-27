---
name: skill-creator
description: |
  Design, scaffold, validate, and iterate on alan skill packages.

  Use this when:
  - The user wants to create a new skill package
  - The user wants to update or refactor an existing skill package
  - You need to structure SKILL.md, sidecars, package-local binaries, scripts,
    references, or assets
  - You need to validate package shape, execution metadata, or description fit
  - You need to plan explicit authoring and evaluation workflows for a skill

metadata:
  short-description: Create or update alan skill packages
  tags: [skills, authoring, scaffolding, validation, eval]
capabilities:
  required_tools: [bash]
compatibility:
  requirements: Use the local `alan` CLI on PATH for init, validate, and eval helper flows.
---

# Skill Creator

This skill helps author ordinary directory-backed alan skill packages.

## Working Model

Treat every skill as a single `skill package` with one root `SKILL.md`.

Package-local surfaces:

- `SKILL.md`: portable selection contract and core workflow
- `skill.yaml` / `package.yaml`: alan-native runtime defaults
- `bin/`: package-local executable tools that should travel with the skill in
  source and packaged form
- `scripts/`: package-private glue, wrappers, and compatibility helpers when a
  `bin/` executable or shared Rust CLI/bin is not the better fit
- `references/`: material to load only when needed
- `assets/`: templates or output resources
- `agents/`: child-agent roots and authoring metadata such as `openai.yaml`
- `evals/` and `eval-viewer/`: explicit authoring and review surfaces

Do not invent a second abstraction for authoring. Prefer tightening the package
shape and using explicit tooling.

## Authoring Workflow

1. Clarify the user task and what the description should communicate.
2. Pick a short package name in lowercase hyphen-case.
3. Scaffold the package with `alan skills init`.
4. Keep `SKILL.md` lean. Move detailed reference material into `references/`.
5. Prefer package-local `bin/` executables for deterministic skill-private
   tools, reusable Rust CLI/bin tooling for shared workflows, and keep only
   glue or ecosystem-bound helpers in `scripts/`.
6. If the skill delegates, export a package-local child agent under `agents/`.
7. Validate with `alan skills validate --path <package>`.
8. Run explicit evaluation with `alan skills eval --path <package>`.

## Resource Guide

- Read `references/authoring.md` for package design guidance.
- Read `references/openai_yaml.md` before editing `agents/openai.yaml`.
- Read `references/schemas.md` for the structured eval manifest shape.
- Reuse `assets/templates/` when you need starter package content.
- Use `alan skills init`, `alan skills validate`, and `alan skills eval` for
  the primary authoring flow.
- Use `bin/aggregate-benchmark` and `bin/generate-review` for package-local
  review artifact regeneration.

## Rules

1. Prefer one package over sprawling nested abstractions.
2. Keep host/runtime tools, package-local `bin/` tools, package-local helpers,
   and shared authoring tooling separate.
3. Prefer package-local `bin/` executables or shared Rust CLI/bin surfaces over
   shell, Python, or TypeScript helpers unless an external ecosystem or a tiny
   package-private step makes a script the better fit.
4. If shared authoring or eval helpers move into Rust, prefer consolidating
   them into existing operator surfaces such as the `alan` CLI rather than
   introducing another standalone helper package.
5. Do not auto-load authoring or eval assets into the runtime prompt.
6. Make `description` concrete enough that catalog-based selection stays
   reliable.
