# Tasks

## 1. Stage 1 — TUI `/issue` (`/bug`) intake
- [ ] 1.1 Implement a strictly redacted diagnostic summary collector in `crates/tui` capturing Alan version, OS/arch, active provider/model label, permission posture, and latest error/notice summary without leaking transcripts, workspace source, or secrets.
- [ ] 1.2 Add `/issue` (with `/bug` alias) in `crates/tui/src/file_backed/app/slash.rs` with an interactive editable preview and explicit confirmation/cancel flow.
- [ ] 1.3 Wire tiered submission: write `/mnt/github/repos/<owner>/<repo>/issues/new` when mounted writable -> invoke authenticated `gh issue create` via Host adapter -> generate prefilled GitHub web URL fallback.
- [ ] 1.4 Add unit tests (redaction, cancel safety, fallback tier selection) and real-PTY tests in `crates/tui`.

## 2. Stage 2 — Self-iteration `skill` and bounded `alan workflow`
- [ ] 2.1 Author a Quartermaster `skill` encoding Alan's engineering loop (untrusted issue intake -> isolated worktree off `origin/main` -> reproduction test -> minimal fix -> `./scripts/check-rust-source-size.sh`, `just quality`, `cargo test --workspace` -> PR creation -> CI & review resolution without auto-merge).
- [ ] 2.2 Add a bounded `alan workflow` manifest template enforcing control-plane repository, actor, and label (`alan:auto`) allowlists with explicit `--grant` bindings.
- [ ] 2.3 Verify the workflow end-to-end on isolated repository/issue fixtures, confirming non-allowlisted actors and prompt-injection payloads are ignored.

## 3. Stage 3 — Quartermaster `fileserver` kind and `githubfs` + `routefs` integration
- [ ] 3.1 Extend `PackageExportKind` and manifest parsing in `crates/service-manager/src/package/` to support the fifth kind `fileserver` cleanly with zero install-time authority or auto-start.
- [ ] 3.2 Add Service Manager confined process supervision speaking `aP` wire framing (`WireTransportClient` over `stdio`/`unix_socket`), `/srv/<id>` publication, and Kernel-enforced `ReadOnly`/`ReadWrite` mount binding.
- [ ] 3.3 Implement the reference `githubfs` package exporting `ctl`, `status`, append-only NDJSON `events` `Stream`, `repos/<owner>/<repo>/issues/...`, and `pulls/...` with Host-adapter credentials and no default merge/release verbs.
- [ ] 3.4 Compose `/mnt/github/events` with `routefs` (`/mnt/route`) inspectable rules, event deduplication, self-actor suppression, concurrency caps, and per-issue retry limits under opt-in `alan workflow` / explicit schedule execution.
- [ ] 3.5 Verify confinement, partial-write clunk semantics, loop guards, and bare-`alan` lifecycle boundaries with integration tests.

## 4. Stage 4 — Human-gated release preparation
- [ ] 4.1 Implement a bounded release-preparation workflow/subagent that computes commits since the latest tag, drafts version bumps + changelog updates in an isolated worktree, runs release verification, and opens a release PR.
- [ ] 4.2 Verify merge, git tag push, and artifact publication cannot be executed autonomously by the workflow, and document the post-release `alan update` close-out step.

## 5. Verification, PR review, and archive readiness
- [ ] 5.1 Run `just openspec-check`, `just quality`, `./scripts/check-rust-source-size.sh`, and `cargo test --workspace` across implemented stages, updating `manpages` and user docs cleanly.
- [ ] 5.2 Complete PR review (including Codex review and 16/16 CI checks) per milestone slice without self-merging prematurely.
- [ ] 5.3 After all stages are merged to `main`, sync canonical spec deltas into `openspec/specs/` (`rust-inline-tui`, `package-management-contract`, and new `file-server-plugin-contract`), verify `just openspec-check`, and move this change to `openspec/changes/archive/YYYY-MM-DD-enable-self-iteration-loop/`.
