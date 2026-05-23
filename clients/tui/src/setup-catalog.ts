import { installChannelDescriptor, type InstallChannelId } from "./install-channel.js";

export type Provider =
  | "chatgpt"
  | "google_gemini_generate_content"
  | "openai_responses"
  | "openai_chat_completions"
  | "openai_chat_completions_compatible"
  | "anthropic_messages";

export interface ConfigValues {
  chatgpt_base_url: string;
  chatgpt_model: string;
  chatgpt_account_id: string;
  google_gemini_generate_content_location: string;
  google_gemini_generate_content_project_id: string;
  google_gemini_generate_content_model: string;
  openai_responses_base_url: string;
  openai_responses_api_key: string;
  openai_responses_model: string;
  openai_chat_completions_base_url: string;
  openai_chat_completions_api_key: string;
  openai_chat_completions_model: string;
  openai_chat_completions_compatible_base_url: string;
  openai_chat_completions_compatible_api_key: string;
  openai_chat_completions_compatible_model: string;
  anthropic_messages_base_url: string;
  anthropic_messages_api_key: string;
  anthropic_messages_model: string;
}

export interface ConfigField {
  key: keyof ConfigValues;
  label: string;
  placeholder: string;
  hint?: string;
}

interface CatalogEntryBase {
  key: string;
  group: "Popular services" | "Official providers" | "Advanced";
  name: string;
  desc: string;
  detail: string;
}

export interface ConfigurableSetupOption extends CatalogEntryBase {
  kind: "preset";
  provider: Provider;
  sectionTitle: string;
  defaults: Partial<ConfigValues>;
  fields: ConfigField[];
}

export interface AdvancedSetupHandoff extends CatalogEntryBase {
  kind: "advanced_handoff";
}

export type ServiceCatalogEntry = ConfigurableSetupOption | AdvancedSetupHandoff;

export const DEFAULT_CONFIG: ConfigValues = {
  chatgpt_base_url: "https://chatgpt.com/backend-api/codex",
  chatgpt_model: "gpt-5.3-codex",
  chatgpt_account_id: "",
  google_gemini_generate_content_location: "us-central1",
  google_gemini_generate_content_project_id: "",
  google_gemini_generate_content_model: "gemini-2.0-flash",
  openai_responses_base_url: "https://api.openai.com/v1",
  openai_responses_api_key: "",
  openai_responses_model: "gpt-5.4",
  openai_chat_completions_base_url: "https://api.openai.com/v1",
  openai_chat_completions_api_key: "",
  openai_chat_completions_model: "gpt-5.4",
  openai_chat_completions_compatible_base_url: "https://api.openai.com/v1",
  openai_chat_completions_compatible_api_key: "",
  openai_chat_completions_compatible_model: "qwen3.5-plus",
  anthropic_messages_base_url: "https://api.anthropic.com/v1",
  anthropic_messages_api_key: "",
  anthropic_messages_model: "claude-3-5-sonnet-latest",
};

const GEMINI_FIELDS: ConfigField[] = [
  {
    key: "google_gemini_generate_content_location",
    label: "Location",
    placeholder: "us-central1",
    hint: "e.g., us-central1, asia-northeast1",
  },
  {
    key: "google_gemini_generate_content_project_id",
    label: "Project ID",
    placeholder: "your-gcp-project-id",
    hint: "Your Google Cloud project ID",
  },
  {
    key: "google_gemini_generate_content_model",
    label: "Model",
    placeholder: "gemini-2.0-flash",
    hint: "e.g., gemini-2.0-flash, gemini-2.5-pro",
  },
];

const CHATGPT_SERVICE_FIELDS: ConfigField[] = [
  {
    key: "chatgpt_model",
    label: "Model",
    placeholder: "gpt-5.3-codex",
    hint: "e.g., gpt-5.3-codex, gpt-5.2-codex, gpt-5.1-codex-max, gpt-5.1-codex-mini",
  },
  {
    key: "chatgpt_account_id",
    label: "Account ID (optional)",
    placeholder: "acct_123",
    hint: "Optional ChatGPT workspace/account binding for requests",
  },
];

const OPENAI_RESPONSES_SERVICE_FIELDS: ConfigField[] = [
  {
    key: "openai_responses_api_key",
    label: "API Key",
    placeholder: "sk-...",
    hint: "Your API key",
  },
  {
    key: "openai_responses_model",
    label: "Model",
    placeholder: "gpt-5.4",
    hint: "e.g., gpt-5.4, gpt-5",
  },
];

