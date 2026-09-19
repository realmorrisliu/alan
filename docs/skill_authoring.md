# Skill Authoring Workflow

> Status: current authoring guide. Normative skill package behavior lives in
> OpenSpec:
> [`skill-system-contract`](../openspec/specs/skill-system-contract/spec.md).

alan's first-party skill authoring tooling operates over the same
directory-backed `skill package` contract used by externally discovered skills.
It does not introduce a second package system.

## Package Model

Every skill package is one directory with one root `SKILL.md` plus optional
bundled assets:

```text
skill-name/
├── SKILL.md
├── skill.yaml
├── package.yaml
├── bin/
├── scripts/
├── references/
├── assets/
├── evals/
├── eval-viewer/
└── agents/
```

Use the root `SKILL.md` for the portable selection contract. Keep it lean. Move
detailed reference material into `references/`, package-local executable tools
into `bin/`, and glue or compatibility helpers into `scripts/`.

alan separates four things:

- host/runtime tools
- package-local executable tools in `bin/`
- package-local helpers in `scripts/`
- reusable authoring/eval tooling shared across packages

Shipping a file inside a skill package does not create a new host-global
runtime tool. `bin/` is the reserved package-local location for deterministic
executables that should travel with the skill package in both source and
packaged form.

## Tooling Preference

The `bin/` and `scripts/` files above are packaged assets, not automatically
registered Alan Tools. q v0 exports Skills only. Host-side authoring/evaluation
hooks and an authorized Tool Process are distinct execution paths; installing
a package grants neither Host execution permission nor a new `/bin` command.
Here “skill package” means one Skill directory; a Package Service distribution
may contain multiple such directories.

For first-party skill authoring and evaluation flows:

- prefer existing Rust CLI/bin surfaces such as `alan ...`
- if a deterministic executable is private to one skill package and should ship
  with that package, prefer a package-local binary in `bin/` over a script in
  `scripts/`
- avoid introducing new shell, Python, or TypeScript scripts unless they are
  genuinely required by an external ecosystem, a trivial package-private glue
  step, or compatibility
- prefer `evals/evals.json` over legacy eval hooks
- when a helper becomes non-trivial or reusable across packages, promote it
  into shared Rust tooling inside existing operator surfaces such as the
  `alan` CLI instead of adding another package-local script or a new standalone
  helper package
- when today’s implementation still sits behind package-local compatibility
  wrappers, prefer those wrappers over introducing or assuming an extra helper
  binary

## Commands

Create a new package from a first-party template:

```bash
alan skills init path/to/my-skill --template inline
alan skills init path/to/repo-review --template delegate
```

Validate package shape, metadata, compatibility sidecars, resources, and
execution shape:

```bash
alan skills validate path/to/my-skill
alan skills validate path/to/my-skill --json
alan skills validate path/to/my-skill --strict
```

Run explicit package-local evaluation hooks:

```bash
alan skills eval path/to/my-skill
alan skills eval path/to/my-skill --output-dir /tmp/skill-eval
alan skills eval path/to/my-skill --require-hook
```

## Templates

V1 ships two first-party templates:

- `inline`: one portable skill with no delegated launch target
- `delegate`: one portable skill plus one package-local launch target whose export
  name matches the package id

Both templates generate ordinary skill packages. They can be installed,
discovered, and mounted the same way as any other package.

## Validation

`alan skills validate` currently checks:

- root package shape and presence of `SKILL.md`
- `SKILL.md` frontmatter parsing and stable field validation
- `skill.yaml`, `package.yaml`, and `agents/openai.yaml` parseability
- resource-directory shape for `scripts/`, `references/`, `assets/`, `evals/`,
  and `eval-viewer/`
- package-local launch-target discovery under `agents/`
- resolved execution shape, including unresolved delegated-package diagnostics

Warnings remain non-fatal by default. `--strict` upgrades warnings to a failing
exit code.

## Eval Workbench

Evaluation remains explicit and package-local. alan does not automatically load
grader/analyzer/comparator/review assets into runtime prompt context.

`alan skills eval` first runs validation, then prefers a structured manifest:

- `evals/evals.json`

Structured manifests currently support:

- `trigger` cases for deterministic direct skill-reference / force-select checks
- `command` cases for explicit candidate runs with optional baseline, grading,
  analyzer, and comparator stages
- comparison modes `with_without_skill`, `new_old_skill`, and `custom`

Structured eval writes stable artifacts under the selected output directory:

- `run.json`
- `benchmark.json`
- `review/index.html`
- per-case artifacts under `cases/<case-id>/`

If no manifest is present, alan falls back to legacy hooks:

- `scripts/eval.sh`
- `scripts/eval.py`

These legacy hooks remain for compatibility. New first-party packages should
prefer `evals/evals.json`, and should not add new Python eval hooks unless the
underlying workflow is unavoidably Python-native.

Legacy hooks run with the package root as the current working directory and
export:

- `ALAN_SKILL_ID`
- `ALAN_SKILL_PACKAGE_ID`
- `ALAN_SKILL_PACKAGE_ROOT`

When no manifest or hook exists, `alan skills eval` reports `no_hook`. This is
non-fatal by default and becomes fatal with `--require-hook`.

## Shared Tooling

alan keeps reusable authoring/eval logic separate from runtime tools.

- `alan skills eval` uses shared internal manifest execution and artifact
  generation logic
- `alan skills aggregate-benchmark` and `alan skills generate-review` are the
  preferred reusable operator surfaces for structured eval artifacts
- package-local wrappers, when they still exist, are compatibility-only and do
  not turn those helpers into runtime tools or new top-level CLI families

Promotion path:

- first prefer an existing Rust CLI/bin surface such as `alan`
- when a helper is private to one skill package and should travel with the
  package, prefer a package-local binary under `bin/`
- keep a helper in `scripts/` only when it is private to one skill package and
  there is a clear reason not to make it a binary in `bin/` or a shared Rust
  CLI/bin
- move a reusable or non-trivial helper into shared Rust tooling when multiple
  skill packages need the same stable operator-side helper, and prefer
  consolidating that work into existing operator surfaces such as the `alan`
  CLI rather than proliferating standalone helper packages
- promote it into a runtime tool only when models need a uniform host-level
  capability rather than an explicit authoring workflow

## Authoring Guidance

- Treat `description` as the selection contract. It should say what the skill
  does and when to use it.
- Keep `SKILL.md` short and procedural.
- Prefer deterministic tooling over repeatedly rewritten code. For first-party
  helpers, default to package-local `bin/` binaries or shared Rust CLI/bin
  surfaces over shell, Python, or TypeScript scripts.
- Use `references/` for detailed schemas, examples, and background material.
- Keep package clutter low. Do not add extra README/changelog/process-history
  files inside skill packages unless they are part of the skill itself.
