# Live Provider Harness

> Status: current implementation guide for opt-in live provider protocol validation.

## Purpose

This harness validates alan's real upstream protocol paths against live providers
without making those tests part of the default CI-safe suite.

It is intentionally narrow:

1. basic non-streaming generation,
2. streaming generation completion,
3. server-managed continuation where the provider declares support.

The shared harness request intentionally mirrors alan's normal runtime baseline
by setting a small `max_tokens` cap and the default `temperature` value.

Code-level adapter tests still own detailed payload-shape assertions. The live
harness exists to catch real upstream drift, auth issues, and request-contract
breakage that mocks cannot detect.

## Test Entry Point

Integration test file:

```text
crates/llm/tests/live_provider_harness.rs
```

Runner script:

```bash
bash scripts/live-provider-harness.sh
```

Just entrypoint:

```bash
just live-providers
```

## Safety Model

The harness is protected in two ways:

1. every test is marked `#[ignore]`, so normal `cargo test` does not run it;
2. the runner also requires `ALAN_LIVE_PROVIDER_TESTS=1`.

This prevents accidental live API usage during normal local development or CI.

## Supported Providers

### OpenAI Responses

Required:

- `ALAN_LIVE_OPENAI_RESPONSES_API_KEY`

Optional:

- `ALAN_LIVE_OPENAI_RESPONSES_BASE_URL`
- `ALAN_LIVE_OPENAI_RESPONSES_MODEL`

### ChatGPT Managed Responses

Required:

- `ALAN_LIVE_CHATGPT_AUTH_STORAGE_PATH`

Optional:

- `ALAN_LIVE_CHATGPT_BASE_URL`
- `ALAN_LIVE_CHATGPT_MODEL`
- `ALAN_LIVE_CHATGPT_ACCOUNT_ID`

Notes:

1. the auth storage path must end with `auth.json`;
2. this path should point at a real managed-login credential store created by
   alan's ChatGPT auth flow.

### OpenAI Chat Completions

Required:

- `ALAN_LIVE_OPENAI_CHAT_COMPLETIONS_API_KEY`

Optional:

- `ALAN_LIVE_OPENAI_CHAT_COMPLETIONS_BASE_URL`
- `ALAN_LIVE_OPENAI_CHAT_COMPLETIONS_MODEL`

### OpenAI Chat Completions Compatible

Required:

- `ALAN_LIVE_OPENAI_CHAT_COMPATIBLE_API_KEY`
- `ALAN_LIVE_OPENAI_CHAT_COMPATIBLE_BASE_URL`

Optional:

- `ALAN_LIVE_OPENAI_CHAT_COMPATIBLE_MODEL`

### Anthropic Messages (protocol adapter; not a first-party provider, ADR-0061)

Required:

- `ALAN_LIVE_ANTHROPIC_MESSAGES_API_KEY`

Optional:

- `ALAN_LIVE_ANTHROPIC_MESSAGES_BASE_URL`
- `ALAN_LIVE_ANTHROPIC_MESSAGES_MODEL`
- `ALAN_LIVE_ANTHROPIC_MESSAGES_CLIENT_NAME`
- `ALAN_LIVE_ANTHROPIC_MESSAGES_USER_AGENT`

## What The Harness Asserts

For each configured provider:

1. a live non-streaming request returns the expected token;
2. the unified response includes `finish_reason`;
3. providers that declare `supports_provider_response_id` surface a non-empty id;
4. providers that declare `supports_provider_response_status` surface a non-empty status;
5. a live streaming request emits the expected token and a terminal chunk;
6. only providers that still declare
   `supports_server_managed_continuation=true` complete a live continuation
   turn using `previous_response_id`.

Current live-verified note:

1. On April 13, 2026, the managed `chatgpt` surface was observed to require
   `stream=true`, require `store=false`, reject `temperature`, reject
   `max_output_tokens`, and reject `previous_response_id`, so alan no longer
   runs continuation assertions for `chatgpt`.

## Recommended Usage

Run all configured providers:

```bash
ALAN_LIVE_PROVIDER_TESTS=1 \
bash scripts/live-provider-harness.sh
```

Run a single ignored test directly:

```bash
ALAN_LIVE_PROVIDER_TESTS=1 \
cargo test -p alan-llm --test live_provider_harness live_chatgpt_contract -- --ignored --nocapture
```

## Recommended CI Shape

Use this harness in an opt-in nightly or manually triggered workflow:

1. inject provider secrets only in the live job,
2. run the harness script,
3. keep the job outside the default required check set unless the org is ready
   to pay for and manage live upstream flakiness.

The normal required suite should remain:

1. unit tests,
2. mocked integration tests,
3. repo-local harness scenarios.
