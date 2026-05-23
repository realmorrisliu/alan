import { describe, expect, test } from "bun:test";
import {
  ADVANCED_PROVIDER_CATALOG,
  applySetupDefaults,
  browserLoginProfileIdForSetup,
  buildConnectionsContent,
  buildConfigContent,
  buildHostConfigContent,
  configForSetupSelection,
  DEFAULT_CONFIG,
  isConfigurableSetupOption,
  SERVICE_CATALOG,
} from "./setup-catalog.js";

function requireServicePreset(key: string) {
  const option = SERVICE_CATALOG.find((entry) => entry.key === key);
  expect(option).toBeDefined();
  expect(option && isConfigurableSetupOption(option)).toBe(true);
  if (!option || !isConfigurableSetupOption(option)) {
    throw new Error(`Missing configurable service preset: ${key}`);
  }
  return option;
}

function requireAdvancedPreset(key: string) {
  const option = ADVANCED_PROVIDER_CATALOG.find((entry) => entry.key === key);
  expect(option).toBeDefined();
  if (!option) {
    throw new Error(`Missing advanced preset: ${key}`);
  }
  return option;
}

describe("service-first setup catalog", () => {
  test("includes popular services and an explicit advanced handoff", () => {
    const keys = SERVICE_CATALOG.map((entry) => entry.key);
    expect(keys).toContain("chatgpt_codex");
    expect(keys).toContain("kimi_coding");
    expect(keys).toContain("deepseek");
    expect(keys).toContain("openrouter");
    expect(keys).toContain("advanced_custom");
  });

  test("ChatGPT / Codex preset writes canonical managed-login config", () => {
    const option = requireServicePreset("chatgpt_codex");
    const agentConfig = buildConfigContent(option, applySetupDefaults(DEFAULT_CONFIG, option));
    const connectionsConfig = buildConnectionsContent(
      option,
      applySetupDefaults(DEFAULT_CONFIG, option),
    );

    expect(option.provider).toBe("chatgpt");
    expect(option.fields.map((field) => field.key)).toEqual([
      "chatgpt_model",
      "chatgpt_account_id",
    ]);
    expect(agentConfig).not.toContain("connection_profile =");
    expect(connectionsConfig).toContain('default_profile = "chatgpt-main"');
    expect(connectionsConfig).toContain("[credentials.chatgpt-main]");
    expect(connectionsConfig).toContain('provider_family = "chatgpt"');
    expect(connectionsConfig).toContain('base_url = "https://chatgpt.com/backend-api/codex"');
    expect(connectionsConfig).toContain('model = "gpt-5.3-codex"');
    expect(connectionsConfig).toContain('account_id = ""');
  });

  test("ChatGPT / Codex preset requests automatic browser login", () => {
    const option = requireServicePreset("chatgpt_codex");

    expect(browserLoginProfileIdForSetup(option)).toBe("chatgpt-main");
  });

  test("API-key presets do not request automatic browser login", () => {
    const option = requireServicePreset("openai_api_platform");

    expect(browserLoginProfileIdForSetup(option)).toBeNull();
  });

  test("ChatGPT / Codex preset writes explicit account binding when provided", () => {
    const option = requireServicePreset("chatgpt_codex");
    const rendered = buildConnectionsContent(option, {
      ...applySetupDefaults(DEFAULT_CONFIG, option),
      chatgpt_account_id: "acct_123",
    });

    expect(rendered).toContain('account_id = "acct_123"');
  });

  test("OpenAI API Platform preset maps to OpenAI Responses without exposing base URL", () => {
    const option = requireServicePreset("openai_api_platform");
    expect(option.provider).toBe("openai_responses");
    expect(option.fields.map((field) => field.key)).toEqual([
      "openai_responses_api_key",
      "openai_responses_model",
    ]);
  });

  test("OpenRouter preset writes canonical compatible config with OpenRouter defaults", () => {
    const option = requireServicePreset("openrouter");
    const config = applySetupDefaults(DEFAULT_CONFIG, option);
    const agentConfig = buildConfigContent(option, config);
    const connectionsConfig = buildConnectionsContent(option, config);

    expect(option.provider).toBe("openai_chat_completions_compatible");
    expect(option.fields.some((field) => field.key.includes("base_url"))).toBe(false);
    expect(agentConfig).not.toContain("connection_profile =");
    expect(connectionsConfig).toContain('base_url = "https://openrouter.ai/api/v1"');
    expect(connectionsConfig).toContain('model = "openai/gpt-5.2"');
    expect(agentConfig).not.toContain("[[skill_overrides]]");
    expect(agentConfig).not.toContain('bind_address = "127.0.0.1:8090"');
  });

  test("service preset keeps its model default when the model field is left blank", () => {
    const option = requireServicePreset("kimi_coding");
    const rendered = buildConnectionsContent(option, {
      ...applySetupDefaults(DEFAULT_CONFIG, option),
      openai_chat_completions_compatible_model: "",
    });

    expect(rendered).toContain('model = "kimi-k2-0905-preview"');
  });

  test("switching to a different service preset resets shared credentials", () => {
    const openrouter = requireServicePreset("openrouter");
    const deepseek = requireServicePreset("deepseek");
    const switched = configForSetupSelection(
      {
        ...applySetupDefaults(DEFAULT_CONFIG, openrouter),
        openai_chat_completions_compatible_api_key: "sk-openrouter",
      },
      openrouter,
      deepseek,
    );

    expect(switched.openai_chat_completions_compatible_api_key).toBe("");
    expect(switched.openai_chat_completions_compatible_base_url).toBe(
      "https://api.deepseek.com/v1",
    );
    expect(switched.openai_chat_completions_compatible_model).toBe("deepseek-chat");
  });

  test("re-entering the same preset preserves its in-progress field values", () => {
    const kimi = requireServicePreset("kimi_coding");
    const preserved = configForSetupSelection(
      {
        ...applySetupDefaults(DEFAULT_CONFIG, kimi),
        openai_chat_completions_compatible_api_key: "sk-kimi",
        openai_chat_completions_compatible_model: "kimi-custom",
      },
      kimi,
      kimi,
    );

    expect(preserved.openai_chat_completions_compatible_api_key).toBe("sk-kimi");
    expect(preserved.openai_chat_completions_compatible_model).toBe("kimi-custom");
    expect(preserved.openai_chat_completions_compatible_base_url).toBe(
      "https://api.moonshot.cn/v1",
    );
  });

  test("advanced compatible setup exposes base URL for manual endpoint control", () => {
    const option = requireAdvancedPreset("advanced_openai_chat_completions_compatible");
    const rendered = buildConnectionsContent(option, applySetupDefaults(DEFAULT_CONFIG, option));

    expect(option.fields.map((field) => field.key)).toEqual([
      "openai_chat_completions_compatible_base_url",
      "openai_chat_completions_compatible_api_key",
      "openai_chat_completions_compatible_model",
    ]);
    expect(rendered).toContain('base_url = "https://api.openai.com/v1"');
  });

  test("host config content uses canonical split host file shape", () => {
    const rendered = buildHostConfigContent();

    expect(rendered).toContain("# alan Host Configuration");
    expect(rendered).toContain('bind_address = "0.0.0.0:8090"');
    expect(rendered).toContain('daemon_url = "http://127.0.0.1:8090"');
  });

  test("dev host config content uses dev daemon defaults", () => {
    const rendered = buildHostConfigContent("dev");

    expect(rendered).toContain('bind_address = "127.0.0.1:8091"');
    expect(rendered).toContain('daemon_url = "http://127.0.0.1:8091"');
  });

  test("dev agent config comments point at dev channel global paths", () => {
    const option = requireServicePreset("chatgpt_codex");
    const rendered = buildConfigContent(option, applySetupDefaults(DEFAULT_CONFIG, option), "dev");

    expect(rendered).toContain("~/.alan-dev/connections.toml");
    expect(rendered).toContain("~/.agents-dev/skills/");
    expect(rendered).not.toContain("~/.alan/connections.toml");
  });
});
