---
name: gemini-api
description: |
  Use the governed `gemini_api` tool (`gemini-api-cli`) for non-interactive
  media generation (`image`, `tts`, `music`, `video`), document/media analysis
  (`analyze`), audio/video transcription (`transcribe`), Deep Research
  (`agent run`), and reusable Gemini Files uploads.
metadata:
  short-description: Governed non-interactive gemini-api-cli wrapper and usage guide
  tags: [gemini, media, tts, video, image, transcription, deep-research, tool]
capabilities:
  required_tools: [read_file]
---

# Gemini API CLI (`gemini-api`)

Companion Skill bundled with the `gemini-api` Quartermaster `tool` package.
Reading this skill grants **no** runtime, network, or credential authority: the
operator must explicitly store `gemini-api-key` and activate `gemini-api`.

## Scope & Boundaries

- **CLI for shell/media work, NOT Alan's LLM backend**: Do not replace Alan's first-party `vertex` / `google_gemini_generate_content` provider (`alan connection`) with `gemini-api`.
- **Host Store auth only**: `bin/gemini-api` always forces `--no-interactive`, and Alan injects `GEMINI_API_KEY` from Host Store (`credential_id = "gemini-api-key"`). Never run `gemini-api configure`, `gemini-api auth login`, or `explore` inside confinement.
- **No self-hosted worker loops**: Never use `gemini-api environments connect`, `poll`, or `serve` as an Alan worker.
- **Egress allowlist**: Outbound network is confined strictly to `generativelanguage.googleapis.com`.

## Operator Setup

If `gemini_api` is inactive or missing credentials, guide the operator to run:

```bash
alan q set-secret gemini-api-key
alan q enable gemini-api --allow-network   # or: alan --package gemini-api
```

Verify installation and auth source offline/read-only before live spend:

```json
{ "args": ["whoami", "-o", "json"] }
```

## Invoking `gemini_api` (`argv` Forwarding)

Pass exact CLI arguments via `{"args": [...], "stdin": "..."}` (never shell strings). `bin/gemini-api` prepends `--no-interactive` automatically and forwards `args` via direct `execve`.

- **Prefer machine-readable output and dry-run previews**:
  - `--agent-mode`: compact TOON output on stdout and structured JSON errors on stderr.
  - `--dry-run -o json`: validates arguments and prints the redacted HTTP request offline with zero network spend.
  - `-o json` + `--jq <expr>`: filters JSON responses in-process without external `jq`.
- **Exit codes**: `0` ok, `1` runtime/API error, `2` usage/validation error, `3` auth error.

## Command Quick Reference

| Command | Key flags | Behavior & Context Discipline |
|---|---|---|
| `generate [prompt]` | `-m`, `-i`, `--stdin`, `--stream=false -o json`, `--previous-interaction-id`, `--body` | Streams text by default; pass `--stream=false -o json` or `--agent-mode` for structured output. |
| `image [prompt]` | `-m`, `-i`, `--aspect-ratio`, `--image-size`, `--out` | Writes image to `--out` path in granted workspace/scratch and prints compact path / `-o json` metadata. |
| `tts [text]` | `-m`, `-f`, `--stdin`, `-v`/`--voice`, `--multi-speaker`, `--out` | Writes 24 kHz WAV to `--out` and prints path. |
| `music [prompt]` | `-m`, `-f`, `--stdin`, `--out`, `--body` | Writes audio track to `--out` and prints path. |
| `video [prompt]` | `-m gemini-omni-1.2-flash`, `-i`, `--aspect-ratio`, `--resolution`, `--async`, `--resume`, `--out` | Use `--async` + `--resume <id>` for long renders; write artifact to `--out`. |
| `analyze [question]` | `-i` (repeatable), `--stdin`, `--system`, `-m` | Answers questions over local PDFs, media, CSV/text, or `files/<id>` URIs. |
| `transcribe` | `-m`, `-i`, `--format` (`md`/`text`/`json`/`srt`), `--out` | Writes transcript file to `--out` instead of flooding chat context. |
| `files` | `upload <path> --wait`, `list`, `get`, `delete` | Uploads larger or reused files once (`files/<id>`, 48h TTL). |
| `models` | `(bare)`, `list [--all]`, `get <id>` | Bare `models` prints the offline catalog without network calls. |
| `agent` | `run --agent <id> --background`, `status <run_id>`, `cancel` | Long async `deep-research-*` runs can be polled via `agent status` and summarized concisely (e.g. into project `inbox/`). |

## Context & Spend Discipline

1. Always write media (`image`, `tts`, `music`, `video`) and `transcribe` outputs to an explicit `--out` path inside the granted workspace or scratch directory, and report only the file path and compact `-o json` metadata.
2. Never pass `--usage` (dumps a large 45 KB KDL spec); use `<subcommand> --help` for compact command help.
3. For piped plain text into `generate`, `image`, `music`, `video`, `analyze`, or `tts`, pass `"stdin": "..."` together with `"--stdin"` in `args`.
4. Confirm with the user before triggering expensive video renders or Deep Research runs (`deep-research-*`).
