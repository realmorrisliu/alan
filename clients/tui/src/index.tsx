#!/usr/bin/env bun
/**
 * alan TUI entrypoint.
 */

import React, { useEffect, useRef, useState } from "react";
import { render, Box, Text, useApp, useInput } from "ink";
import TextInput from "ink-text-input";
import { homedir } from "node:os";
import { AlanClient } from "./client.js";
import { parseAgentCommand } from "./agent-commands.js";
import { openUrlInBrowser } from "./open-url.js";
import {
  defaultHostConfigPath,
  isExistingConfigFile,
  resolveAgentdUrlOverride,
  resolveConfigPathCandidates,
  selectExistingConfigPath,
  shouldRunFirstTimeSetup,
} from "./config-path.js";
import { detectWorkspaceDirFromCwd } from "./workspace-detect.js";
import type {
  ClientCapabilities,
  ChildRunRecord,
  ConnectionCurrentState,
  ConnectionCredentialStatus,
  ConnectionPinScope,
  ConnectionProfileSummary,
  CreateSessionRequest,
  DaemonStatus,
  EventEnvelope,
  ProviderId,
} from "./types.js";
import { MessageList } from "./components.js";
import { InitWizard, type InitWizardCompletion } from "./init.js";
import { getAdaptiveSurface } from "./adaptive-surfaces/registry.js";
import { buildSchemaDrivenYieldPayload } from "./schema-driven-yield.js";
import { parsePendingYieldKind, type PendingYield } from "./adaptive-surfaces/yield-state.js";
import {
  buildAdaptiveSurfaceViewModel,
  isAdaptiveSurfaceReadyForInput,
} from "./adaptive-surfaces/view-model.js";
import {
  buildStructuredResumePayload,
  createStructuredFormState,
  getStructuredAnswer,
  moveStructuredQuestion,
  questionValidationError,
  selectStructuredSingleOption,
  setStructuredTextAnswer,
  shouldReuseStructuredFormState,
  structuredFormValidationError,
  type StructuredFormState,
} from "./structured-input.js";
import {
  confirmationActionOptions,
  confirmationDefaultOption,
  preferredConfirmationActionIndex,
  structuredQuestions,
  usesMultiSelectKind,
  usesSingleSelectKind,
  usesTextEntryKind,
} from "./yield.js";
import { clearShellBinding, readShellBindingTarget, writeShellBinding } from "./shell-binding.js";
import {
  mergeHydratedCurrentPlanState,
  reduceCurrentPlanState,
  type CurrentPlanState,
} from "./summary-surfaces/plan-state.js";
import {
  buildCurrentRuntimeSummary,
  createCurrentRuntimeState,
  reduceCurrentRuntimeState,
  type CurrentRuntimeState,
  type ShellRunStatus,
} from "./summary-surfaces/runtime-state.js";
import { SummaryHud } from "./summary-surfaces/hud-surface.js";
import { installChannelDescriptor, resolveInstallChannel } from "./install-channel.js";

const INSTALL_CHANNEL = resolveInstallChannel(process.env, process.argv[1]);
const INSTALL_DESCRIPTOR = installChannelDescriptor(INSTALL_CHANNEL);
const AGENTD_URL = resolveAgentdUrlOverride(process.env);
const AUTO_MANAGE = !AGENTD_URL;
const VERBOSE = process.env.ALAN_VERBOSE === "1";
const MAX_EVENT_HISTORY = 2000;
const DEFAULT_AGENT_NAME = normalizeAgentName(process.env.ALAN_AGENT_NAME);

function displayPath(path: string): string {
  const home = homedir();
  if (path === home) {
    return "~";
  }
  const homePrefix = `${home}/`;
  return path.startsWith(homePrefix) ? `~/${path.slice(homePrefix.length)}` : path;
}

const CONFIG_PATH_CANDIDATES = resolveConfigPathCandidates(homedir(), process.env, INSTALL_CHANNEL);
const CONFIG_PATH =
  selectExistingConfigPath(CONFIG_PATH_CANDIDATES, isExistingConfigFile) ??
  CONFIG_PATH_CANDIDATES[0];
const HOST_CONFIG_PATH = defaultHostConfigPath(homedir(), INSTALL_CHANNEL);
const CONFIG_PATH_HINT = CONFIG_PATH_CANDIDATES.map(displayPath).join(" -> ");

const STARTUP_INFO = {
  mode: AGENTD_URL ? "remote" : ("embedded" as const),
  url: AGENTD_URL || INSTALL_DESCRIPTOR.daemonWsUrl,
};

const TUI_CLIENT_CAPABILITIES: ClientCapabilities = {
  adaptive_yields: {
    rich_confirmation: true,
    structured_input: true,
    schema_driven_forms: true,
    presentation_hints: true,
  },
};

function needsFirstTimeSetup(): boolean {
  if (AGENTD_URL) return false;
  return shouldRunFirstTimeSetup(CONFIG_PATH_CANDIDATES, isExistingConfigFile);
}

function shortId(value: string | null | undefined): string {
  if (!value) return "-";
  return value.slice(0, 8);
}

function formatChildRun(run: ChildRunRecord): string {
  const heartbeat = run.latest_heartbeat_at_ms
    ? new Date(run.latest_heartbeat_at_ms).toISOString()
    : "none";
  const progress = run.latest_progress_at_ms
    ? new Date(run.latest_progress_at_ms).toISOString()
    : "none";
  return `${run.id} | ${run.status} | child=${shortId(run.child_session_id)} | heartbeat=${heartbeat} | progress=${progress}${run.latest_status_summary ? ` | ${run.latest_status_summary}` : ""}`;
}

function isActiveChildRun(run: ChildRunRecord): boolean {
  return ["starting", "running", "blocked", "terminating"].includes(run.status);
}

function countActiveChildRuns(runs: ChildRunRecord[]): number {
  return runs.filter(isActiveChildRun).length;
}

function parseGovernanceProfile(input: string | undefined): "autonomous" | "conservative" | null {
  if (!input) return null;
  const value = input.trim().toLowerCase();
  if (value === "autonomous" || value === "conservative") {
    return value;
  }
  return null;
}

function normalizeAgentName(input: string | undefined): string | null {
  if (!input) return null;
  const value = input.trim();
  return value.length > 0 ? value : null;
}

function parseAgentSelector(input: string | undefined): string | null {
  if (!input) return null;
  const [key, ...rest] = input.split("=");
  if (key !== "agent" && key !== "agent_name") {
    return null;
  }
  return normalizeAgentName(rest.join("="));
}

function parseStreamingMode(input: string | undefined): "auto" | "on" | "off" | null {
  if (!input) return null;
  const value = input.trim().toLowerCase();
  if (value === "auto" || value === "on" || value === "off") {
    return value;
  }
  return null;
}

function parsePartialStreamRecoveryMode(input: string | undefined): "continue_once" | "off" | null {
  if (!input) return null;
  const value = input.trim().toLowerCase();
  if (value === "continue_once") {
    return "continue_once";
  }
  if (value.startsWith("recovery=")) {
    const mode = value.slice("recovery=".length);
    if (mode === "continue_once" || mode === "off") {
      return mode;
    }
  }
  if (value.startsWith("partial_stream_recovery_mode=")) {
    const mode = value.slice("partial_stream_recovery_mode=".length);
    if (mode === "continue_once" || mode === "off") {
      return mode;
    }
  }
  return null;
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

interface SessionBindingSummary {
  profileId?: string;
  provider?: ProviderId;
  resolvedModel?: string;
}

function isChatgptNotLoggedInMessage(message: string): boolean {
  return message.toLowerCase().includes("not logged in to chatgpt");
}

function formatTimestamp(value: string | undefined): string | null {
  if (!value) {
    return null;
  }
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }
  return parsed.toLocaleString();
}

function providerDisplayName(provider: ProviderId): string {
  switch (provider) {
    case "chatgpt":
      return "ChatGPT / Codex";
    case "google_gemini_generate_content":
      return "Google Gemini";
    case "openai_responses":
      return "OpenAI Responses";
    case "openai_chat_completions":
      return "OpenAI Chat Completions";
    case "openai_chat_completions_compatible":
      return "OpenAI-compatible";
    case "anthropic_messages":
      return "Anthropic Messages";
    default:
      return provider;
  }
}

function summarizeSessionBinding(binding: SessionBindingSummary | null): string {
  if (!binding?.profileId) {
    return "profile=unresolved";
  }
  const parts = [`profile=${binding.profileId}`];
  if (binding.provider) {
    parts.push(`provider=${binding.provider}`);
  }
  if (binding.resolvedModel) {
    parts.push(`model=${binding.resolvedModel}`);
  }
  return parts.join(" | ");
}

function summarizeConnectionProfile(
  profile: ConnectionProfileSummary,
  status?: ConnectionCredentialStatus,
): string {
  const details = [
    `${profile.profile_id}`,
    `provider=${profile.provider}`,
    `credential=${profile.credential_status}`,
  ];
  if (profile.is_default) {
    details.push("default");
  }
  const model = profile.settings.model;
  if (typeof model === "string" && model.trim()) {
    details.push(`model=${model}`);
  }
  if (status?.detail?.account_email) {
    details.push(`email=${status.detail.account_email}`);
  }
  if (status?.detail?.account_plan) {
    details.push(`plan=${status.detail.account_plan}`);
  }
  return details.join(" | ");
}

function summarizeCredentialStatus(status: ConnectionCredentialStatus): string {
  const details = [
    `profile=${status.profile_id}`,
    `credential=${status.status}`,
    `kind=${status.credential_kind}`,
  ];
  if (status.detail?.account_email) {
    details.push(`email=${status.detail.account_email}`);
  }
  if (status.detail?.account_plan) {
    details.push(`plan=${status.detail.account_plan}`);
  }
  const checkedAt = formatTimestamp(status.last_checked_at);
  if (checkedAt) {
    details.push(`checked=${checkedAt}`);
  }
  if (status.detail?.message) {
    details.push(`detail=${status.detail.message}`);
  }
  return details.join(" | ");
}

function detectWorkspaceDirForSelection(): string | undefined {
  return detectWorkspaceDirFromCwd(process.cwd());
}

function summarizePinState(label: string, pin?: ConnectionCurrentState["global_pin"]): string {
  if (!pin) {
    return `${label}: <unset>`;
  }
  return `${label}: ${pin.profile_id} (${pin.scope}) [${displayPath(pin.config_path)}]`;
}