const OPENAI_RESPONSES_ADVANCED_FIELDS: ConfigField[] = [
  {
    key: "openai_responses_base_url",
    label: "Base URL",
    placeholder: "https://api.openai.com/v1",
    hint: "API endpoint URL",
  },
  ...OPENAI_RESPONSES_SERVICE_FIELDS,
];

const OPENAI_CHAT_COMPLETIONS_SERVICE_FIELDS: ConfigField[] = [
  {
    key: "openai_chat_completions_api_key",
    label: "API Key",
    placeholder: "sk-...",
    hint: "Your API key",
  },
  {
    key: "openai_chat_completions_model",
    label: "Model",
    placeholder: "gpt-5.4",
    hint: "e.g., gpt-5.4, gpt-5",
  },
];

const OPENAI_CHAT_COMPLETIONS_ADVANCED_FIELDS: ConfigField[] = [
  {
    key: "openai_chat_completions_base_url",
    label: "Base URL",
    placeholder: "https://api.openai.com/v1",
    hint: "API endpoint URL",
  },
  ...OPENAI_CHAT_COMPLETIONS_SERVICE_FIELDS,
];

const OPENAI_COMPATIBLE_SERVICE_FIELDS: ConfigField[] = [
  {
    key: "openai_chat_completions_compatible_api_key",
    label: "API Key",
    placeholder: "sk-...",
    hint: "Your API key",
  },
  {
    key: "openai_chat_completions_compatible_model",
    label: "Model",
    placeholder: "qwen3.5-plus",
    hint: "e.g., deepseek-chat, kimi-k2-0905-preview, openai/gpt-5.2",
  },
];

const OPENAI_COMPATIBLE_ADVANCED_FIELDS: ConfigField[] = [
  {
    key: "openai_chat_completions_compatible_base_url",
    label: "Base URL",
    placeholder: "https://api.openai.com/v1",
    hint: "API endpoint URL",
  },
  ...OPENAI_COMPATIBLE_SERVICE_FIELDS,
];

const ANTHROPIC_SERVICE_FIELDS: ConfigField[] = [
  {
    key: "anthropic_messages_api_key",
    label: "API Key",
    placeholder: "sk-ant-...",
    hint: "Your API key",
  },
  {
    key: "anthropic_messages_model",
    label: "Model",
    placeholder: "claude-3-5-sonnet-latest",
    hint: "e.g., claude-3-5-sonnet-latest, claude-3-7-sonnet-latest",
  },
];

const ANTHROPIC_ADVANCED_FIELDS: ConfigField[] = [
  {
    key: "anthropic_messages_base_url",
    label: "Base URL",
    placeholder: "https://api.anthropic.com/v1",
    hint: "API endpoint URL",
  },
  ...ANTHROPIC_SERVICE_FIELDS,
];

