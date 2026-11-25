# Design: Personal Capability Extensions via Quartermaster

## Context

Alan already ships the five-kind Quartermaster extension hub (`skill`, `tool`, `mcp`, `provider`, `fileserver`), Host-private credential storage (`crates/os-host/src/secret_store.rs`), Google PKCE/ADC authentication (`crates/auth/src/google.rs`), and the reference `github-fileserver` (`alan-ap` over stdio). What prevents effortless daily leverage of community skills (`poteto/pstack`), `gemini-api-cli` (#1154), Google AI Ultra compute (`google-colab-cli`), and Google Drive (#1166) is a small set of missing Quartermaster platform primitives plus one native `gdrivefs` fileserver.

## Goals / Non-Goals

### Goals
- Keep `q` universal: ingest upstream community multi-skill repositories directly via `q install` without repo-specific hardcoding or custom forks.
- Keep `tool` packages DRY and injection-safe via thin `argv` forwarding (`execve` argument arrays, no `sh -c`), reusing `network_domains` validation and existing `SecretStore` (`ADR-0051`).
- Deliver verified blueprints for `gemini-api`, `colab-compute`, thin `alan-arena`, and native pure-Rust `gdrivefs` at `/mnt/gdrive`.
- Record the final architectural disposition for Issue #1166 (Mirage).

### Non-Goals
- No replacement of Alan's first-party Vertex AI provider (`crates/llm`/`llmfs`) with `gemini-api`.
- No integration of `gemini-api environments connect/poll/serve` self-hosted workers.
- No replacement of Alan's Kernel, aP protocol, OS sandbox, or Agent recovery with Mirage (`reach="process"` / `reach="remote"` bypass virtual gates).
- No ambient Host directory scanning (`ADR-0052`), no macOS Keychain access from inside confined subprocesses, and no raw credentials in manifests or namespace files.

## 1. Universal Community Multi-Skill Intake (`q install`)

`crates/service-manager/src/package/materializer_exports.rs` currently discovers nested `SKILL.md` and top-level `skills/*.md` exports. To ingest standard third-party multi-skill/plugin repositories (such as `poteto/pstack`, `agentskills.io`, or Claude/Cursor plugin layouts) cleanly:

1. **Tolerant Frontmatter Parsing**: Unknown third-party YAML frontmatter keys in `SKILL.md` (for example plugin-specific routing tags or model hints) are parsed safely as non-authoritative metadata rather than failing strict validation, while preserving Alan's canonical `name`, `description`, and typed `dependencies`.
2. **Command Markdown Ingestion (`commands/*.md`)**: Alongside `skills/*.md`, `q install` on manifest-less or `kind = "skill"` bundles ingests top-level `commands/*.md` files as materialized `skills/<id>/SKILL.md` entries when no collision occurs, preserving deterministic `name_to_id` naming.
3. **Preserved Shared Package Assets**: Read-only package-level resource directories (`playbooks/`, `references/`, `templates/`, `assets/`) referenced by imported skills are preserved under materialized package content (bounded by existing snapshot byte/file limits) so relative paths resolve safely inside `/lib/pkg/<pkg>/` without scanning Host directories.
4. **Foreign Vocabulary Adapter Note**: Extend `detect_availability_dependencies` with concise guidance mapping common foreign workflow placeholders (`Read`/`Edit`/`Bash`/`Subagent`) to Alan primitives alongside existing `WebSearch`/`WebFetch`/`TeamCreate`/`TaskCreate` dependency gates.
5. **Unchanged Boundary (`ADR-0052`)**: Intake accepts explicit readable namespace snapshots only—no ambient `.cursor`/`.claude` scanning and no remote Git/HTTP URLs inside Package Service.

## 2. Thin Alan-Native `alan-arena` Skill

Rather than maintaining a forked playbook suite, Alan ships one minimal `kind = "skill"` package (`alan-arena`) that composes orthogonally with imported community playbooks (such as `pstack`'s `how` / `architect` / `prove-it`) or works standalone:

- **Parallel Multi-Provider Contenders**: Uses Alan's concurrent `spawn_subagent` batch with `read_only = true` and explicit `model` overrides across the user's configured providers (for example Codex/OpenAI, Vertex/Gemini, OpenRouter) to produce independent architectural sketches or root-cause analyses.
- **Independent Verifier Adjudication**: Passes contender outputs (never raw authority) to an independent read-only verifier subagent for side-by-side scoring against explicit acceptance criteria.
- **M8 Alignment**: Each contender and verifier naturally maps to an auto-collapsing `one_off` workstream when `add-project-workstreams` is active.

## 3. Governed `tool` Platform Primitives (`network_domains`, `required_secrets`, Thin `argv`)

### Manifest Extensions (`RawToolManifest` in `manifest.rs`)
- **Reuse `network_domains`**: Add `network_domains: Vec<String>` to `RawToolManifest`, sharing the exact domain-validation helper already used by `RawFileserverManifest`. When `allow_network = true`, confined subprocesses receive outbound access restricted to `network_domains` (an empty list with `allow_network = true` fails validation closed).
- **Named Secret References (`required_secrets`)**:
  ```toml
  [[tools.required_secrets]]
  env = "GEMINI_API_KEY"
  credential_id = "gemini-api-key"
  ```
  Or for file-backed scoped ADC credentials:
  ```toml
  [tools.required_adc]
  env = "GOOGLE_APPLICATION_CREDENTIALS"
  credential_id = "colab-adc"
  ```
  - Manifests NEVER contain raw tokens or key paths.
  - At invocation time, Host adapters (`crates/os-host/src/secret_store.rs`) read the named secret from `HostStorePaths` (`secrets.toml`) and inject it exclusively into the confined tool child environment (or a short-lived ephemeral 0600 file inside instance `tmp/` for ADC JSON). Confined subprocesses never read macOS Keychain.
  - Missing credentials fail deterministically with an explicit `alan connection` / `alan q` setup hint.
- **Thin `argv` Forwarding (DRY & Injection-Free)**:
  - `ToolExecutionHints` supports `arguments = "argv"` in addition to `"json_first_arg"`.
  - Tool manifests expose `{ "args": { "type": "array", "items": { "type": "string" } } }` (plus optional `stdin` text), executed directly via `execve` (`Command::args`) without invoking `sh -c`.
  - Subcommands, flags, and usage patterns live exclusively in the package's bundled Companion Skill (`skills/<id>/SKILL.md`), avoiding brittle per-flag schema duplication as upstream CLIs evolve.

## 4. Reference `tool` Packages: `gemini-api` & `colab-compute`

### `gemini-api` (Absorbs Issue #1154)
- **Kind**: `tool` + Companion Skill (`skills/gemini-api/SKILL.md`), `SkillsOnly` default on `q install`, activated via `alan q enable gemini-api --allow-network` or `--package gemini-api`.
- **Execution**: Thin `argv` forwarding to pinned host `gemini-api` binary (always enforcing `--no-interactive`; prefers `--agent-mode` / `--dry-run` / `whoami`).
- **Egress & Auth**: `network_domains = ["generativelanguage.googleapis.com"]`, `required_secrets = [{ env = "GEMINI_API_KEY", credential_id = "gemini-api-key" }]`.
- **Context Discipline**: Media (`image`, `tts`, `music`, `video`) and `transcribe` write artifacts to explicit granted workspace/scratch paths and return compact file paths or JSON metadata; long async `deep-research-*` runs can hand off summaries into M8 project `inbox/`.
- **CLI Design Borrowings**: Track adopting `gemini-api-cli`'s offline `--dry-run`, layered exit codes (`0/1/2/3`), and `--help`/turn discoverability eval harness (`evals/harness-comparison`) for Alan's own non-interactive CLI.

### `colab-compute` (Google AI Ultra GPU & Drive-Mounted Compute)
- **Kind**: `tool` + Companion Skill (`skills/colab-compute/SKILL.md`).
- **Execution**: Thin `argv` forwarding to `colab` (`googlecolab/google-colab-cli`) forced into `--auth=adc` so confined subprocesses never call Python `keyring` against macOS Keychain.
- **Verified Egress & Auth**:
  - `network_domains = ["colab.research.google.com", "colaboratory.googleapis.com", "colab.pa.googleapis.com", "oauth2.googleapis.com", "www.googleapis.com"]`.
  - `required_adc = { env = "GOOGLE_APPLICATION_CREDENTIALS", credential_id = "colab-adc" }` initialized with verified ADC scopes (`openid`, `https://www.googleapis.com/auth/cloud-platform`, `https://www.googleapis.com/auth/userinfo.email`, `https://www.googleapis.com/auth/colaboratory`, optional `drive.file`).
- **Cost & Lifecycle Guardrails**:
  - Companion Skill requires checking `colab usage` before GPU allocation and explicit user confirmation before provisioning paid/accelerated runtimes (`L4`/`A100`/`H100`/`TPU`).
  - Defaults to ephemeral `colab run` (provision -> execute -> collect stdout/artifacts -> auto-teardown); `--keep` requires explicit user request.

## 5. Native Pure-Rust `gdrivefs` at `/mnt/gdrive` & Mirage Verdict (#1166)

### `gdrive-fileserver` Architecture (`crates/service-manager/src/gdrivefs/` + `alan __fileserver-gdrive`)
Mirrors `github-fileserver` (`file-server-plugin-contract`) as a pure-Rust `kind = "fileserver"` package over framed stdio `alan-ap`:

- **Activation & Auth**:
  - Serves `/srv/gdrive` mounted explicitly at `/mnt/gdrive` only upon `alan q enable gdrive-fileserver [--access read-only|read-write] --allow-network` or `--fileserver` (`SkillsOnly` default on `q install`).
  - `network_domains = ["www.googleapis.com", "oauth2.googleapis.com"]`.
  - Reuses `crates/auth/src/google.rs` Desktop PKCE OAuth + single-flight refresh persisted in Host Store (`drive.readonly` default; scoped `drive.file` required before `ReadWrite` mutations are admitted). Raw OAuth tokens never appear in namespace files.
- **Namespace Tree (`/mnt/gdrive`)**:
  - `status` (RO JSON): active OAuth scope, Drive change cursor, quota/usage summary, rate-limit state.
  - `ctl` (commit-on-clunk): `sync`, `refresh <file_id>`, `pin <folder_id>`.
  - `by-id/<file_id>/{meta.json,content}`: binary range reads (`alt=media` with revision/ETag verification) and automatic Google Workspace `files.export` (`Docs -> text/markdown`, `Sheets -> text/csv`, `Slides -> text/plain`, `Apps Script -> application/json`, failing explicitly if exceeding Drive API's 10 MB export ceiling).
  - `files/<path...>`: lazy cached directory tree projection with deterministic duplicate-filename disambiguation (`<name>__<short_id>`).
  - `search/<query>`: read-only server-side Drive query returning bounded JSON hits.
  - `uploads/new`: available only when mounted `ReadWrite` with `drive.file` scope; commit-on-clunk upload. Destructive permanent delete and empty-trash verbs are omitted and fail closed.
- **Untrusted Data Rule**: All content read from `/mnt/gdrive` is strictly non-authoritative external data that cannot alter trust posture, mount grants, or policy.

### Final Architecture Verdict on Mirage (Issue #1166)
1. **No second OS / sandbox**: Alan retains sole authority over Kernel, `aP`, OS Sandbox, Host Mounts, and Agent recovery; Mirage's `LocalRuntime`/`RemoteRuntime` (`reach="process"`/`"remote"`) bypass virtual gates and are not adopted.
2. **No Python/TS Drive sidecar required**: Google Drive is served natively by pure-Rust `gdrivefs` following `githubfs`, avoiding cross-runtime GIL/async/session duplication.
3. **Deferred optional items**: `mirage-rs` pure byte-stream Unix pipes (`grep`/`jq`/`sort` over virtual `aP` streams) remain an optional, standalone narrow experiment (`issue #1166 §6`) only if virtual-tree querying becomes a recurring bottleneck; Mirage's structured pre-exec `explain` output is retained purely as governance UX inspiration.

## Phased Delivery Plan

- **Phase 1 — Universal Community Skill Intake + `alan-arena`**:
  - Enhance `materializer_exports.rs` for tolerant frontmatter, `commands/*.md`, preserved `playbooks/`/`references/` assets, and foreign vocabulary adapter notes; bundle `alan-arena` skill package.
- **Phase 2 — `tool` `network_domains`, Host Store Secrets & Thin `argv` Forwarding**:
  - Extend `RawToolManifest` + `ToolPackageManifest` + Host adapter env/ADC injection from `SecretStore`; add `execve` `argv` execution path inside OS confinement.
- **Phase 3 — Reference `tool` Packages (`gemini-api`, `colab-compute`) & CLI Evals**:
  - Ship `gemini-api` and `colab-compute` packages + Companion Skills; add `--dry-run`/discoverability eval checks.
- **Phase 4 — Native Pure-Rust `gdrivefs` at `/mnt/gdrive`**:
  - Implement `crates/service-manager/src/gdrivefs/`, `alan __fileserver-gdrive`, Host-owned Drive OAuth in `crates/auth`, and `crates/service-manager/packages/gdrive-fileserver/`.

## Risks / Trade-offs

| Risk | Mitigation |
| :--- | :--- |
| Host CLI binary (`gemini-api` or `colab`) missing on user machine | `q enable` / preflight reports an honest actionable install diagnostic; `SkillsOnly` install never fails on missing host binaries. |
| Confined subprocess fails trying to read macOS Keychain | `required_secrets` / `required_adc` inject credentials solely from Host Store; `colab-compute` enforces `--auth=adc`; `gemini-api` runs with explicit `GEMINI_API_KEY` env. |
| Large Google Workspace export (>10 MB) or duplicate Drive filenames | `files.export` checks size limits and fails closed with clear diagnostic; `files/` path projection disambiguates collisions via `<name>__<short_id>` while `by-id/<file_id>` stays canonical. |
| Unintended cloud GPU or Gemini API spend | Default `SkillsOnly` at install; require `--allow-network` on `q enable`; Companion Skills mandate `--dry-run` / `colab usage` checks and explicit confirmation before paid GPU or long media/research runs. |
