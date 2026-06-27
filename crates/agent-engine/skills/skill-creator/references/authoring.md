# Skill Authoring Guide

Use this guide when shaping or reviewing a skill package.

## Package Design

- Keep the package centered on one root `SKILL.md`.
- Put detailed supporting material in `references/`, not in the main skill body.
- Prefer deterministic scripts for repetitive or fragile tasks.
- Put templates and output resources in `assets/`.
- Put child-agent roots under `agents/<name>/`.

## Authoring Layers

- Runtime tools: host capabilities registered by alan
- Package-local helpers: scripts inside one package
- Reusable skill tooling: shared authoring helpers used by multiple packages

Do not collapse all three layers into `alan` top-level commands.

## Eval Workflow

- Put structured eval manifests under `evals/evals.json`.
- Keep fixtures under `evals/files/` when a case needs external input files.
- Keep graders, analyzers, and comparators explicit under `agents/`.
- Write review assets under `eval-viewer/`.
- Prefer shared authoring/eval tooling for aggregation and review generation.
