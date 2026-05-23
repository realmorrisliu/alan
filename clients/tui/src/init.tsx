/**
 * First-time setup wizard for alan
 *
 * Runs when no agent config exists at the canonical or override path.
 */

import React, { useState } from "react";
import { Box, Text, useInput } from "ink";
import TextInput from "ink-text-input";
import { homedir } from "node:os";
import { join } from "node:path";
import { writeCanonicalSetupFiles } from "./init-files.js";
import { installChannelDescriptor, type InstallChannelId } from "./install-channel.js";
import {
  ADVANCED_PROVIDER_CATALOG,
  browserLoginProfileIdForSetup,
  buildConnectionsContent,
  buildConfigContent,
  buildHostConfigContent,
  buildSetupSecretMaterial,
  configForSetupSelection,
  configFieldsForSetup,
  DEFAULT_CONFIG,
  defaultProfileIdForSetup,
  isConfigurableSetupOption,
  SERVICE_CATALOG,
  type ConfigValues,
  type ConfigurableSetupOption,
  type Provider,
} from "./setup-catalog.js";

export interface InitWizardCompletion {
  selectedProfileId: string;
  selectedProvider: Provider;
  browserLoginProfileId: string | null;
}

interface InitWizardProps {
  onComplete: (completion: InitWizardCompletion) => void;
  agentConfigPath: string;
  hostConfigPath: string;
  installChannel?: InstallChannelId;
}

type WizardStep = "welcome" | "service" | "advanced_provider" | "config" | "done";
type ConfigReturnStep = "service" | "advanced_provider";

function displayPath(path: string): string {
  const home = homedir();
  if (path === home) return "~";
  const homePrefix = `${home}/`;
  return path.startsWith(homePrefix) ? `~/${path.slice(homePrefix.length)}` : path;
}

const DEFAULT_TARGET =
  SERVICE_CATALOG.find(isConfigurableSetupOption) ?? ADVANCED_PROVIDER_CATALOG[0];

function completionForSetup(option: ConfigurableSetupOption): InitWizardCompletion {
  return {
    selectedProfileId: defaultProfileIdForSetup(option),
    selectedProvider: option.provider,
    browserLoginProfileId: browserLoginProfileIdForSetup(option),
  };
}

