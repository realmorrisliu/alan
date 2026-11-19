# Anthropic Provider Package (`anthropic-provider`)

Extension provider package enabling the `anthropic_messages` protocol adapter in Alan via
Quartermaster (`q`). Built-in first-party defaults remain `magpie`, `openai_responses`,
`chatgpt`, `google_gemini_generate_content`, and `openrouter`.

## Install

```bash
alan q install crates/service-manager/packages/anthropic-provider
```

## Configure Connection Profile + Host Credential

```bash
alan connection add pkg:anthropic-provider --profile claude-main
alan connection set-secret claude-main
alan connection default set claude-main
alan connection test claude-main
```

API secrets are stored strictly in Alan Host Store (`SecretStore`), never inside this package.
Uninstalling `anthropic-provider` via `alan q uninstall anthropic-provider` revokes callable
availability cleanly on the next Connection Service refresh.
