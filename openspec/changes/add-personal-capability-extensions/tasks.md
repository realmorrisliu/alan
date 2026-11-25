# Tasks

## 1. Phase 1 — Universal Community Skill Intake and `alan-arena`

- [x] 1.1 Extend `crates/service-manager/src/package/materializer_exports.rs` to accept unknown third-party frontmatter keys safely, ingest `commands/*.md` alongside `skills/`, preserve shared package assets (`playbooks/`, `references/`), and extend foreign tool vocabulary guidance without ambient Host scanning.
- [x] 1.2 Add unit and integration tests installing representative multi-skill community bundle fixtures (such as `pstack`-shaped layouts) via `alan q install` and verifying `/lib/pkg/` projections and `SkillsOnly` zero-authority behavior.
- [x] 1.3 Add the thin `alan-arena` preinstalled `skill` package (`crates/service-manager/packages/alan-arena/`) orchestrating parallel read-only `spawn_subagent` contenders across distinct models and independent verifier scoring.

## 2. Phase 2 — Governed `tool` Platform Primitives (`network_domains`, Host Store Secrets, Thin `argv`)

- [x] 2.1 Extend `RawToolManifest` and `ToolPackageManifest` (`crates/service-manager/src/package/manifest.rs`, `crates/agent-engine/src/runtime/tool_packages.rs`) with `network_domains`, `required_secrets` / `required_adc` references (rejecting raw secrets), and `arguments = "argv"` execution hints.
- [x] 2.2 Implement Host Store secret env injection and ephemeral `0600` ADC file projection in `alan-os-host` / confined tool execution (`ADR-0051`) plus OS-sandbox `network_domains` enforcement and `execve` `argv` forwarding (no `sh -c`).
- [x] 2.3 Add security tests proving confined `tool` subprocesses cannot access ambient OS Keychain or unlisted domains, and missing credentials fail deterministically without leaking secret bytes.

## 3. Phase 3 — Reference `tool` Packages (`gemini-api`, `colab-compute`) and CLI Evals

- [x] 3.1 Add `crates/service-manager/packages/gemini-api/` (`alan-package.toml` + `skills/gemini-api/SKILL.md` + thin `argv` wrapper) enforcing `--no-interactive`, `network_domains = ["generativelanguage.googleapis.com"]`, and `GEMINI_API_KEY` secret ref; test dry-run, missing-binary, and missing-key diagnostics.
- [x] 3.2 Add `crates/service-manager/packages/colab-compute/` (`alan-package.toml` + `skills/colab-compute/SKILL.md`) enforcing `--auth=adc`, verified Colab/Google API `network_domains`, `GOOGLE_APPLICATION_CREDENTIALS` projection, `colab usage` pre-check, and ephemeral `colab run`.
- [x] 3.3 Add a CLI discoverability / `--help` turn-budget eval fixture inspired by `gemini-api-cli` (`evals/harness-comparison`) covering `alan q` and non-interactive `alan -p`.

## 4. Phase 4 — Native Pure-Rust `gdrivefs` at `/mnt/gdrive`

- [x] 4.1 Extend `crates/auth/src/google.rs` for Host-owned Google Drive OAuth scopes (`drive.readonly` default, explicit opt-in `drive.file` for `ReadWrite`) stored solely in Host Store.
- [x] 4.2 Implement `crates/service-manager/src/gdrivefs/` (`status`, `ctl`, `by-id/<id>/{meta.json,content}` with binary range reads and Workspace `files.export`, disambiguated `files/<path...>`, `search/<query>`, and commit-on-clunk `uploads/new` gated by `ReadWrite` + `drive.file`), plus hidden `alan __fileserver-gdrive` entrypoint.
- [x] 4.3 Add `crates/service-manager/packages/gdrive-fileserver/` (`alan-package.toml` + Companion Skills + tests covering read-only isolation, `files.export` Markdown/CSV conversion, >10 MB export failure, token redaction, and untrusted-content boundaries).

## 5. Verification, PR Review, and Archive Readiness

- [x] 5.1 Run `just test`, `just quality`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `bash scripts/check-openspec-current-surfaces.sh && openspec validate add-personal-capability-extensions --strict`.
- [x] 5.2 Complete independent security and architecture review against `ADR-0051`, `ADR-0052`, `ADR-0061`, and `file-server-plugin-contract`.
- [ ] 5.3 After implementation PRs merge on `main` and CI passes, sync the four delta specs into `openspec/specs/` and archive the change under `openspec/changes/archive/YYYY-MM-DD-add-personal-capability-extensions/`.