export function InitWizard({
  onComplete,
  agentConfigPath,
  hostConfigPath,
  installChannel = "stable",
}: InitWizardProps) {
  const descriptor = installChannelDescriptor(installChannel);
  const alanHomeDir = join(homedir(), descriptor.alanHomeDirName);
  const globalPublicSkillsDir = join(homedir(), descriptor.globalSkillsParentDirName, "skills");
  const connectionsConfigPath = join(alanHomeDir, "connections.toml");
  const [step, setStep] = useState<WizardStep>("welcome");
  const [selectedTarget, setSelectedTarget] = useState<ConfigurableSetupOption>(DEFAULT_TARGET);
  const [configReturnStep, setConfigReturnStep] = useState<ConfigReturnStep>("service");
  const [config, setConfig] = useState<ConfigValues>(DEFAULT_CONFIG);

  const [serviceCursor, setServiceCursor] = useState(0);
  const [advancedProviderCursor, setAdvancedProviderCursor] = useState(0);
  const [configFieldIndex, setConfigFieldIndex] = useState(0);
  const [inputValue, setInputValue] = useState("");
  const [hostConfigStatus, setHostConfigStatus] = useState<"created" | "preserved" | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);

  const beginConfig = (option: ConfigurableSetupOption, returnStep: ConfigReturnStep) => {
    const nextConfig = configForSetupSelection(config, selectedTarget, option);
    const fields = configFieldsForSetup(option);
    setSelectedTarget(option);
    setConfigReturnStep(returnStep);
    setConfig(nextConfig);
    setConfigFieldIndex(0);
    setInputValue(String(nextConfig[fields[0].key] || ""));
    setHostConfigStatus(null);
    setSaveError(null);
    setStep("config");
  };

  const saveConfig = (option: ConfigurableSetupOption, values: ConfigValues): boolean => {
    const agentConfigContent = buildConfigContent(option, values, installChannel);
    const connectionsConfigContent = buildConnectionsContent(option, values);
    const setupSecret = buildSetupSecretMaterial(option, values);
    const hostConfigContent = buildHostConfigContent(installChannel);
    try {
      const result = writeCanonicalSetupFiles({
        agentConfigPath,
        agentConfigContent,
        connectionsConfigPath,
        connectionsConfigContent,
        credentialSecretPath: setupSecret
          ? join(alanHomeDir, "credentials", `${setupSecret.credentialId}.secret`)
          : undefined,
        credentialSecretContent: setupSecret?.secret,
        globalPublicSkillsDir,
        hostConfigPath,
        hostConfigContent,
      });
      setHostConfigStatus(result.hostConfigStatus);
      setSaveError(null);
      return true;
    } catch (error) {
      setHostConfigStatus(null);
      setSaveError(
        error instanceof Error ? error.message : "Failed to write alan configuration files.",
      );
      return false;
    }
  };

  useInput((_input, key) => {
    if (step === "welcome") {
      if (key.return) {
        setStep("service");
      }
      return;
    }

    if (step === "service") {
      if (key.upArrow) {
        setServiceCursor((cursor) => (cursor > 0 ? cursor - 1 : SERVICE_CATALOG.length - 1));
      } else if (key.downArrow) {
        setServiceCursor((cursor) => (cursor < SERVICE_CATALOG.length - 1 ? cursor + 1 : 0));
      } else if (key.return) {
        const option = SERVICE_CATALOG[serviceCursor];
        if (isConfigurableSetupOption(option)) {
          beginConfig(option, "service");
        } else {
          setStep("advanced_provider");
        }
      }
      return;
    }

    if (step === "advanced_provider") {
      if (key.upArrow) {
        setAdvancedProviderCursor((cursor) =>
          cursor > 0 ? cursor - 1 : ADVANCED_PROVIDER_CATALOG.length - 1,
        );
      } else if (key.downArrow) {
        setAdvancedProviderCursor((cursor) =>
          cursor < ADVANCED_PROVIDER_CATALOG.length - 1 ? cursor + 1 : 0,
        );
      } else if (key.escape) {
        setStep("service");
      } else if (key.return) {
        beginConfig(ADVANCED_PROVIDER_CATALOG[advancedProviderCursor], "advanced_provider");
      }
      return;
    }

    if (step !== "config") {
      return;
    }

    const fields = configFieldsForSetup(selectedTarget);
    const currentField = fields[configFieldIndex];

    if (key.escape) {
      setInputValue("");
      setSaveError(null);
      setStep(configReturnStep);
      return;
    }

    if (!key.return) {
      return;
    }

    const nextConfig = { ...config, [currentField.key]: inputValue };
    setConfig(nextConfig);

    if (configFieldIndex < fields.length - 1) {
      const nextIndex = configFieldIndex + 1;
      setConfigFieldIndex(nextIndex);
      setInputValue(String(nextConfig[fields[nextIndex].key] || ""));
      return;
    }

    if (!saveConfig(selectedTarget, nextConfig)) {
      return;
    }
    const completion = completionForSetup(selectedTarget);
    setStep("done");
    setTimeout(() => onComplete(completion), 2000);
  });

  if (step === "welcome") {
    return (
      <Box flexDirection="column" padding={1}>
        <Text bold color="cyan">
          Welcome to alan!
        </Text>
        <Text> </Text>
        <Text>alan is an AI assistant that runs in your terminal.</Text>
        <Text> </Text>
        <Text>First, choose the service you want alan to connect to.</Text>
        <Text> </Text>
        <Text color="gray">
          alan will write the canonical agent config and, when host.toml is missing, create it so
          the daemon keeps the wizard's loopback default. It also prepares ~/.agents/skills/ as the
          default zero-conversion public skill install directory for this install channel. Existing
          host config is preserved. Advanced / custom setup is still available.
        </Text>
        <Text> </Text>
        <Text color="gray">Press Enter to continue...</Text>
      </Box>
    );
  }

  if (step === "service") {
    let previousGroup: string | null = null;

    return (
      <Box flexDirection="column" padding={1}>
        <Text bold>Select a service to connect:</Text>
        <Text color="gray">
          You pick the service. alan handles the underlying provider mapping.
        </Text>
        <Text> </Text>
        {SERVICE_CATALOG.map((option, index) => {
          const heading =
            option.group === previousGroup ? null : (
              <Text key={`${option.key}-group`} bold color="cyan">
                {option.group}
              </Text>
            );
          previousGroup = option.group;

          return (
            <Box key={option.key} flexDirection="column" marginBottom={1}>
              {heading}
              <Text color={serviceCursor === index ? "green" : "white"}>
                {serviceCursor === index ? "> " : "  "}
                {option.name}
              </Text>
              <Text color="gray"> {option.desc}</Text>
              <Text color="gray"> {option.detail}</Text>
            </Box>
          );
        })}
        <Text color="gray">↑↓ to select, Enter to continue</Text>
      </Box>
    );
  }

  if (step === "advanced_provider") {
    return (
      <Box flexDirection="column" padding={1}>
        <Text bold>Advanced / custom setup</Text>
        <Text color="gray">
          Choose the raw API family only if you need manual endpoint-level control.
        </Text>
        <Text> </Text>
        {ADVANCED_PROVIDER_CATALOG.map((option, index) => (
          <Box key={option.key} flexDirection="column" marginBottom={1}>
            <Text color={advancedProviderCursor === index ? "green" : "white"}>
              {advancedProviderCursor === index ? "> " : "  "}
              {option.name}
            </Text>
            <Text color="gray"> {option.desc}</Text>
            <Text color="gray"> {option.detail}</Text>
          </Box>
        ))}
        <Text color="gray">↑↓ to select, Enter to continue, Esc to go back</Text>
      </Box>
    );
  }

  if (step === "config") {
    const fields = configFieldsForSetup(selectedTarget);
    const currentField = fields[configFieldIndex];
    const exposesBaseUrl = fields.some((field) => field.key.includes("base_url"));

    return (
      <Box flexDirection="column" padding={1}>
        <Text bold>Configure {selectedTarget.name}</Text>
        <Text color="gray">{selectedTarget.desc}</Text>
        <Text color="gray">{selectedTarget.detail}</Text>
        <Text color="gray">
          alan writes canonical agent config plus connections.toml, and preserves any existing host
          config for {selectedTarget.provider}. If host.toml is missing, setup must create it so the
          daemon keeps the wizard's loopback defaults.
        </Text>
        {selectedTarget.provider === "chatgpt" && (
          <Text color="gray">
            This preset uses managed login instead of an API key. After setup, alan will open
            browser login for {defaultProfileIdForSetup(selectedTarget)} automatically.
          </Text>
        )}
        {!exposesBaseUrl && selectedTarget.provider !== "google_gemini_generate_content" && (
          <Text color="gray">
            Endpoint defaults are prefilled for this service. Use Advanced / custom setup if you
            need to edit raw base URLs or switch API families.
          </Text>
        )}
        <Text color="gray">
          Step {configFieldIndex + 1} of {fields.length}
        </Text>
        <Text> </Text>

        {fields.slice(0, configFieldIndex).map((field) => (
          <Box key={field.key} flexDirection="row">
            <Text color="green">✓ </Text>
            <Text>{field.label}: </Text>
            <Text color="cyan">
              {field.key.includes("api_key") && config[field.key]
                ? "*".repeat(String(config[field.key]).length)
                : String(config[field.key] || "")}
            </Text>
          </Box>
        ))}

        <Box flexDirection="column">
          <Box flexDirection="row">
            <Text color="yellow">→ </Text>
            <Text bold>{currentField.label}: </Text>
            <TextInput
              value={inputValue}
              onChange={(value) => {
                setInputValue(value);
                if (saveError) {
                  setSaveError(null);
                }
              }}
              placeholder={currentField.placeholder}
              mask={currentField.key.includes("api_key") ? "*" : undefined}
            />
          </Box>
          {currentField.hint && <Text color="gray"> {currentField.hint}</Text>}
          {saveError && <Text color="red"> {saveError}</Text>}
        </Box>

        {fields.slice(configFieldIndex + 1).map((field) => (
          <Box key={field.key} flexDirection="row">
            <Text color="gray">○ {field.label}</Text>
          </Box>
        ))}

        <Text> </Text>
        <Text color="gray">Enter to continue, Esc to go back</Text>
      </Box>
    );
  }

  return (
    <Box flexDirection="column" padding={1}>
      <Text bold color="green">
        ✓ Configuration saved!
      </Text>
      <Text> </Text>
      <Text>Selected service: {selectedTarget.name}</Text>
      <Text>Agent config: {displayPath(agentConfigPath)}</Text>
      <Text>Connections: {displayPath(connectionsConfigPath)}</Text>
      <Text>
        {hostConfigStatus === "preserved"
          ? `Host config: preserved existing file at ${displayPath(hostConfigPath)}`
          : `Host config: ${displayPath(hostConfigPath)}`}
      </Text>
      {selectedTarget.provider === "chatgpt" && (
        <Text>
          Next step: alan will open browser login for {defaultProfileIdForSetup(selectedTarget)}.
        </Text>
      )}
      <Text> </Text>
      <Text>Starting alan...</Text>
    </Box>
  );
}
