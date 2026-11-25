# Change: Personal Capability Extensions via Quartermaster

## Why

Alan (`alan9`) intentionally keeps its Root Agent minimal—7 core tools, 4 built-in first-party LLM providers (`ADR-0061`), and OS-boundary confinement—while routing extensible capabilities through Quartermaster (`q`) (`package-management-contract`). In daily personal use, four high-leverage capability gaps remain outside the core runtime:

1. **Community engineering skill bundles (`poteto/pstack`, Cursor/Claude plugin repos)**: `q install` already scans nested `SKILL.md` and `skills/*.md`, but real-world multi-skill repositories ship `commands/*.md`, shared repository-level `playbooks/` and `references/` assets, and custom frontmatter fields that currently require manual adaptation or fragile forks.
2. **True multi-provider design bakeoffs (`alan-arena`)**: Upstream skills emulate design bakeoffs inside one model, whereas Alan natively supports multiple configured providers, parallel `spawn_subagent` calls, and independent read-only verifiers.
3. **External multimodal & GPU CLI tools (`gemini-api-cli` / Issue #1154, `google-colab-cli` / Google AI Ultra)**: Confined Seatbelt/Linux subprocesses cannot read macOS Keychain, and `RawToolManifest` currently lacks per-tool `network_domains` allowlisting, named Host Store secret references (`ADR-0051`), and DRY thin `argv` forwarding.
4. **Google Drive projection (`/mnt/gdrive`, Issue #1166 resolution)**: Accessing a user's Google Drive (including Docs/Sheets/Slides export and large binary range reads) belongs in a native pure-Rust `aP` fileserver following `githubfs` (`file-server-plugin-contract`) rather than adopting Mirage as a second OS or sandbox replacement.

## What Changes

- **Universal Community Multi-Skill Intake (`package-management-contract`)**: Extend `q install` snapshot materialization (`materializer_exports.rs`) to ingest `commands/*.md`, preserve shared package-level read-only directories (`playbooks/`, `references/`), accept unknown third-party frontmatter keys as non-authoritative metadata, and append foreign tool vocabulary adapter guidance—strictly over explicit namespace snapshots (`ADR-0052`, zero ambient Host scanning, zero remote URLs).
- **Thin Alan-Native `alan-arena` Skill (`skill-system-contract`)**: Bundle a lightweight pure `kind = "skill"` package composing cleanly with imported playbooks (such as `pstack`) to launch parallel read-only `spawn_subagent` contenders across distinct configured models and adjudicate via an independent read-only verifier.
- **Governed `tool` Packages: `network_domains`, Host Store Secrets & Thin `argv` Forwarding (`package-management-contract`, `os-boundary-security`)**: Extend `RawToolManifest` to reuse `network_domains: Vec<String>`, declare named `required_secrets` (`{ env, credential_id }` or scoped ADC file projection, never raw secret bytes), and support thin `argv` forwarding (`execve` argument arrays, never `sh -c`) alongside bundled Companion Skills. `q install` remains `SkillsOnly` by default.
- **Reference `tool` Packages (`gemini-api` & `colab-compute`)**:
  - `gemini-api`: wraps pinned `gemini-api` binary (`--no-interactive`, `--agent-mode`, `--dry-run`, `whoami`) with `network_domains = ["generativelanguage.googleapis.com"]` and Host Store `GEMINI_API_KEY` injection; preserves non-goals (never replaces `crates/llm`/`llmfs` Vertex AI or integrates self-hosted `environments connect/poll/serve`).
  - `colab-compute`: wraps `google-colab-cli` (`colab --auth=adc`) with verified domains (`colab.research.google.com`, `colaboratory.googleapis.com`, `colab.pa.googleapis.com`, `oauth2.googleapis.com`, `www.googleapis.com`), scoped Host Store ADC projection (`openid`, `cloud-platform`, `userinfo.email`, `colaboratory`, optional `drive.file`), ephemeral-by-default `colab run`, `colab usage` visibility, and explicit confirmation before paid GPU provisioning.
- **Native Pure-Rust `gdrivefs` at `/mnt/gdrive` (`file-server-plugin-contract`)**: Add a `githubfs`-style `kind = "fileserver"` package (`alan __fileserver-gdrive`) serving `/srv/gdrive` -> `/mnt/gdrive` with Host-owned Google OAuth (`crates/auth/src/google.rs`), `drive.readonly` default (opt-in `drive.file` for `ReadWrite`), binary range reads, Drive API v3 `files.export` (`Docs -> text/markdown`, `Sheets -> text/csv`, `Slides -> text/plain`), and read-only search—recording the final verdict on Mirage (Issue #1166).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `package-management-contract`
- `skill-system-contract`
- `os-boundary-security`
- `file-server-plugin-contract`

## Impact

- **Affected specs**: `package-management-contract`, `skill-system-contract`, `os-boundary-security`, `file-server-plugin-contract`.
- **Affected crates**: `alan-service-manager`, `alan-agent-engine`, `alan-os-host`, `alan-auth`, `alan`.
- **Backwards compatibility & security posture**: Existing `skill`, `tool`, `mcp`, `provider`, and `fileserver` packages remain valid; `q install` continues granting zero runtime authority (`SkillsOnly` default); OS confinement and `SecretStore` / `HostStorePaths` ownership (`ADR-0051`, `ADR-0061`) remain strictly enforced.