function summarizeConnectionCurrentState(current: ConnectionCurrentState): string[] {
  const lines: string[] = [];
  if (current.workspace_dir) {
    lines.push(`workspace: ${displayPath(current.workspace_dir)}`);
  }
  lines.push(summarizePinState("Global pin", current.global_pin));
  lines.push(summarizePinState("Workspace pin", current.workspace_pin));
  lines.push(
    current.default_profile
      ? `Default profile: ${current.default_profile}`
      : "Default profile: <unset>",
  );
  lines.push(
    current.effective_profile
      ? `Effective profile for new sessions: ${current.effective_profile} (${current.effective_source})`
      : `Effective profile for new sessions: <unset> (${current.effective_source})`,
  );
  return lines;
}

function App() {
  const app = useApp();

  const [needsSetup, setNeedsSetup] = useState(needsFirstTimeSetup());
  const [inputValue, setInputValue] = useState("");
  const [status, setStatus] = useState<"connecting" | "connected" | "error">("connecting");
  const [statusMessage, setStatusMessage] = useState("Starting...");
  const [currentSessionId, setCurrentSessionId] = useState<string | null>(null);
  const [currentSessionBinding, setCurrentSessionBinding] = useState<SessionBindingSummary | null>(
    null,
  );
  const [events, setEvents] = useState<EventEnvelope[]>([]);
  const [currentPlan, setCurrentPlan] = useState<CurrentPlanState | null>(null);
  const [activeChildRunCount, setActiveChildRunCount] = useState<number | null>(null);
  const [currentRuntimeState, setCurrentRuntimeState] =
    useState<CurrentRuntimeState>(createCurrentRuntimeState);
  const [daemonStatus, setDaemonStatus] = useState<DaemonStatus | null>(null);
  const [pendingYield, setPendingYield] = useState<PendingYield | null>(null);
  const [shellRunStatus, setShellRunStatus] = useState<ShellRunStatus>("starting");
  const [confirmationActionIndex, setConfirmationActionIndex] = useState(0);
  const [confirmationActionRequestId, setConfirmationActionRequestId] = useState<string | null>(
    null,
  );
  const [schemaFormState, setSchemaFormState] = useState<StructuredFormState | null>(null);
  const [structuredFormState, setStructuredFormState] = useState<StructuredFormState | null>(null);

  const clientRef = useRef<AlanClient | null>(null);
  const sessionIdRef = useRef<string>("");
  const currentSessionBindingRef = useRef<SessionBindingSummary | null>(null);
  const activeConnectionLoginsRef = useRef<Set<string>>(new Set());
  const pendingSetupBrowserLoginRef = useRef<string | null>(null);
  const shellBindingTarget = useRef(readShellBindingTarget(process.env)).current;
  const adaptiveSurfaceViewModel = buildAdaptiveSurfaceViewModel({
    pendingYield,
    confirmationActionIndex,
    confirmationActionRequestId,
    structuredFormState,
    schemaFormState,
  });
  const {
    pendingStructuredQuestions,
    pendingSchemaForm,
    activeStructuredQuestion,
    activeSchemaQuestion,
    activeSurface,
    adaptiveSurfaceContext,
  } = adaptiveSurfaceViewModel;

  useEffect(() => {
    sessionIdRef.current = currentSessionId ?? "";
    setActiveChildRunCount(null);
  }, [currentSessionId]);

  useEffect(() => {
    currentSessionBindingRef.current = currentSessionBinding;
  }, [currentSessionBinding]);

  useEffect(() => {
    if (!shellBindingTarget) {
      return;
    }

    if (!currentSessionId) {
      void clearShellBinding(shellBindingTarget);
      return;
    }

    void writeShellBinding(
      shellBindingTarget,
      currentSessionId,
      pendingYield ? "yielded" : shellRunStatus,
      Boolean(pendingYield),
    );
  }, [currentSessionId, pendingYield, shellBindingTarget, shellRunStatus]);

  useEffect(() => {
    if (pendingYield?.kind !== "dynamic_tool" && pendingYield?.kind !== "custom") {
      setSchemaFormState(null);
      return;
    }

    if (!pendingSchemaForm) {
      setSchemaFormState(null);
      return;
    }

    setSchemaFormState((previous) => {
      if (
        previous &&
        shouldReuseStructuredFormState(
          previous,
          pendingYield.requestId,
          pendingSchemaForm.questions,
        )
      ) {
        return previous;
      }
      return createStructuredFormState(pendingYield.requestId, pendingSchemaForm.questions);
    });
  }, [pendingSchemaForm, pendingYield]);

  useEffect(() => {
    if (pendingYield?.kind !== "structured_input") {
      setStructuredFormState(null);
      return;
    }

    const questions = structuredQuestions(pendingYield.payload);
    setStructuredFormState((previous) => {
      if (previous && shouldReuseStructuredFormState(previous, pendingYield.requestId, questions)) {
        return previous;
      }
      return createStructuredFormState(pendingYield.requestId, questions);
    });
  }, [pendingYield]);

  useEffect(() => {
    if (
      pendingYield?.kind !== "structured_input" ||
      !structuredFormState ||
      !activeStructuredQuestion
    ) {
      return;
    }

    setInputValue((previous) => {
      if (previous.startsWith("/")) {
        return previous;
      }

      if (!usesTextEntryKind(activeStructuredQuestion.kind)) {
        return "";
      }

      const answer = getStructuredAnswer(structuredFormState, activeStructuredQuestion);
      return typeof answer === "string" ? answer : "";
    });
  }, [activeStructuredQuestion, pendingYield, structuredFormState]);

  useEffect(() => {
    if (
      !pendingSchemaForm ||
      !schemaFormState ||
      !activeSchemaQuestion ||
      !usesTextEntryKind(activeSchemaQuestion.kind)
    ) {
      return;
    }

    setInputValue((previous) => {
      if (previous.startsWith("/")) {
        return previous;
      }

      const answer = getStructuredAnswer(schemaFormState, activeSchemaQuestion);
      return typeof answer === "string" ? answer : "";
    });
  }, [activeSchemaQuestion, pendingSchemaForm, schemaFormState]);

  const pushEvent = (event: EventEnvelope) => {
    setEvents((prev) => {
      const base =
        prev.length >= MAX_EVENT_HISTORY ? prev.slice(prev.length - MAX_EVENT_HISTORY + 1) : prev;
      return [...base, event];
    });
  };

  const addSystemEvent = (type: EventEnvelope["type"], message: string) => {
    pushEvent({
      event_id: crypto.randomUUID(),
      sequence: 0,
      session_id: sessionIdRef.current,
      turn_id: "system",
      item_id: "system",
      timestamp_ms: Date.now(),
      type,
      message,
    } as EventEnvelope);
  };

  const updateCurrentSessionBinding = (session: {
    profile_id?: string;
    provider?: ProviderId;
    resolved_model?: string;
  }) => {
    setCurrentSessionBinding({
      profileId: session.profile_id,
      provider: session.provider,
      resolvedModel: session.resolved_model,
    });
  };

  const addConnectionManagementHint = () => {
    const binding = currentSessionBindingRef.current;
    if (binding?.profileId && binding.provider === "chatgpt") {
      addSystemEvent(
        "system_message",
        `Current profile ${binding.profileId} uses ChatGPT / Codex and needs managed login.`,
      );
      addSystemEvent(
        "system_message",
        `Run /connection login ${binding.profileId} browser or /connection login ${binding.profileId} device.`,
      );
      return;
    }

    addSystemEvent(
      "system_message",
      "Hint: this looks like a connection credential or managed-login issue.",
    );
    addSystemEvent(
      "system_message",
      "Run /connection current to inspect selection state, then /connection show <profile-id>.",
    );
  };

  const printConnectionStatus = (
    profile: ConnectionProfileSummary,
    status?: ConnectionCredentialStatus,
  ) => {
    addSystemEvent("system_message", summarizeConnectionProfile(profile, status));
    if (status) {
      addSystemEvent("system_message", summarizeCredentialStatus(status));
    }
  };

  const announceConnectionLoginSuccess = (
    profileId: string,
    provider: ProviderId,
    email?: string,
    plan?: string,
  ) => {
    const details = [`${providerDisplayName(provider)} login complete for profile ${profileId}.`];
    if (email) {
      details.push(`email=${email}`);
    }
    if (plan) {
      details.push(`plan=${plan}`);
    }
    addSystemEvent("system_message", details.join(" "));
  };

  const announceConnectionSessionGap = (profileId: string, provider: ProviderId) => {
    const binding = currentSessionBindingRef.current;
    if (!binding?.profileId || binding.profileId === profileId) {
      return;
    }

    addSystemEvent(
      "system_warning",
      `${providerDisplayName(provider)} login complete for profile ${profileId}.`,
    );
    addSystemEvent(
      "system_warning",
      `Current session is still using profile ${binding.profileId} (${binding.provider ?? "unknown"}).`,
    );
    addSystemEvent(
      "system_warning",
      `Run /connection default set ${profileId} and create a new session to use it.`,
    );
  };

  const resolveTargetConnectionProfile = async (
    client: AlanClient,
    requestedProfileId?: string,
  ): Promise<string> => {
    if (requestedProfileId?.trim()) {
      return requestedProfileId.trim();
    }

    const currentProfileId = currentSessionBindingRef.current?.profileId;
    if (currentProfileId) {
      return currentProfileId;
    }

    const current = await client.getConnectionCurrent(detectWorkspaceDirForSelection());
    if (current.effective_profile) {
      return current.effective_profile;
    }

    const listing = await client.listConnections();
    if (listing.profiles.length === 1) {
      return listing.profiles[0].profile_id;
    }

    throw new Error(
      "No target profile selected. Pass a profile id, set a default with /connection default set <profile-id>, or pin one with /connection pin <profile-id>.",
    );
  };

  const watchConnectionBrowserLogin = async (
    client: AlanClient,
    profile: ConnectionProfileSummary,
    loginId: string,
    expiresAt: string,
  ): Promise<boolean> => {
    if (activeConnectionLoginsRef.current.has(loginId)) {
      return false;
    }
    activeConnectionLoginsRef.current.add(loginId);

    const deadline = Date.parse(expiresAt);

    try {
      while (Number.isNaN(deadline) || Date.now() <= deadline + 1_000) {
        const status = await client.getConnectionCredentialStatus(profile.profile_id);
        if (status.status === "available") {
          announceConnectionLoginSuccess(
            profile.profile_id,
            profile.provider,
            status.detail?.account_email,
            status.detail?.account_plan,
          );
          announceConnectionSessionGap(profile.profile_id, profile.provider);
          return true;
        }
        if (status.status === "error" || status.status === "expired") {
          addSystemEvent(
            "system_warning",
            `Login for profile ${profile.profile_id} ended with status=${status.status}${status.detail?.message ? ` (${status.detail.message})` : ""}.`,
          );
          return false;
        }
        await sleep(750);
      }

      addSystemEvent(
        "system_warning",
        `Login for profile ${profile.profile_id} is still pending. Use /connection show ${profile.profile_id} to inspect it.`,
      );
      return false;
    } catch (error) {
      addSystemEvent(
        "system_warning",
        `Connection login watcher paused: ${(error as Error).message}`,
      );
      return false;
    } finally {
      activeConnectionLoginsRef.current.delete(loginId);
    }
  };

  const startConnectionBrowserLogin = async (
    client: AlanClient,
    profileId: string,
    options: {
      source?: "command" | "setup";
      waitForAvailability?: boolean;
    } = {},
  ): Promise<boolean> => {
    const profile = await client.getConnection(profileId);
    const status = await client.getConnectionCredentialStatus(profileId);
    if (status.status === "available") {
      printConnectionStatus(profile, status);
      addSystemEvent("system_warning", `Profile ${profileId} already has available credentials.`);
      return true;
    }

    const start = await client.startConnectionBrowserLogin(profileId);
    addSystemEvent(
      "system_message",
      `${providerDisplayName(profile.provider)} browser login started for ${profileId} (${start.login_id}).`,
    );
    if (options.source === "setup") {
      addSystemEvent(
        "system_message",
        "First-time setup selected ChatGPT / Codex, so alan opened managed browser login automatically.",
      );
    }
    if (STARTUP_INFO.mode !== "embedded") {
      addSystemEvent(
        "system_message",
        "Browser login requires the daemon callback URL to be reachable from this browser. Use device mode if that is not true.",
      );
    }

    try {
      await openUrlInBrowser(start.auth_url);
      addSystemEvent("system_message", "Opened browser for managed login.");
    } catch (error) {
      addSystemEvent(
        "system_warning",
        `Could not open a browser automatically: ${(error as Error).message}`,
      );
    }

    addSystemEvent("system_message", `Auth URL: ${start.auth_url}`);
    if (options.waitForAvailability) {
      addSystemEvent(
        "system_message",
        "Waiting for managed login to complete before creating the first session...",
      );
      return await watchConnectionBrowserLogin(client, profile, start.login_id, start.expires_at);
    }

    void watchConnectionBrowserLogin(client, profile, start.login_id, start.expires_at);
    return false;
  };

  const completeConnectionDeviceLogin = async (
    client: AlanClient,
    profile: ConnectionProfileSummary,
    loginId: string,
  ) => {
    if (activeConnectionLoginsRef.current.has(loginId)) {
      return;
    }
    activeConnectionLoginsRef.current.add(loginId);

    try {
      const login = await client.completeConnectionDeviceLogin(profile.profile_id, loginId);
      announceConnectionLoginSuccess(
        profile.profile_id,
        profile.provider,
        login.email,
        login.plan_type,
      );
      announceConnectionSessionGap(profile.profile_id, profile.provider);
    } catch (error) {
      addSystemEvent(
        "system_error",
        `Device login for profile ${profile.profile_id} failed: ${(error as Error).message}`,
      );
    } finally {
      activeConnectionLoginsRef.current.delete(loginId);
    }
  };

  const announceYield = (incoming: PendingYield) => {
    const surface = getAdaptiveSurface(incoming);
    if (!surface) {
      return;
    }

    for (const message of surface.buildAnnouncement(incoming)) {
      addSystemEvent(message.type, message.message);
    }
  };

  const handleSetupComplete = (completion: InitWizardCompletion) => {
    pendingSetupBrowserLoginRef.current = completion.browserLoginProfileId;
    setNeedsSetup(false);
  };

  useEffect(() => {
    if (needsSetup) {
      return;
    }

    const client = new AlanClient({
      url: STARTUP_INFO.url,
      autoManageDaemon: AUTO_MANAGE,
      verbose: VERBOSE,
    });
    clientRef.current = client;

    const syncClientCapabilities = async (sessionId: string) => {
      try {
        await client.setClientCapabilities(sessionId, TUI_CLIENT_CAPABILITIES);
      } catch (error) {
        addSystemEvent(
          "system_warning",
          `Failed to sync adaptive UI capabilities: ${(error as Error).message}`,
        );
      }
    };

    client.on("connected", () => {
      setStatus("connected");
      if (sessionIdRef.current) {
        setShellRunStatus("ready");
      }
      setStatusMessage(
        STARTUP_INFO.mode === "embedded" ? "Ready" : `Connected to ${STARTUP_INFO.url}`,
      );

      if (sessionIdRef.current) {
        void syncClientCapabilities(sessionIdRef.current);
      }
    });

    client.on("disconnected", () => {
      setStatus("error");
      setShellRunStatus("error");
      setStatusMessage("Disconnected");
      addSystemEvent("system_message", "Disconnected from agent");
    });

    client.on("error", (error: Error) => {
      setStatus("error");
      setShellRunStatus("error");
      setStatusMessage(`Error: ${error.message}`);
      addSystemEvent("system_error", error.message);
    });

    client.on("event", (envelope: EventEnvelope) => {
      pushEvent(envelope);
      setCurrentPlan((previous) =>
        reduceCurrentPlanState(previous, envelope, sessionIdRef.current || null),
      );
      setCurrentRuntimeState((previous) =>
        reduceCurrentRuntimeState(previous, envelope, sessionIdRef.current || null),
      );

      if (envelope.type === "turn_started") {
        setPendingYield(null);
        setConfirmationActionRequestId(null);
        setConfirmationActionIndex(0);
        setShellRunStatus("running");
      }

      if (envelope.type === "turn_completed") {
        setPendingYield(null);
        setConfirmationActionRequestId(null);
        setConfirmationActionIndex(0);
        setShellRunStatus("ready");
      }

      if (envelope.type === "yield" && envelope.request_id) {
        const incoming: PendingYield = {
          requestId: envelope.request_id,
          kind: parsePendingYieldKind(envelope.kind),
          payload: envelope.payload,
        };
        if (incoming.kind === "confirmation") {
          const options = confirmationActionOptions(incoming.payload);
          setConfirmationActionRequestId(incoming.requestId);
          setConfirmationActionIndex(
            preferredConfirmationActionIndex(options, confirmationDefaultOption(incoming.payload)),
          );
        } else {
          setConfirmationActionRequestId(null);
          setConfirmationActionIndex(0);
        }
        setPendingYield(incoming);
        setShellRunStatus("yielded");
        announceYield(incoming);
      }

      if (envelope.type === "error") {
        setShellRunStatus("error");
      }
    });

    client.on("session_created", (sessionId: string) => {
      setCurrentSessionId(sessionId);
      setCurrentPlan(null);
      setCurrentRuntimeState(createCurrentRuntimeState());
      setConfirmationActionRequestId(null);
      setConfirmationActionIndex(0);
      setShellRunStatus("starting");
      addSystemEvent("session_created", sessionId);
    });

    const detectWorkspaceDir = (): string | undefined => {
      const workspaceDir = detectWorkspaceDirForSelection();
      if (workspaceDir) {
        addSystemEvent("system_message", `Detected workspace: ${workspaceDir}`);
        return workspaceDir;
      }

      return undefined;
    };

    const init = async () => {
      try {
        setStatusMessage(
          STARTUP_INFO.mode === "embedded" ? "Starting daemon..." : "Connecting to agent...",
        );

        await client.connect();

        if (AUTO_MANAGE) {
          const latestStatus = client.getDaemonStatus();
          if (latestStatus) {
            setDaemonStatus(latestStatus);
          }
        }

        const setupBrowserLoginProfileId = pendingSetupBrowserLoginRef.current;
        pendingSetupBrowserLoginRef.current = null;
        if (setupBrowserLoginProfileId) {
          try {
            const completed = await startConnectionBrowserLogin(
              client,
              setupBrowserLoginProfileId,
              {
                source: "setup",
                waitForAvailability: true,
              },
            );
            if (!completed) {
              addSystemEvent(
                "system_warning",
                "Managed login did not complete yet. Session creation may still require login.",
              );
            }
          } catch (error) {
            addSystemEvent(
              "system_error",
              `Failed to start first-time browser login: ${(error as Error).message}`,
            );
          }
        }

        try {
          const workspaceDir = detectWorkspaceDir();

          if (workspaceDir) {
            addSystemEvent("system_message", `Creating session for workspace: ${workspaceDir}...`);
          } else {
            addSystemEvent("system_message", "Auto-creating session on default workspace...");
          }

          const session = await client.createSession({
            workspace_dir: workspaceDir,
            agent_name: DEFAULT_AGENT_NAME ?? undefined,
          });
          const sessionId = session.session_id;
          sessionIdRef.current = sessionId;
          setCurrentPlan(null);
          setCurrentRuntimeState(createCurrentRuntimeState());
          setConfirmationActionRequestId(null);
          setConfirmationActionIndex(0);
          setCurrentSessionId(sessionId);
          updateCurrentSessionBinding(session);
          await client.connectToSession(sessionId);
          setShellRunStatus("ready");
          addSystemEvent(
            "system_message",
            `alan ready. Type your request directly or /help. (${session.agent_name ? `agent=${session.agent_name}, ` : ""}${summarizeSessionBinding(
              {
                profileId: session.profile_id,
                provider: session.provider,
                resolvedModel: session.resolved_model,
              },
            )}, streaming=${session.streaming_mode}, recovery=${session.partial_stream_recovery_mode})`,
          );
        } catch (error) {
          const msg = (error as Error).message;
          addSystemEvent("system_error", msg);

          if (isChatgptNotLoggedInMessage(msg)) {
            addConnectionManagementHint();
          } else if (
            msg.includes("LLM") ||
            msg.includes("llm") ||
            msg.includes("model") ||
            msg.includes("key")
          ) {
            addSystemEvent("system_message", "Hint: this looks like an LLM configuration issue.");
            addSystemEvent(
              "system_message",
              `Please check ${CONFIG_PATH_HINT} (or ALAN_CONFIG_PATH).`,
            );
          } else if (msg.includes("500") || msg.includes("Internal Server Error")) {
            addSystemEvent(
              "system_message",
              "Hint: daemon internal error, please check daemon logs.",
            );
          }
          addSystemEvent("system_message", "Type /new to try again, or /help for commands.");
        }
      } catch (error) {
        const message = (error as Error).message;
        setStatus("error");
        setStatusMessage(`Failed: ${message}`);
        addSystemEvent("system_error", `Connection failed: ${message}`);

        if (STARTUP_INFO.mode === "embedded") {
          addSystemEvent(
            "system_message",
            "Make sure `alan` is installed and on PATH (try: just install).",
          );
        }
      }
    };

    void init();

    const cleanup = async () => {
      await clearShellBinding(shellBindingTarget);
      await client.shutdown();
    };

    const handleExit = () => {
      void cleanup().then(() => {
        process.exit(0);
      });
    };

    process.on("SIGINT", handleExit);
    process.on("SIGTERM", handleExit);

    return () => {
      process.off("SIGINT", handleExit);
      process.off("SIGTERM", handleExit);
      void cleanup();
    };
  }, [needsSetup, shellBindingTarget]);

  useInput((input, key) => {
    if (key.ctrl && input === "c") {
      app.exit();
      return;
    }

    if (key.ctrl && input === "l") {
      setEvents([]);
      addSystemEvent("system_message", "Timeline cleared.");
      return;
    }

    if (!isAdaptiveSurfaceReadyForInput(adaptiveSurfaceViewModel)) {
      return;
    }

    const handleAdaptiveInputKey = activeSurface?.handleInputKey;
    if (!pendingYield || !activeSurface || !adaptiveSurfaceContext || !handleAdaptiveInputKey) {
      return;
    }

    if (
      handleAdaptiveInputKey({
        input,
        key,
        inputValue,
        ...adaptiveSurfaceContext,
        setInputValue,
        addSystemEvent,
        submitPendingYield: (content) => {
          void submitPendingYield(content);
        },
        confirmationControls:
          pendingYield.kind === "confirmation"
            ? {
                setActionIndex: setConfirmationActionIndex,
              }
            : undefined,
        schemaFormControls: pendingSchemaForm
          ? {
              setFormState: setSchemaFormState,
              submitForm: () => {
                void submitSchemaForm();
              },
              confirmActiveQuestion: () => {
                void confirmActiveSchemaQuestion();
              },
            }
          : undefined,
        structuredInputControls:
          pendingYield.kind === "structured_input"
            ? {
                setFormState: setStructuredFormState,
                submitStructuredForm: () => {
                  void submitStructuredForm();
                },
                confirmActiveQuestion: () => {
                  void confirmActiveStructuredQuestion();
                },
              }
            : undefined,
      })
    ) {
      return;
    }
  });

  const submitPendingYield = async (content: unknown) => {
    const client = clientRef.current;
    if (!client || !currentSessionId || !pendingYield) {
      addSystemEvent("system_warning", "No pending yield to resume.");
      return;
    }

    try {
      setShellRunStatus("running");
      await client.resume(currentSessionId, pendingYield.requestId, content);
      addSystemEvent("system_message", `Resumed ${pendingYield.kind} (${pendingYield.requestId}).`);
      setPendingYield(null);
      setConfirmationActionRequestId(null);
      setConfirmationActionIndex(0);
    } catch (error) {
      addSystemEvent("system_error", `Failed to resume: ${(error as Error).message}`);
    }
  };

  const handleInputChange = (nextValue: string) => {
    setInputValue(nextValue);

    if (
      pendingYield?.kind === "structured_input" &&
      structuredFormState &&
      activeStructuredQuestion &&
      usesTextEntryKind(activeStructuredQuestion.kind) &&
      !nextValue.startsWith("/")
    ) {
      setStructuredFormState((previous) =>
        previous
          ? setStructuredTextAnswer(previous, activeStructuredQuestion.id, nextValue)
          : previous,
      );
    }

    if (
      pendingSchemaForm &&
      schemaFormState &&
      activeSchemaQuestion &&
      usesTextEntryKind(activeSchemaQuestion.kind) &&
      !nextValue.startsWith("/")
    ) {
      setSchemaFormState((previous) =>
        previous ? setStructuredTextAnswer(previous, activeSchemaQuestion.id, nextValue) : previous,
      );
    }
  };

  const submitStructuredForm = async (overrideState?: StructuredFormState) => {
    if (pendingYield?.kind !== "structured_input" || !structuredFormState) {
      addSystemEvent("system_warning", "No pending structured input request.");
      return;
    }

    const formState = overrideState ?? structuredFormState;

    const error = structuredFormValidationError(formState, pendingStructuredQuestions);
    if (error) {
      addSystemEvent("system_warning", error);
      return;
    }

    await submitPendingYield(buildStructuredResumePayload(formState, pendingStructuredQuestions));
  };

  const submitSchemaForm = async (overrideState?: StructuredFormState) => {
    if (!pendingSchemaForm || !schemaFormState) {
      addSystemEvent("system_warning", "No schema-driven yield request.");
      return;
    }

    const formState = overrideState ?? schemaFormState;
    const error = structuredFormValidationError(formState, pendingSchemaForm.questions);
    if (error) {
      addSystemEvent("system_warning", error);
      return;
    }

    const result = buildSchemaDrivenYieldPayload(formState, pendingSchemaForm);
    if ("error" in result) {
      addSystemEvent("system_warning", result.error);
      return;
    }

    await submitPendingYield(result.payload);
  };

  const confirmActiveStructuredQuestion = async (overrideState?: StructuredFormState) => {
    if (
      pendingYield?.kind !== "structured_input" ||
      !structuredFormState ||
      !activeStructuredQuestion
    ) {
      return;
    }

    const baseState = overrideState ?? structuredFormState;
    const nextState = baseState;
    const error = questionValidationError(nextState, activeStructuredQuestion);
    if (error) {
      addSystemEvent("system_warning", `${activeStructuredQuestion.label}: ${error}`);
      return;
    }

    if (nextState.activeQuestionIndex >= pendingStructuredQuestions.length - 1) {
      await submitStructuredForm(nextState);
      return;
    }

    setStructuredFormState((previous) =>
      previous ? moveStructuredQuestion(nextState, pendingStructuredQuestions, 1) : previous,
    );
  };

  const confirmActiveSchemaQuestion = async (overrideState?: StructuredFormState) => {
    if (!pendingSchemaForm || !schemaFormState || !activeSchemaQuestion) {
      return;
    }

    const baseState = overrideState ?? schemaFormState;
    const error = questionValidationError(baseState, activeSchemaQuestion);
    if (error) {
      addSystemEvent("system_warning", `${activeSchemaQuestion.label}: ${error}`);
      return;
    }

    if (baseState.activeQuestionIndex >= pendingSchemaForm.questions.length - 1) {
      await submitSchemaForm(baseState);
      return;
    }

    setSchemaFormState((previous) =>
      previous ? moveStructuredQuestion(baseState, pendingSchemaForm.questions, 1) : previous,
    );
  };

  const handleSubmit = async (text: string) => {
    const client = clientRef.current;
    if (!client) return;

    const trimmed = text.trim();
    if (!trimmed) return;

    setInputValue("");
    addSystemEvent("user_message", trimmed);

    if (trimmed.startsWith("/")) {
      await handleCommand(trimmed, client);
      return;
    }

    if (!currentSessionId) {
      addSystemEvent("system_warning", "No active session. Use /new to create one.");
      return;
    }

    if (pendingYield) {
      if (
        pendingYield.kind === "structured_input" &&
        structuredFormState &&
        activeStructuredQuestion &&
        usesTextEntryKind(activeStructuredQuestion.kind)
      ) {
        const nextFormState = setStructuredTextAnswer(
          structuredFormState,
          activeStructuredQuestion.id,
          trimmed,
        );
        setStructuredFormState(nextFormState);
        await confirmActiveStructuredQuestion(nextFormState);
        setInputValue("");
        return;
      }

      if (
        (pendingYield.kind === "dynamic_tool" || pendingYield.kind === "custom") &&
        pendingSchemaForm &&
        schemaFormState &&
        activeSchemaQuestion &&
        usesTextEntryKind(activeSchemaQuestion.kind)
      ) {
        const nextFormState = setStructuredTextAnswer(
          schemaFormState,
          activeSchemaQuestion.id,
          trimmed,
        );
        setSchemaFormState(nextFormState);
        await confirmActiveSchemaQuestion(nextFormState);
        setInputValue("");
        return;
      }

      addSystemEvent(
        "system_warning",
        pendingYield.kind === "structured_input"
          ? "Structured input is pending. Use the Action panel, /answers <json-array>, or /resume <json>."
          : pendingSchemaForm
            ? "Schema-driven yield is pending. Use the Action panel or /resume <json>."
            : "Yield is pending. Resolve it first (/approve, /reject, /modify, /answer, /answers, /resume).",
      );
      return;
    }

    try {
      setShellRunStatus("running");
      await client.sendMessage(currentSessionId, trimmed);
    } catch (error) {
      addSystemEvent("system_error", `Failed to send: ${(error as Error).message}`);
    }
  };

  const activeWorkspaceDir = (): string | undefined => detectWorkspaceDirForSelection();

  const printConnectionCurrentState = (current: ConnectionCurrentState) => {
    for (const line of summarizeConnectionCurrentState(current)) {
      addSystemEvent("system_message", line);
    }
    if (currentSessionBindingRef.current) {
      addSystemEvent(
        "system_message",
        `Current session ${summarizeSessionBinding(currentSessionBindingRef.current)}`,
      );
    }
  };

  const announceCurrentSessionSelectionGap = (profileId: string) => {
    const binding = currentSessionBindingRef.current;
    if (!binding?.profileId || binding.profileId === profileId) {
      return;
    }
    addSystemEvent(
      "system_warning",
      `Current session is still using profile ${binding.profileId}. Create a new session to use ${profileId}.`,
    );
  };

  const announceEffectiveSelection = (
    current: ConnectionCurrentState,
    requestedProfileId?: string,
  ) => {
    printConnectionCurrentState(current);
    if (
      requestedProfileId &&
      current.effective_profile &&
      current.effective_profile !== requestedProfileId
    ) {
      addSystemEvent(
        "system_warning",
        `Default profile is ${requestedProfileId}, but effective new-session profile remains ${current.effective_profile} because a pin overrides it.`,
      );
    }
    if (current.effective_profile) {
      announceCurrentSessionSelectionGap(current.effective_profile);
    }
  };

  const handleConnectionCommand = async (args: string[], client: AlanClient) => {
    const usage =
      "Usage: /connection <list|show|current|add|login|logout|set-secret|default|pin|unpin|status|test|remove> ...";
    const action = args[0]?.toLowerCase();

    if (!action) {
      addSystemEvent("system_warning", usage);
      return;
    }

    switch (action) {
      case "list": {
        try {
          const current = await client.getConnectionCurrent(activeWorkspaceDir());
          printConnectionCurrentState(current);
          const listing = await client.listConnections();
          if (listing.profiles.length === 0) {
            addSystemEvent(
              "system_warning",
              "No connection profiles configured. Use /connection add <provider> or rerun onboarding.",
            );
            return;
          }
          for (const profile of listing.profiles) {
            let status: ConnectionCredentialStatus | undefined;
            try {
              status = await client.getConnectionCredentialStatus(profile.profile_id);
            } catch {
              status = undefined;
            }
            printConnectionStatus(profile, status);
          }
        } catch (error) {
          addSystemEvent("system_error", `Failed to list connections: ${(error as Error).message}`);
        }
        return;
      }

      case "current": {
        try {
          const current = await client.getConnectionCurrent(activeWorkspaceDir());
          printConnectionCurrentState(current);
        } catch (error) {
          addSystemEvent(
            "system_error",
            `Failed to inspect connection selection: ${(error as Error).message}`,
          );
        }
        return;
      }

      case "show": {
        try {
          const profileId = args[1];
          if (!profileId) {
            addSystemEvent("system_warning", "Usage: /connection show <profile-id>");
            return;
          }
          const profile = await client.getConnection(profileId);
          const status = await client.getConnectionCredentialStatus(profileId);
          printConnectionStatus(profile, status);
          if (currentSessionBindingRef.current) {
            addSystemEvent(
              "system_message",
              `Current session ${summarizeSessionBinding(currentSessionBindingRef.current)}`,
            );
          }
        } catch (error) {
          addSystemEvent(
            "system_error",
            `Failed to inspect connection: ${(error as Error).message}`,
          );
        }
        return;
      }

      case "status": {
        const requestedProfileId = args[1];
        if (!requestedProfileId) {
          try {
            const current = await client.getConnectionCurrent(activeWorkspaceDir());
            printConnectionCurrentState(current);
          } catch (error) {
            addSystemEvent(
              "system_error",
              `Failed to inspect connection selection: ${(error as Error).message}`,
            );
          }
          return;
        }
        try {
          const profile = await client.getConnection(requestedProfileId);
          const status = await client.getConnectionCredentialStatus(requestedProfileId);
          printConnectionStatus(profile, status);
        } catch (error) {
          addSystemEvent(
            "system_error",
            `Failed to inspect connection: ${(error as Error).message}`,
          );
        }
        return;
      }

      case "add": {
        const providerId = args[1]?.toLowerCase() as ProviderId | undefined;
        if (!providerId) {
          addSystemEvent(
            "system_warning",
            "Usage: /connection add <provider> [profile=<id>] [label=<text>] [credential=<id>] [default] [setting=value ...]",
          );
          return;
        }

        try {
          const catalog = await client.getConnectionCatalog();
          const descriptor = catalog.providers.find(
            (provider) => provider.provider_id === providerId,
          );
          if (!descriptor) {
            addSystemEvent(
              "system_warning",
              `Unknown provider ${providerId}. Run /connection list or onboarding to inspect supported providers.`,
            );
            return;
          }

          let profileId =
            providerId === "chatgpt" ? "chatgpt-main" : providerId.replaceAll("_", "-");
          let label: string | undefined;
          let credentialId: string | undefined;
          let activate = false;
          const settings: Record<string, string> = {};

          for (const token of args.slice(2)) {
            if (token === "activate" || token === "default") {
              activate = true;
              continue;
            }
            const separator = token.indexOf("=");
            if (separator <= 0) {
              addSystemEvent(
                "system_warning",
                "Usage: /connection add <provider> [profile=<id>] [label=<text>] [credential=<id>] [default] [setting=value ...]",
              );
              return;
            }
            const key = token.slice(0, separator).toLowerCase();
            const value = token.slice(separator + 1);
            switch (key) {
              case "profile":
              case "profile_id":
                profileId = value;
                break;
              case "label":
                label = value;
                break;
              case "credential":
              case "credential_id":
                credentialId = value;
                break;
              case "activate":
              case "default":
                activate = value !== "false";
                break;
              default:
                settings[key] = value;
                break;
            }
          }

          const profile = await client.createConnection({
            profile_id: profileId,
            label,
            provider: descriptor.provider_id,
            credential_id: credentialId,
            settings,
            activate,
          });
          let status: ConnectionCredentialStatus | undefined;
          try {
            status = await client.getConnectionCredentialStatus(profile.profile_id);
          } catch {
            status = undefined;
          }
          addSystemEvent("system_message", `Created connection profile ${profile.profile_id}.`);
          printConnectionStatus(profile, status);
          if (descriptor.credential_kind === "secret_string") {
            addSystemEvent(
              "system_message",
              `Run /connection set-secret ${profile.profile_id} <secret> to store credentials.`,
            );
          } else if (descriptor.credential_kind === "managed_oauth") {
            addSystemEvent(
              "system_message",
              `Run /connection login ${profile.profile_id} browser or /connection login ${profile.profile_id} device.`,
            );
          }
          if (activate) {
            const current = await client.getConnectionCurrent(activeWorkspaceDir());
            announceEffectiveSelection(current, profile.profile_id);
          }
        } catch (error) {
          addSystemEvent("system_error", `Failed to add connection: ${(error as Error).message}`);
        }
        return;
      }

      case "default": {
        const subaction = args[1]?.toLowerCase();
        if (subaction === "set") {
          const profileId = args[2];
          if (!profileId) {
            addSystemEvent("system_warning", "Usage: /connection default set <profile-id>");
            return;
          }
          try {
            const current = await client.setConnectionDefault({
              profile_id: profileId,
              workspace_dir: activeWorkspaceDir(),
            });
            addSystemEvent("system_message", `Default profile set to ${profileId}.`);
            announceEffectiveSelection(current, profileId);
          } catch (error) {
            addSystemEvent(
              "system_error",
              `Failed to set default profile ${profileId}: ${(error as Error).message}`,
            );
          }
          return;
        }

        if (subaction === "clear") {
          try {
            const current = await client.clearConnectionDefault({
              workspace_dir: activeWorkspaceDir(),
            });
            addSystemEvent("system_message", "Cleared default profile.");
            announceEffectiveSelection(current);
          } catch (error) {
            addSystemEvent(
              "system_error",
              `Failed to clear default profile: ${(error as Error).message}`,
            );
          }
          return;
        }

        addSystemEvent("system_warning", "Usage: /connection default <set|clear> ...");
        return;
      }

      case "pin": {
        const profileId = args[1];
        if (!profileId) {
          addSystemEvent(
            "system_warning",
            "Usage: /connection pin <profile-id> [scope=global|workspace] [workspace=<path>]",
          );
          return;
        }

        let scope: ConnectionPinScope = "global";
        let workspaceDir: string | undefined;
        for (const token of args.slice(2)) {
          const separator = token.indexOf("=");
          if (separator <= 0) {
            addSystemEvent(
              "system_warning",
              "Usage: /connection pin <profile-id> [scope=global|workspace] [workspace=<path>]",
            );
            return;
          }
          const key = token.slice(0, separator).toLowerCase();
          const value = token.slice(separator + 1);
          switch (key) {
            case "scope":
              if (value !== "global" && value !== "workspace") {
                addSystemEvent(
                  "system_warning",
                  "Usage: /connection pin <profile-id> [scope=global|workspace] [workspace=<path>]",
                );
                return;
              }
              scope = value;
              break;
            case "workspace":
            case "workspace_dir":
              workspaceDir = value;
              break;
            default:
              addSystemEvent(
                "system_warning",
                "Usage: /connection pin <profile-id> [scope=global|workspace] [workspace=<path>]",
              );
              return;
          }
        }

        try {
          const current = await client.pinConnection({
            profile_id: profileId,
            scope,
            workspace_dir:
              scope === "workspace" ? (workspaceDir ?? activeWorkspaceDir()) : undefined,
          });
          addSystemEvent("system_message", `Pinned profile ${profileId} at ${scope} scope.`);
          announceEffectiveSelection(current, profileId);
        } catch (error) {
          addSystemEvent(
            "system_error",
            `Failed to pin profile ${profileId}: ${(error as Error).message}`,
          );
        }
        return;
      }

      case "unpin": {
        let scope: ConnectionPinScope = "global";
        let workspaceDir: string | undefined;
        for (const token of args.slice(1)) {
          const separator = token.indexOf("=");
          if (separator <= 0) {
            addSystemEvent(
              "system_warning",
              "Usage: /connection unpin [scope=global|workspace] [workspace=<path>]",
            );
            return;
          }
          const key = token.slice(0, separator).toLowerCase();
          const value = token.slice(separator + 1);
          switch (key) {
            case "scope":
              if (value !== "global" && value !== "workspace") {
                addSystemEvent(
                  "system_warning",
                  "Usage: /connection unpin [scope=global|workspace] [workspace=<path>]",
                );
                return;
              }
              scope = value;
              break;
            case "workspace":
            case "workspace_dir":
              workspaceDir = value;
              break;
            default:
              addSystemEvent(
                "system_warning",
                "Usage: /connection unpin [scope=global|workspace] [workspace=<path>]",
              );
              return;
          }
        }

        try {
          const current = await client.unpinConnection({
            scope,
            workspace_dir:
              scope === "workspace" ? (workspaceDir ?? activeWorkspaceDir()) : undefined,
          });
          addSystemEvent("system_message", `Cleared ${scope} pin.`);
          announceEffectiveSelection(current);
        } catch (error) {
          addSystemEvent(
            "system_error",
            `Failed to clear ${scope} pin: ${(error as Error).message}`,
          );
        }
        return;
      }

      case "set-secret": {
        const profileId = args[1];
        const secret = args.slice(2).join(" ").trim();
        if (!profileId || !secret) {
          addSystemEvent("system_warning", "Usage: /connection set-secret <profile-id> <secret>");
          return;
        }

        try {
          const profile = await client.getConnection(profileId);
          const status = await client.setConnectionSecret(profileId, secret);
          addSystemEvent("system_message", `Stored secret for profile ${profileId}.`);
          printConnectionStatus(profile, status);
        } catch (error) {
          addSystemEvent(
            "system_error",
            `Failed to store secret for ${profileId}: ${(error as Error).message}`,
          );
        }
        return;
      }

      case "login": {
        const profileId = args[1];
        const mode = args[2]?.toLowerCase() ?? "browser";
        if (!profileId) {
          addSystemEvent(
            "system_warning",
            "Usage: /connection login <profile-id> [browser|device]",
          );
          return;
        }
        if (mode !== "browser" && mode !== "device") {
          addSystemEvent(
            "system_warning",
            "Usage: /connection login <profile-id> [browser|device]",
          );
          return;
        }

        try {
          if (mode === "browser") {
            await startConnectionBrowserLogin(client, profileId);
            return;
          }

          const profile = await client.getConnection(profileId);
          const status = await client.getConnectionCredentialStatus(profileId);
          if (status.status === "available") {
            printConnectionStatus(profile, status);
            addSystemEvent(
              "system_warning",
              `Profile ${profileId} already has available credentials.`,
            );
            return;
          }

          if (mode === "device") {
            const start = await client.startConnectionDeviceLogin(profileId);
            const expiresAt = formatTimestamp(start.expires_at);
            addSystemEvent(
              "system_message",
              `${providerDisplayName(profile.provider)} device login started for ${profileId} (${start.login_id}).`,
            );
            addSystemEvent("system_message", `Open: ${start.verification_url}`);
            addSystemEvent("system_message", `Enter code: ${start.user_code}`);
            if (expiresAt) {
              addSystemEvent("system_message", `Code expires: ${expiresAt}`);
            }
            addSystemEvent("system_message", "Waiting for device approval to complete...");
            void completeConnectionDeviceLogin(client, profile, start.login_id);
            return;
          }
        } catch (error) {
          addSystemEvent(
            "system_error",
            `Failed to start connection login: ${(error as Error).message}`,
          );
        }
        return;
      }

      case "logout": {
        const profileId = args[1];
        if (!profileId) {
          addSystemEvent("system_warning", "Usage: /connection logout <profile-id>");
          return;
        }

        try {
          const profile = await client.getConnection(profileId);
          const result = await client.logoutConnection(profileId);
          const status = await client.getConnectionCredentialStatus(profileId);
          addSystemEvent(
            "system_message",
            result.removed
              ? `Removed credentials for profile ${profileId}.`
              : `No removable credentials were present for profile ${profileId}.`,
          );
          printConnectionStatus(profile, status);
        } catch (error) {
          addSystemEvent(
            "system_error",
            `Failed to logout ${profileId}: ${(error as Error).message}`,
          );
        }
        return;
      }

      case "activate":
      case "use": {
        const profileId = args[1];
        if (!profileId) {
          addSystemEvent("system_warning", "Usage: /connection default set <profile-id>");
          return;
        }

        try {
          addSystemEvent(
            "system_warning",
            `/connection ${action} is deprecated. Use /connection default set ${profileId}.`,
          );
          const current = await client.setConnectionDefault({
            profile_id: profileId,
            workspace_dir: activeWorkspaceDir(),
          });
          addSystemEvent("system_message", `Default profile set to ${profileId}.`);
          announceEffectiveSelection(current, profileId);
        } catch (error) {
          addSystemEvent(
            "system_error",
            `Failed to set default profile ${profileId}: ${(error as Error).message}`,
          );
        }
        return;
      }

      case "test": {
        try {
          const profileId = await resolveTargetConnectionProfile(client, args[1]);
          const result = await client.testConnection(profileId);
          addSystemEvent(
            result.ok ? "system_message" : "system_warning",
            `${result.message} profile=${result.profile_id} provider=${result.provider} model=${result.resolved_model}`,
          );
        } catch (error) {
          addSystemEvent("system_error", `Failed to test connection: ${(error as Error).message}`);
        }
        return;
      }

      case "remove": {
        const profileId = args[1];
        if (!profileId) {
          addSystemEvent("system_warning", "Usage: /connection remove <profile-id>");
          return;
        }
        try {
          const removed = await client.removeConnection(profileId);
          addSystemEvent(
            removed ? "system_message" : "system_warning",
            removed
              ? `Removed connection profile ${profileId}.`
              : `Connection profile ${profileId} was not present.`,
          );
        } catch (error) {
          addSystemEvent(
            "system_error",
            `Failed to remove ${profileId}: ${(error as Error).message}`,
          );
        }
        return;
      }

      default:
        addSystemEvent("system_warning", usage);
    }
  };

  const handleCommand = async (text: string, client: AlanClient) => {
    const [rawCmd, ...args] = text.slice(1).split(" ");
    const cmd = rawCmd.toLowerCase();

    switch (cmd) {
      case "connection":
        await handleConnectionCommand(args, client);
        break;

      case "new": {
        let requestedProfile: "autonomous" | "conservative" | null = null;
        let requestedStreaming: "auto" | "on" | "off" | null = null;
        let requestedRecovery: "continue_once" | "off" | null = null;
        let requestedAgentName: string | null = null;
        let requestedConnectionProfileId: string | null = null;

        for (const arg of args.filter(Boolean)) {
          const agentName = parseAgentSelector(arg);
          if (agentName) {
            if (requestedAgentName && requestedAgentName !== agentName) {
              addSystemEvent(
                "system_warning",
                "Usage: /new [agent=<name>] [profile=<connection-profile>] [autonomous|conservative] [auto|on|off] [continue_once|recovery=off]",
              );
              return;
            }
            requestedAgentName = agentName;
            continue;
          }

          const profile = parseGovernanceProfile(arg);
          if (profile) {
            if (requestedProfile && requestedProfile !== profile) {
              addSystemEvent(
                "system_warning",
                "Usage: /new [agent=<name>] [profile=<connection-profile>] [autonomous|conservative] [auto|on|off] [continue_once|recovery=off]",
              );
              return;
            }
            requestedProfile = profile;
            continue;
          }

          const streaming = parseStreamingMode(arg);
          if (streaming) {
            if (requestedStreaming && requestedStreaming !== streaming) {
              addSystemEvent(
                "system_warning",
                "Usage: /new [agent=<name>] [profile=<connection-profile>] [autonomous|conservative] [auto|on|off] [continue_once|recovery=off]",
              );
              return;
            }
            requestedStreaming = streaming;
            continue;
          }

          const recovery = parsePartialStreamRecoveryMode(arg);
          if (recovery) {
            if (requestedRecovery && requestedRecovery !== recovery) {
              addSystemEvent(
                "system_warning",
                "Usage: /new [agent=<name>] [profile=<connection-profile>] [autonomous|conservative] [auto|on|off] [continue_once|recovery=off]",
              );
              return;
            }
            requestedRecovery = recovery;
            continue;
          }

          if (arg.startsWith("profile=") || arg.startsWith("connection=")) {
            const profileId = arg.slice(arg.indexOf("=") + 1).trim();
            if (!profileId) {
              addSystemEvent(
                "system_warning",
                "Usage: /new [agent=<name>] [profile=<connection-profile>] [autonomous|conservative] [auto|on|off] [continue_once|recovery=off]",
              );
              return;
            }
            if (requestedConnectionProfileId && requestedConnectionProfileId !== profileId) {
              addSystemEvent(
                "system_warning",
                "Usage: /new [agent=<name>] [profile=<connection-profile>] [autonomous|conservative] [auto|on|off] [continue_once|recovery=off]",
              );
              return;
            }
            requestedConnectionProfileId = profileId;
            continue;
          }

          addSystemEvent(
            "system_warning",
            "Usage: /new [agent=<name>] [profile=<connection-profile>] [autonomous|conservative] [auto|on|off] [continue_once|recovery=off]",
          );
          return;
        }

        try {
          addSystemEvent("system_message", "Creating new session...");
          const createRequest: {
            agent_name?: string;
            profile_id?: string;
            governance?: { profile: "autonomous" | "conservative" };
            streaming_mode?: "auto" | "on" | "off";
            partial_stream_recovery_mode?: "continue_once" | "off";
          } = {};
          if (requestedAgentName || DEFAULT_AGENT_NAME) {
            createRequest.agent_name = requestedAgentName ?? DEFAULT_AGENT_NAME ?? undefined;
          }
          if (requestedProfile) {
            createRequest.governance = { profile: requestedProfile };
          }
          if (requestedConnectionProfileId) {
            createRequest.profile_id = requestedConnectionProfileId;
          }
          if (requestedStreaming) {
            createRequest.streaming_mode = requestedStreaming;
          }
          if (requestedRecovery) {
            createRequest.partial_stream_recovery_mode = requestedRecovery;
          }

          const session = await client.createSession(
            Object.keys(createRequest).length > 0 ? createRequest : undefined,
          );
          const sessionId = session.session_id;
          sessionIdRef.current = sessionId;
          setCurrentPlan(null);
          setCurrentRuntimeState(createCurrentRuntimeState());
          setConfirmationActionRequestId(null);
          setConfirmationActionIndex(0);
          setCurrentSessionId(sessionId);
          updateCurrentSessionBinding(session);
          setPendingYield(null);
          await client.connectToSession(sessionId);
          setShellRunStatus("ready");
          addSystemEvent(
            "system_message",
            `Session ready (${shortId(sessionId)}), ${session.agent_name ? `agent=${session.agent_name}, ` : ""}${summarizeSessionBinding(
              {
                profileId: session.profile_id,
                provider: session.provider,
                resolvedModel: session.resolved_model,
              },
            )}, governance=${session.governance.profile}, streaming=${session.streaming_mode}, recovery=${session.partial_stream_recovery_mode}.`,
          );
        } catch (error) {
          const msg = (error as Error).message;
          addSystemEvent("system_error", msg);

          if (isChatgptNotLoggedInMessage(msg)) {
            addConnectionManagementHint();
          } else if (
            msg.includes("LLM") ||
            msg.includes("llm") ||
            msg.includes("model") ||
            msg.includes("key")
          ) {
            addSystemEvent("system_message", "Hint: this looks like an LLM configuration issue.");
            addSystemEvent(
              "system_message",
              `Please check ${CONFIG_PATH_HINT} (or ALAN_CONFIG_PATH).`,
            );
          } else if (msg.includes("500") || msg.includes("Internal Server Error")) {
            addSystemEvent(
              "system_message",
              "Hint: daemon internal error, please check daemon logs.",
            );
          }
        }
        break;
      }

      case "connect":
        if (!args[0]) {
          addSystemEvent("system_warning", "Usage: /connect <session-id>");
          return;
        }
        const targetSessionId = args[0];
        const previousSessionId = currentSessionId;
        const previousPlan = currentPlan;
        const previousRuntimeState = currentRuntimeState;
        const previousPendingYield = pendingYield;
        const previousConfirmationActionRequestId = confirmationActionRequestId;
        const previousConfirmationActionIndex = confirmationActionIndex;
        const previousShellRunStatus = shellRunStatus;
        const previousSessionBinding = currentSessionBinding;
        const previousReplayState = client.captureReplayState();
        try {
          addSystemEvent("system_message", `Connecting to session ${shortId(targetSessionId)}...`);
          sessionIdRef.current = targetSessionId;
          setCurrentSessionId(targetSessionId);
          setCurrentSessionBinding(null);
          setCurrentPlan(null);
          setCurrentRuntimeState(createCurrentRuntimeState());
          setPendingYield(null);
          setConfirmationActionRequestId(null);
          setConfirmationActionIndex(0);
          await client.connectToSession(targetSessionId);
          setShellRunStatus("ready");

          try {
            const session = await client.getSession(targetSessionId);
            updateCurrentSessionBinding(session);
            setCurrentPlan((previous) =>
              mergeHydratedCurrentPlanState(previous, session.latest_plan_snapshot),
            );
            addSystemEvent(
              "system_message",
              `Connected (${summarizeSessionBinding({
                profileId: session.profile_id,
                provider: session.provider,
                resolvedModel: session.resolved_model,
              })}).`,
            );
          } catch (error) {
            addSystemEvent(
              "system_warning",
              `Connected, but failed to hydrate session snapshot: ${(error as Error).message}`,
            );
          }
        } catch (error) {
          if (previousSessionId) {
            try {
              sessionIdRef.current = previousSessionId;
              setCurrentSessionId(previousSessionId);
              setCurrentSessionBinding(previousSessionBinding ?? null);
              setCurrentPlan(previousPlan ?? null);
              setCurrentRuntimeState(previousRuntimeState);
              setPendingYield(previousPendingYield ?? null);
              setConfirmationActionRequestId(previousConfirmationActionRequestId);
              setConfirmationActionIndex(previousConfirmationActionIndex);
              await client.connectToSession(previousSessionId, {
                replayState: previousReplayState,
              });
              setShellRunStatus(previousShellRunStatus);
              addSystemEvent(
                "system_warning",
                `Failed to connect to ${shortId(targetSessionId)}; restored previous session ${shortId(previousSessionId)}.`,
              );
            } catch (restoreError) {
              client.disconnect();
              sessionIdRef.current = "";
              setCurrentSessionId(null);
              setCurrentSessionBinding(null);
              setCurrentPlan(null);
              setCurrentRuntimeState(createCurrentRuntimeState());
              setPendingYield(null);
              setConfirmationActionRequestId(null);
              setConfirmationActionIndex(0);
              setShellRunStatus("error");
              addSystemEvent(
                "system_error",
                `Failed to connect: ${(error as Error).message}. Previous session restore also failed: ${(restoreError as Error).message}`,
              );
            }
          } else {
            client.disconnect();
            sessionIdRef.current = "";
            setCurrentSessionId(null);
            setCurrentSessionBinding(null);
            setCurrentPlan(null);
            setCurrentRuntimeState(createCurrentRuntimeState());
            setPendingYield(null);
            setConfirmationActionRequestId(null);
            setConfirmationActionIndex(0);
            setShellRunStatus("error");
            addSystemEvent("system_error", `Failed to connect: ${(error as Error).message}`);
          }
        }
        break;

      case "sessions":
        try {
          const sessions = await client.listSessions();
          addSystemEvent("system_message", `Active sessions: ${sessions.length}`);
          sessions.forEach((s) => {
            addSystemEvent(
              "system_message",
              `  ${shortId(s.session_id)} | ${s.active ? "active" : "inactive"} | ${s.agent_name ? `agent=${s.agent_name} | ` : ""}${s.profile_id ? `profile=${s.profile_id} | provider=${s.provider ?? "unknown"} | model=${s.resolved_model} | ` : ""}${s.governance.profile} | streaming=${s.streaming_mode} | recovery=${s.partial_stream_recovery_mode} | workspace=${s.workspace_id}`,
            );
          });
        } catch (error) {
          addSystemEvent("system_error", `Failed to list sessions: ${(error as Error).message}`);
        }
        break;

      case "agents": {
        if (!currentSessionId) {
          addSystemEvent("system_warning", "No active session.");
          return;
        }

        try {
          const runs = await client.listChildRuns(currentSessionId);
          setActiveChildRunCount(countActiveChildRuns(runs));
          addSystemEvent("system_message", `Child runs: ${runs.length}`);
          runs.forEach((run) => {
            addSystemEvent("system_message", `  ${formatChildRun(run)}`);
          });
        } catch (error) {
          addSystemEvent("system_error", `Failed to list child runs: ${(error as Error).message}`);
        }
        break;
      }

      case "agent": {
        if (!currentSessionId) {
          addSystemEvent("system_warning", "No active session.");
          return;
        }

        const agentCommand = parseAgentCommand(args);
        if (agentCommand.kind === "invalid") {
          addSystemEvent("system_warning", agentCommand.usage);
          return;
        }

        if (agentCommand.kind === "terminate") {
          try {
            const run = await client.terminateChildRun(currentSessionId, agentCommand.childRunId, {
              mode: agentCommand.mode,
              reason: agentCommand.reason,
              actor: "tui_operator",
            });
            addSystemEvent(
              "system_message",
              `Child run ${shortId(run.id)} termination requested: ${run.status}`,
            );
            const runs = await client.listChildRuns(currentSessionId);
            setActiveChildRunCount(countActiveChildRuns(runs));
          } catch (error) {
            addSystemEvent(
              "system_error",
              `Failed to terminate child run: ${(error as Error).message}`,
            );
          }
          break;
        }

        try {
          const run = await client.getChildRun(currentSessionId, agentCommand.childRunId);
          addSystemEvent("system_message", formatChildRun(run));
          if (run.workspace_root) {
            addSystemEvent("system_message", `  workspace=${run.workspace_root}`);
          }
          if (run.rollout_path) {
            addSystemEvent("system_message", `  rollout=${run.rollout_path}`);
          }
          if (run.latest_event_kind) {
            addSystemEvent("system_message", `  latest_event=${run.latest_event_kind}`);
          }
          if (run.error_message) {
            addSystemEvent("system_warning", `  error=${run.error_message}`);
          }
        } catch (error) {
          addSystemEvent("system_error", `Failed to read child run: ${(error as Error).message}`);
        }
        break;
      }

      case "status":
        if (AUTO_MANAGE) {
          const latestStatus = client.getDaemonStatus() || daemonStatus;
          if (latestStatus) {
            addSystemEvent(
              "system_message",
              `Daemon: ${latestStatus.state}${latestStatus.pid ? ` (pid: ${latestStatus.pid})` : ""}`,
            );
          } else {
            const running = await client.isDaemonRunning();
            addSystemEvent("system_message", `Daemon: ${running ? "running" : "stopped"}`);
          }
        } else {
          const running = await client.isDaemonRunning();
          addSystemEvent("system_message", `Remote agent: ${running ? "online" : "offline"}`);
        }
        break;

      case "input": {
        if (!currentSessionId) {
          addSystemEvent("system_warning", "No active session.");
          return;
        }

        const message = args.join(" ").trim();
        if (!message) {
          addSystemEvent("system_warning", "Usage: /input <text>");
          return;
        }

        try {
          setShellRunStatus("running");
          await client.sendInput(currentSessionId, message);
          addSystemEvent("system_message", "Input appended to active turn.");
        } catch (error) {
          addSystemEvent("system_error", `Failed to append input: ${(error as Error).message}`);
        }
        break;
      }

      case "interrupt": {
        if (!currentSessionId) {
          addSystemEvent("system_warning", "No active session.");
          return;
        }

        try {
          await client.interrupt(currentSessionId);
          addSystemEvent("system_message", "Interrupt requested.");
          setPendingYield(null);
          setConfirmationActionRequestId(null);
          setConfirmationActionIndex(0);
          setShellRunStatus("ready");
        } catch (error) {
          addSystemEvent("system_error", `Failed to interrupt: ${(error as Error).message}`);
        }
        break;
      }

      case "compact": {
        if (!currentSessionId) {
          addSystemEvent("system_warning", "No active session.");
          return;
        }

        try {
          await client.compact(currentSessionId);
          addSystemEvent("system_message", "Compaction requested.");
        } catch (error) {
          addSystemEvent("system_error", `Failed to compact: ${(error as Error).message}`);
        }
        break;
      }

      case "rollback": {
        if (!currentSessionId) {
          addSystemEvent("system_warning", "No active session.");
          return;
        }

        const turnsRaw = args[0];
        const turns = Number(turnsRaw);
        if (!turnsRaw || Number.isNaN(turns) || turns < 1 || !Number.isInteger(turns)) {
          addSystemEvent("system_warning", "Usage: /rollback <positive-integer>");
          return;
        }

        try {
          await client.rollback(currentSessionId, turns);
          addSystemEvent("system_message", `Rollback requested for ${turns} turn(s).`);
        } catch (error) {
          addSystemEvent("system_error", `Failed to rollback: ${(error as Error).message}`);
        }
        break;
      }

      case "approve":
        if (!pendingYield || pendingYield.kind !== "confirmation") {
          addSystemEvent("system_warning", "No pending confirmation.");
          return;
        }
        await submitPendingYield({ choice: "approve" });
        break;

      case "reject":
        if (!pendingYield || pendingYield.kind !== "confirmation") {
          addSystemEvent("system_warning", "No pending confirmation.");
          return;
        }
        await submitPendingYield({ choice: "reject" });
        break;

      case "modify": {
        if (!pendingYield || pendingYield.kind !== "confirmation") {
          addSystemEvent("system_warning", "No pending confirmation.");
          return;
        }
        const modifications = args.join(" ").trim();
        if (!modifications) {
          addSystemEvent("system_warning", "Usage: /modify <text>");
          return;
        }
        await submitPendingYield({ choice: "modify", modifications });
        break;
      }

      case "answer": {
        if (!pendingYield || pendingYield.kind !== "structured_input") {
          addSystemEvent("system_warning", "No pending structured input request.");
          return;
        }

        const value = args.join(" ").trim();
        if (!value) {
          addSystemEvent("system_warning", "Usage: /answer <value>");
          return;
        }

        const questions = structuredQuestions(pendingYield.payload);
        const targetQuestion = questions.length === 1 ? questions[0] : activeStructuredQuestion;

        if (!targetQuestion) {
          addSystemEvent("system_warning", "No active structured question.");
          return;
        }

        if (questions.length === 1) {
          const singleAnswerValue = usesMultiSelectKind(targetQuestion.kind)
            ? value
                .split(",")
                .map((item) => item.trim())
                .filter(Boolean)
            : value;
          const nextState = {
            ...createStructuredFormState(pendingYield.requestId, questions),
            answers: {
              [targetQuestion.id]: singleAnswerValue,
            },
          };
          const error = questionValidationError(nextState, targetQuestion);
          if (error) {
            addSystemEvent("system_warning", `${targetQuestion.label}: ${error}`);
            return;
          }

          await submitPendingYield(buildStructuredResumePayload(nextState, questions));
          break;
        }

        if (!structuredFormState) {
          addSystemEvent("system_warning", "Structured form is not ready yet.");
          return;
        }

        let nextFormState = structuredFormState;
        if (usesMultiSelectKind(targetQuestion.kind)) {
          const selectedValues = value
            .split(",")
            .map((item) => item.trim())
            .filter(Boolean);
          nextFormState = {
            ...structuredFormState,
            answers: {
              ...structuredFormState.answers,
              [targetQuestion.id]: selectedValues,
            },
          };
        } else if (usesSingleSelectKind(targetQuestion.kind)) {
          const optionIndex =
            targetQuestion.options?.findIndex((option) => option.value === value) ?? -1;
          if (optionIndex < 0) {
            addSystemEvent(
              "system_warning",
              `Unknown option for ${targetQuestion.label}. Use one of: ${(targetQuestion.options ?? []).map((option) => option.value).join(", ")}`,
            );
            return;
          }
          nextFormState = selectStructuredSingleOption(
            structuredFormState,
            targetQuestion,
            optionIndex,
          );
        } else {
          nextFormState = {
            ...structuredFormState,
            answers: {
              ...structuredFormState.answers,
              [targetQuestion.id]: value,
            },
          };
        }
        setStructuredFormState(nextFormState);
        await confirmActiveStructuredQuestion(nextFormState);
        break;
      }

      case "answers": {
        if (!pendingYield || pendingYield.kind !== "structured_input") {
          addSystemEvent("system_warning", "No pending structured input request.");
          return;
        }

        const payload = args.join(" ").trim();
        if (!payload) {
          addSystemEvent("system_warning", "Usage: /answers <json-array>");
          return;
        }

        try {
          const parsed = JSON.parse(payload);
          const content = Array.isArray(parsed) ? { answers: parsed } : parsed;
          await submitPendingYield(content);
        } catch {
          addSystemEvent("system_warning", "Invalid JSON payload for /answers.");
        }
        break;
      }

      case "resume": {
        if (!pendingYield) {
          addSystemEvent("system_warning", "No pending yield.");
          return;
        }

        const payload = args.join(" ").trim();
        if (!payload) {
          addSystemEvent("system_warning", "Usage: /resume <json-object>");
          return;
        }

        try {
          await submitPendingYield(JSON.parse(payload));
        } catch {
          addSystemEvent("system_warning", "Invalid JSON payload for /resume.");
        }
        break;
      }

      case "clear":
        setEvents([]);
        addSystemEvent("system_message", "Timeline cleared.");
        break;

      case "help":
        addSystemEvent("system_message", "Available Commands:");
        addSystemEvent(
          "system_message",
          "  /new [agent=<name>] [profile=<connection-profile>] [autonomous|conservative] [auto|on|off] [continue_once|recovery=off] - Create a new session",
        );
        addSystemEvent(
          "system_message",
          "  /connect <id>                  - Connect to an existing session",
        );
        addSystemEvent("system_message", "  /sessions                      - List active sessions");
        addSystemEvent(
          "system_message",
          "  /agents                        - List child runs for current session",
        );
        addSystemEvent("system_message", "  /agent <id>                    - Inspect a child run");
        addSystemEvent(
          "system_message",
          "  /agent terminate <id> [reason] - Gracefully terminate a child run",
        );
        addSystemEvent(
          "system_message",
          "  /agent kill <id> [reason]      - Forcefully terminate a child run",
        );
        addSystemEvent("system_message", "  /status                        - Show daemon status");
        addSystemEvent(
          "system_message",
          "  /connection list               - List configured connection profiles",
        );
        addSystemEvent(
          "system_message",
          "  /connection current            - Show pin/default/effective profile state",
        );
        addSystemEvent(
          "system_message",
          "  /connection login <profile>    - Start managed login (mode: browser|device)",
        );
        addSystemEvent(
          "system_message",
          "  /connection default set <id>   - Set the default profile for new sessions",
        );
        addSystemEvent(
          "system_message",
          "  /connection pin <id>           - Pin a profile at global or workspace scope",
        );
        addSystemEvent(
          "system_message",
          "  /input <text>                  - Append input to current turn",
        );
        addSystemEvent(
          "system_message",
          "  /interrupt                     - Interrupt current execution",
        );
        addSystemEvent(
          "system_message",
          "  /compact                       - Trigger context compaction",
        );
        addSystemEvent("system_message", "  /rollback <n>                  - Roll back N turns");
        addSystemEvent(
          "system_message",
          "  /approve | /reject | /modify   - Resolve confirmation yield",
        );
        addSystemEvent(
          "system_message",
          "  /answer | /answers             - Manual structured input fallback",
        );
        addSystemEvent(
          "system_message",
          "  /resume <json>                 - Resolve custom/dynamic yield",
        );
        addSystemEvent("system_message", "  /clear                         - Clear timeline");
        addSystemEvent("system_message", "  /help                          - Show this help");
        addSystemEvent("system_message", "  /exit                          - Exit (or Ctrl+C)");
        addSystemEvent(
          "system_message",
          "Keyboard: Ctrl+N/Ctrl+P next/prev question, Ctrl+S submit structured input, Ctrl+L clear, Ctrl+C exit",
        );
        break;

      case "exit":
      case "quit":
        app.exit();
        break;

      default:
        addSystemEvent(
          "system_warning",
          `Unknown command: /${cmd}. Type /help for available commands.`,
        );
    }
  };

  if (needsSetup) {
    return (
      <InitWizard
        onComplete={handleSetupComplete}
        agentConfigPath={CONFIG_PATH}
        hostConfigPath={HOST_CONFIG_PATH}
        installChannel={INSTALL_CHANNEL}
      />
    );
  }

  const getStatusColor = () => {
    switch (status) {
      case "connected":
        return "green";
      case "connecting":
        return "yellow";
      case "error":
        return "red";
      default:
        return "gray";
    }
  };

  const getStatusGlyph = () => {
    switch (status) {
      case "connected":
        return "●";
      case "connecting":
        return "◐";
      case "error":
        return "○";
      default:
        return "○";
    }
  };

  const pendingLabel = pendingYield
    ? `${pendingYield.kind}:${shortId(pendingYield.requestId)}`
    : "none";
  const runtimeSummary = buildCurrentRuntimeSummary({
    state: currentRuntimeState,
    shellRunStatus,
    pendingYield,
  });

  const footerHint = pendingYield
    ? activeSurface && adaptiveSurfaceContext
      ? activeSurface.footerHint(adaptiveSurfaceContext)
      : "Resolve: /resume <json>"
    : "Enter to send | /help commands | terminal scrollback | Ctrl+C exit";

  return (
    <Box flexDirection="column" width="100%">
      <Box paddingX={1} flexWrap="wrap">
        <Text bold>alan TUI</Text>
        <Text color="gray"> protocol-first terminal workspace assistant | </Text>
        <Text color="gray">
          mode={STARTUP_INFO.mode === "embedded" ? "local" : "remote"}
          {" | "}session={shortId(currentSessionId)}
          {currentSessionId ? "..." : ""}
          {" | "}
          {summarizeSessionBinding(currentSessionBinding)}
          {" | "}pending={pendingLabel}
          {" | "}events={events.length}
        </Text>
      </Box>
      <Box paddingX={1}>
        <Text color={getStatusColor()}>
          {getStatusGlyph()} {statusMessage}
        </Text>
      </Box>

      {activeSurface && adaptiveSurfaceContext
        ? activeSurface.render(adaptiveSurfaceContext)
        : null}

      <Box flexDirection="column" paddingX={1}>
        <MessageList events={events} />
      </Box>

      <SummaryHud
        plan={currentPlan}
        runtimeSummary={runtimeSummary}
        activeChildRunCount={activeChildRunCount}
        footerHint={footerHint}
        pending={Boolean(pendingYield)}
      />

      <Box paddingX={1}>
        <Text color={pendingYield ? "yellow" : "cyan"} bold>
          {pendingYield && activeSurface && adaptiveSurfaceContext
            ? activeSurface.inputLabel({
                ...adaptiveSurfaceContext,
                inputValue,
              })
            : "Input"}
        </Text>
        <Text> </Text>
        <Text color={pendingYield ? "yellow" : "white"}>{"> "}</Text>
        <TextInput
          value={inputValue}
          onChange={handleInputChange}
          onSubmit={handleSubmit}
          focus={
            pendingYield && activeSurface && adaptiveSurfaceContext
              ? (activeSurface.inputFocus?.({
                  ...adaptiveSurfaceContext,
                  inputValue,
                }) ?? true)
              : true
          }
          placeholder={
            pendingYield && activeSurface && adaptiveSurfaceContext
              ? activeSurface.inputPlaceholder({
                  ...adaptiveSurfaceContext,
                  inputValue,
                })
              : pendingYield
                ? "Resolve pending yield with command..."
                : "Type message or /help"
          }
        />
      </Box>
    </Box>
  );
}