export const SERVICE_CATALOG: ServiceCatalogEntry[] = [
  {
    key: "openai_api_platform",
    group: "Official providers",
    kind: "preset",
    name: "OpenAI API Platform",
    desc: "Official OpenAI API key setup.",
    detail: "Uses OpenAI Responses API as the default transport.",
    provider: "openai_responses",
    sectionTitle: "OpenAI API Platform (Responses API) Configuration",
    defaults: {
      openai_responses_base_url: "https://api.openai.com/v1",
      openai_responses_model: "gpt-5.4",
    },
    fields: OPENAI_RESPONSES_SERVICE_FIELDS,
  },
  {
    key: "chatgpt_codex",
    group: "Official providers",
    kind: "preset",
    name: "ChatGPT / Codex",
    desc: "Managed ChatGPT / Codex login.",
    detail:
      "Uses the distinct chatgpt provider surface with daemon-managed browser or device login.",
    provider: "chatgpt",
    sectionTitle: "ChatGPT / Codex Managed Login Configuration",
    defaults: {
      chatgpt_base_url: "https://chatgpt.com/backend-api/codex",
      chatgpt_model: "gpt-5.3-codex",
      chatgpt_account_id: "",
    },
    fields: CHATGPT_SERVICE_FIELDS,
  },
  {
    key: "openrouter",
    group: "Popular services",
    kind: "preset",
    name: "OpenRouter",
    desc: "One API key, many hosted models.",
    detail: "Uses OpenAI Chat Completions API-compatible settings under the hood.",
    provider: "openai_chat_completions_compatible",
    sectionTitle: "OpenRouter (OpenAI Chat Completions API-compatible) Configuration",
    defaults: {
      openai_chat_completions_compatible_base_url: "https://openrouter.ai/api/v1",
      openai_chat_completions_compatible_model: "openai/gpt-5.2",
    },
    fields: OPENAI_COMPATIBLE_SERVICE_FIELDS,
  },
  {
    key: "kimi_coding",
    group: "Popular services",
    kind: "preset",
    name: "Kimi Coding",
    desc: "Moonshot AI's Kimi API for coding workflows.",
    detail: "Uses OpenAI Chat Completions API-compatible settings with Moonshot defaults.",
    provider: "openai_chat_completions_compatible",
    sectionTitle: "Kimi Coding (OpenAI Chat Completions API-compatible) Configuration",
    defaults: {
      openai_chat_completions_compatible_base_url: "https://api.moonshot.cn/v1",
      openai_chat_completions_compatible_model: "kimi-k2-0905-preview",
    },
    fields: OPENAI_COMPATIBLE_SERVICE_FIELDS,
  },
  {
    key: "deepseek",
    group: "Popular services",
    kind: "preset",
    name: "DeepSeek",
    desc: "Official DeepSeek API setup.",
    detail: "Uses OpenAI Chat Completions API-compatible settings with DeepSeek defaults.",
    provider: "openai_chat_completions_compatible",
    sectionTitle: "DeepSeek (OpenAI Chat Completions API-compatible) Configuration",
    defaults: {
      openai_chat_completions_compatible_base_url: "https://api.deepseek.com/v1",
      openai_chat_completions_compatible_model: "deepseek-chat",
    },
    fields: OPENAI_COMPATIBLE_SERVICE_FIELDS,
  },
  {
    key: "google_gemini_vertex",
    group: "Official providers",
    kind: "preset",
    name: "Google Gemini via Vertex AI",
    desc: "Official Google Cloud / Vertex AI setup.",
    detail: "Uses Google Gemini GenerateContent API settings.",
    provider: "google_gemini_generate_content",
    sectionTitle: "Google Gemini via Vertex AI Configuration",
    defaults: {
      google_gemini_generate_content_location: "us-central1",
      google_gemini_generate_content_model: "gemini-2.0-flash",
    },
    fields: GEMINI_FIELDS,
  },
  {
    key: "anthropic_api",
    group: "Official providers",
    kind: "preset",
    name: "Anthropic API",
    desc: "Official Anthropic API key setup.",
    detail: "Uses Anthropic Messages API settings.",
    provider: "anthropic_messages",
    sectionTitle: "Anthropic API (Messages API) Configuration",
    defaults: {
      anthropic_messages_base_url: "https://api.anthropic.com/v1",
      anthropic_messages_model: "claude-3-5-sonnet-latest",
    },
    fields: ANTHROPIC_SERVICE_FIELDS,
  },
  {
    key: "advanced_custom",
    group: "Advanced",
    kind: "advanced_handoff",
    name: "Advanced / custom setup",
    desc: "Choose the raw API family yourself.",
    detail: "Use this expert path for custom endpoints or manual transport selection.",
  },
];

export const ADVANCED_PROVIDER_CATALOG: ConfigurableSetupOption[] = [
  {
    key: "advanced_openai_responses",
    group: "Advanced",
    kind: "preset",
    name: "OpenAI Responses API",
    desc: "Manual OpenAI API Platform setup.",
    detail: "Expert path when you want direct control over the Responses API config.",
    provider: "openai_responses",
    sectionTitle: "OpenAI Responses API Configuration",
    defaults: {
      openai_responses_base_url: "https://api.openai.com/v1",
      openai_responses_model: "gpt-5.4",
    },
    fields: OPENAI_RESPONSES_ADVANCED_FIELDS,
  },
  {
    key: "advanced_openai_chat_completions",
    group: "Advanced",
    kind: "preset",
    name: "OpenAI Chat Completions API",
    desc: "Manual official chat/completions setup.",
    detail: "Use this only if you explicitly want chat/completions instead of Responses.",
    provider: "openai_chat_completions",
    sectionTitle: "OpenAI Chat Completions API Configuration",
    defaults: {
      openai_chat_completions_base_url: "https://api.openai.com/v1",
      openai_chat_completions_model: "gpt-5.4",
    },
    fields: OPENAI_CHAT_COMPLETIONS_ADVANCED_FIELDS,
  },
  {
    key: "advanced_openai_chat_completions_compatible",
    group: "Advanced",
    kind: "preset",
    name: "OpenAI Chat Completions API-compatible",
    desc: "Manual setup for custom compatible endpoints.",
    detail: "Use this for providers that mirror OpenAI chat/completions semantics.",
    provider: "openai_chat_completions_compatible",
    sectionTitle: "Custom OpenAI Chat Completions API-compatible Configuration",
    defaults: {
      openai_chat_completions_compatible_base_url: "https://api.openai.com/v1",
      openai_chat_completions_compatible_model: "qwen3.5-plus",
    },
    fields: OPENAI_COMPATIBLE_ADVANCED_FIELDS,
  },
  {
    key: "advanced_anthropic_messages",
    group: "Advanced",
    kind: "preset",
    name: "Anthropic Messages API",
    desc: "Manual setup for Anthropic-compatible endpoints.",
    detail: "Use this for Anthropic-native or compatible Messages API surfaces.",
    provider: "anthropic_messages",
    sectionTitle: "Anthropic Messages API Configuration",
    defaults: {
      anthropic_messages_base_url: "https://api.anthropic.com/v1",
      anthropic_messages_model: "claude-3-5-sonnet-latest",
    },
    fields: ANTHROPIC_ADVANCED_FIELDS,
  },
  {
    key: "advanced_google_gemini_generate_content",
    group: "Advanced",
    kind: "preset",
    name: "Google Gemini GenerateContent API",
    desc: "Manual Google Vertex AI setup.",
    detail: "Use this when you want raw control over Gemini project, region, and model.",
    provider: "google_gemini_generate_content",
    sectionTitle: "Google Gemini GenerateContent API Configuration",
    defaults: {
      google_gemini_generate_content_location: "us-central1",
      google_gemini_generate_content_model: "gemini-2.0-flash",
    },
    fields: GEMINI_FIELDS,
  },
];

