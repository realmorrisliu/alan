# Live Runtime Smoke

This opt-in layer drives a real Agent Execution Engine turn against a configured
provider. It catches runtime-level request shaping, memory, and persistence
regressions that provider-adapter tests cannot see.

## Safety

- tests are `#[ignore]`;
- the runner requires `ALAN_LIVE_PROVIDER_TESTS=1`;
- credentials are supplied through explicit live-test environment variables.

Current managed ChatGPT coverage verifies startup, turn completion, assistant
text, Process-to-Process memory continuity, handoff continuity, and absence of
provider errors.

```bash
ALAN_LIVE_PROVIDER_TESTS=1 \
bash scripts/live-runtime-smoke.sh
```

Required for managed ChatGPT:

```text
ALAN_LIVE_CHATGPT_AUTH_STORAGE_PATH
```

Optional overrides:

```text
ALAN_LIVE_CHATGPT_BASE_URL
ALAN_LIVE_CHATGPT_MODEL
ALAN_LIVE_CHATGPT_ACCOUNT_ID
```
