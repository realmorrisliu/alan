---
name: colab-compute
description: |
  Run remote Google Colab / Google AI Ultra GPU and TPU workloads via the
  governed `colab_compute` tool (`google-colab-cli` with forced `--auth=adc`),
  enforcing `colab usage` pre-checks, explicit confirmation before paid GPU
  provisioning, and ephemeral-by-default `colab run` execution.
metadata:
  short-description: Governed google-colab-cli wrapper with ADC isolation and GPU cost guardrails
  tags: [colab, gpu, tpu, compute, adc, google-ai-ultra, tool]
capabilities:
  required_tools: [read_file]
---

# Google Colab Compute (`colab-compute`)

Companion Skill bundled with the `colab-compute` Quartermaster `tool` package.
Reading this skill grants **no** runtime, network, or credential authority: the
operator must explicitly store `colab-adc` and activate `colab-compute`.

## Credential & Network Boundary

- **Forced `--auth=adc`**: `bin/colab-compute` enforces `--auth=adc` so confined subprocesses never invoke Python `keyring` against macOS Keychain.
- **Ephemeral `0600` ADC Projection**: Alan resolves `credential_id = "colab-adc"` from Host Store (`SecretStore`) and projects `GOOGLE_APPLICATION_CREDENTIALS` into a short-lived `0600` file unlinked on completion.
- **Verified ADC Scopes**: `openid`, `https://www.googleapis.com/auth/cloud-platform`, `https://www.googleapis.com/auth/userinfo.email`, `https://www.googleapis.com/auth/colaboratory`, and optional `https://www.googleapis.com/auth/drive.file` when Drive-mounted input/output is needed.
- **Egress Allowlist**: Outbound network is restricted strictly to `colab.research.google.com`, `colaboratory.googleapis.com`, `colab.pa.googleapis.com`, `oauth2.googleapis.com`, and `www.googleapis.com`.

## Operator Setup

If `colab_compute` is inactive or `colab-adc` is missing, guide the operator to run:

```bash
cat /path/to/application_default_credentials.json | alan q set-secret colab-adc
alan q enable colab-compute --allow-network   # or: alan --package colab-compute
```

## Non-Negotiable Cost & Lifecycle Guardrails

1. **Mandatory `colab usage` Pre-Check**: Always run `colab usage` before allocating or provisioning any GPU/TPU runtime so the operator sees current compute-unit balance and active sessions:
   ```json
   { "args": ["usage"] }
   ```
2. **Explicit User Confirmation Before Paid GPU Provisioning**: Never provision paid or accelerated runtimes (`L4`, `A100`, `H100`, `TPU`, or higher-tier accelerators) without first presenting the `colab usage` check, target accelerator, and expected job scope and receiving explicit user confirmation.
3. **Ephemeral-by-Default `colab run`**: Default to ephemeral `colab run` (provision -> execute -> collect stdout/artifacts -> auto-teardown). Never pass `--keep` or leave a paid runtime allocated unless the user explicitly asks to keep the session alive.

## Invoking `colab_compute` (`argv` Forwarding)

Pass exact CLI arguments via `{"args": [...], "stdin": "..."}` (never shell strings). `bin/colab-compute` prepends `--auth=adc` automatically and executes `colab` directly via `execve`:

```json
{ "args": ["usage"] }
```

```json
{ "args": ["run", "--accelerator", "T4", "python train_probe.py"] }
```

- Check `colab --help` or `colab run --help` for subcommand flags before inventing flags.
- Tear down any explicitly kept runtime (`colab runtimes delete` / stop) once the user's task finishes.