export function isConfigurableSetupOption(
  option: ServiceCatalogEntry | ConfigurableSetupOption,
): option is ConfigurableSetupOption {
  return option.kind === "preset";
}

export function applySetupDefaults(
  current: ConfigValues,
  option: ConfigurableSetupOption,
): ConfigValues {
  return { ...current, ...option.defaults };
}

export function configForSetupSelection(
  current: ConfigValues,
  previous: ConfigurableSetupOption | null,
  next: ConfigurableSetupOption,
): ConfigValues {
  const resetConfig = applySetupDefaults(DEFAULT_CONFIG, next);
  if (!previous || previous.key !== next.key) {
    return resetConfig;
  }

  const preservedValues = Object.fromEntries(
    next.fields.map((field) => [field.key, current[field.key]]),
  ) as Partial<ConfigValues>;

  return { ...resetConfig, ...preservedValues };
}

export function configFieldsForSetup(option: ConfigurableSetupOption): ConfigField[] {
  return option.fields;
}

function resolvedValue(
  option: ConfigurableSetupOption,
  config: ConfigValues,
  key: keyof ConfigValues,
): string {
  const currentValue = config[key];
  if (typeof currentValue === "string" && currentValue.trim() !== "") {
    return currentValue;
  }

  const presetValue = option.defaults[key];
  if (typeof presetValue === "string" && presetValue.trim() !== "") {
    return presetValue;
  }

  return DEFAULT_CONFIG[key];
}

export interface SetupSecretMaterial {
  credentialId: string;
  secret: string;
}

export function defaultProfileIdForSetup(option: ConfigurableSetupOption): string {
  switch (option.key) {
    case "chatgpt_codex":
      return "chatgpt-main";
    case "openai_api_platform":
      return "openai-main";
    case "openrouter":
      return "openrouter";
    case "kimi_coding":
      return "kimi";
    case "deepseek":
      return "deepseek";
    case "google_gemini_vertex":
      return "gemini";
    case "anthropic_api":
      return "anthropic-main";
    default:
      return option.provider.replaceAll("_", "-");
  }
}

export function browserLoginProfileIdForSetup(option: ConfigurableSetupOption): string | null {
  if (option.provider !== "chatgpt") {
    return null;
  }
  return defaultProfileIdForSetup(option);
}

function defaultCredentialIdForSetup(option: ConfigurableSetupOption): string | null {
  if (option.provider === "google_gemini_generate_content") {
    return null;
  }
  return defaultProfileIdForSetup(option);
}

function credentialKindForProvider(option: ConfigurableSetupOption): string | null {
  switch (option.provider) {
    case "chatgpt":
      return "managed_oauth";
    case "google_gemini_generate_content":
      return null;
    default:
      return "secret_string";
  }
}

function credentialBackendForProvider(option: ConfigurableSetupOption): string | null {
  switch (option.provider) {
    case "chatgpt":
      return "alan_home_auth_json";
    case "google_gemini_generate_content":
      return null;
    default:
      return "alan_home_secret_store";
  }
}

function tomlString(value: string): string {
  return JSON.stringify(value);
}