async function runStandaloneWebSocketSmoke(): Promise<void> {
  const client = new AlanClient({
    url: STARTUP_INFO.url,
    autoManageDaemon: false,
    verbose: VERBOSE,
  });
  let sessionId: string | null = null;

  try {
    await client.connect();
    const createRequest: CreateSessionRequest = {
      workspace_dir: process.env.ALAN_TUI_SMOKE_WORKSPACE ?? process.cwd(),
      agent_name: DEFAULT_AGENT_NAME ?? undefined,
      profile_id: process.env.ALAN_TUI_SMOKE_PROFILE || undefined,
      streaming_mode: "on",
      partial_stream_recovery_mode: "continue_once",
    };
    const session = await client.createSession(createRequest);
    sessionId = session.session_id;
    await client.connectToSession(sessionId);
    console.log(`standalone websocket smoke ok: ${sessionId}`);
  } finally {
    client.disconnect();
    if (sessionId) {
      try {
        await client.deleteSession(sessionId);
      } catch (error) {
        console.error(`Failed to clean up smoke session ${sessionId}: ${(error as Error).message}`);
      }
    }
    await client.shutdown();
  }
}

if (process.env.ALAN_TUI_SMOKE_WEBSOCKET === "1") {
  await runStandaloneWebSocketSmoke();
} else {
  render(<App />);
}
