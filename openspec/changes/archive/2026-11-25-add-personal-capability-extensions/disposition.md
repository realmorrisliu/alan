# Disposition

Status: planned (independent Quartermaster extension ecosystem track; non-blocking to Qualification and M8).

- This change consolidates the accepted design decisions from the 2026-11-25
  personal capability extensions alignment:
  1. Universal community multi-skill bundle intake via `q install` (e.g., `poteto/pstack`, `agentskills.io`, Claude/Cursor plugin repositories);
  2. Thin Alan-native `alan-arena` multi-provider bakeoff skill;
  3. Governed `tool` packages with `network_domains`, Host Store secret/ADC injection (`ADR-0051`), and thin `argv` forwarding, instantiated for `gemini-api` (absorbing Issue #1154) and `colab-compute` (`google-colab-cli`);
  4. Native pure-Rust `gdrivefs` fileserver at `/mnt/gdrive` alongside the final architecture verdict on Mirage (Issue #1166).
- Execution proceeds independently of `qualify-personal-daily-development` and `add-project-workstreams`.