function buildProfileSettings(
  option: ConfigurableSetupOption,
  config: ConfigValues,
): Record<string, string> {
  switch (option.provider) {
    case "chatgpt":
      return {
        base_url: resolvedValue(option, config, "chatgpt_base_url"),
        model: resolvedValue(option, config, "chatgpt_model"),
        account_id: resolvedValue(option, config, "chatgpt_account_id"),
      };
    case "google_gemini_generate_content":
      return {
        project_id: resolvedValue(option, config, "google_gemini_generate_content_project_id"),
        location: resolvedValue(option, config, "google_gemini_generate_content_location"),
        model: resolvedValue(option, config, "google_gemini_generate_content_model"),
      };
    case "openai_responses":
      return {
        base_url: resolvedValue(option, config, "openai_responses_base_url"),
        model: resolvedValue(option, config, "openai_responses_model"),
      };
    case "openai_chat_completions":
      return {
        base_url: resolvedValue(option, config, "openai_chat_completions_base_url"),
        model: resolvedValue(option, config, "openai_chat_completions_model"),
      };
    case "openai_chat_completions_compatible":
      return {
        base_url: resolvedValue(option, config, "openai_chat_completions_compatible_base_url"),
        model: resolvedValue(option, config, "openai_chat_completions_compatible_model"),
      };
    case "anthropic_messages":
      return {
        base_url: resolvedValue(option, config, "anthropic_messages_base_url"),
        model: resolvedValue(option, config, "anthropic_messages_model"),
      };
  }
}

export function buildSetupSecretMaterial(
  option: ConfigurableSetupOption,
  config: ConfigValues,
): SetupSecretMaterial | null {
  const credentialId = defaultCredentialIdForSetup(option);
  if (!credentialId) {
    return null;
  }

  let secret = "";
  switch (option.provider) {
    case "openai_responses":
      secret = resolvedValue(option, config, "openai_responses_api_key");
      break;
    case "openai_chat_completions":
      secret = resolvedValue(option, config, "openai_chat_completions_api_key");
      break;
    case "openai_chat_completions_compatible":
      secret = resolvedValue(option, config, "openai_chat_completions_compatible_api_key");
      break;
    case "anthropic_messages":
      secret = resolvedValue(option, config, "anthropic_messages_api_key");
      break;
    default:
      secret = "";
      break;
  }

  if (!secret.trim()) {
    return null;
  }

  return { credentialId, secret };
}

export function buildConfigContent(
  option: ConfigurableSetupOption,
  _config: ConfigValues,
  channel: InstallChannelId = "stable",
): string {
  const descriptor = installChannelDescriptor(channel);
  return `# alan Agent Configuration
# Generated by alan init wizard

# Selected service
# ${option.name}

# Default connection profile is managed in ~/${descriptor.alanHomeDirName}/connections.toml
# Use \`alan connection pin <profile-id>\` only when this agent/workspace
# should stay fixed to one profile.

# Public skills install directory
# ~/${descriptor.globalSkillsParentDirName}/skills/

# Runtime Configuration
llm_request_timeout_secs = 180
tool_timeout_secs = 30

# Memory Configuration
[memory]
enabled = true
strict_workspace = true
`;
}

export function buildConnectionsContent(
  option: ConfigurableSetupOption,
  config: ConfigValues,
): string {
  const profileId = defaultProfileIdForSetup(option);
  const credentialId = defaultCredentialIdForSetup(option);
  const credentialKind = credentialKindForProvider(option);
  const credentialBackend = credentialBackendForProvider(option);
  const settings = buildProfileSettings(option, config);
  const now = new Date().toISOString();
  const settingsLines = Object.entries(settings)
    .map(([key, value]) => `${key} = ${tomlString(value)}`)
    .join("\n");

  let content = `version = 1
default_profile = ${tomlString(profileId)}
`;

  if (credentialId && credentialKind && credentialBackend) {
    content += `

[credentials.${credentialId}]
kind = ${tomlString(credentialKind)}
provider_family = ${tomlString(option.provider)}
label = ${tomlString(`${option.name} credential`)}
backend = ${tomlString(credentialBackend)}
`;
  }

  content += `

[profiles.${profileId}]
provider = ${tomlString(option.provider)}
created_at = ${tomlString(now)}
updated_at = ${tomlString(now)}
`;

  if (credentialId) {
    content += `credential_id = ${tomlString(credentialId)}\n`;
  }

  content += `source = "managed"

[profiles.${profileId}.settings]
${settingsLines}
`;

  return content;
}

export function buildHostConfigContent(channel: InstallChannelId = "stable"): string {
  const descriptor = installChannelDescriptor(channel);
  return `# alan Host Configuration
# Generated by alan init wizard

bind_address = "${descriptor.daemonBind}"
daemon_url = "${descriptor.daemonUrl}"
`;
}
