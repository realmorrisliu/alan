/**
 * alan Client - WebSocket and HTTP client for alan agent daemon
 *
 * Supports two modes:
 * 1. Auto mode (default): TUI starts and manages the daemon process
 * 2. Remote mode: connect to an already-running daemon instance
 */

import { WebSocket } from "ws";
import { DaemonManager, getDaemon } from "./daemon.js";
import { apiPaths } from "./generated/api-contract";
import { installChannelDescriptor, resolveInstallChannel } from "./install-channel.js";
import type {
  ClientCapabilities,
  ChildRunListResponse,
  ChildRunRecord,
  ChildRunResponse,
  ConnectionCatalogResponse,
  ConnectionCurrentState,
  ConnectionCredentialStatus,
  ConnectionListResponse,
  ConnectionLoginSuccessResponse,
  ConnectionLogoutResponse,
  ConnectionProfileSummary,
  ConnectionTestResponse,
  ContentPart,
  CreateSessionRequest,
  CreateSessionResponse,
  ClientEvents,
  CreateConnectionRequest,
  ClearConnectionDefaultRequest,
  EventEnvelope,
  Op,
  PinConnectionRequest,
  SessionListResponse,
  SessionListItem,
  SessionReadResponse,
  TerminateChildRunRequest,
  SetConnectionDefaultRequest,
  StartConnectionBrowserLoginRequest,
  StartConnectionBrowserLoginResponse,
  StartConnectionDeviceLoginResponse,
  UnpinConnectionRequest,
  UpdateConnectionRequest,
} from "./types";

type EventHandler<T> = (data: T) => void;

interface ReadEventsResponse {
  gap: boolean;
  oldest_event_id?: string | null;
  latest_event_id?: string | null;
  events: EventEnvelope[];
}

interface ErrorResponse {
  error?: string;
  message?: string;
}

type ReplayUnavailableState = "active" | "archived" | "missing" | "unknown";

export interface ReplayStateSnapshot {
  sessionId: string;
  lastEventId: string | null;
  seenEventIds: string[];
}

export interface ConnectToSessionOptions {
  replayState?: ReplayStateSnapshot | null;
}

function toResumeContent(contentInput: unknown): ContentPart[] {
  if (contentInput === null || contentInput === undefined) {
    return [];
  }

  if (typeof contentInput === "string") {
    return [{ type: "text", text: contentInput }];
  }

  return [{ type: "structured", data: contentInput }];
}

export interface AlanClientOptions {
  /**
   * Daemon URL. Defaults to auto-managing a local daemon.
   * Can also be set to a remote URL such as ws://remote-server:8090.
   */
  url?: string;
  /** Auto-manage the daemon process (only valid for local daemon URLs). */
  autoManageDaemon?: boolean;
  /** Enable verbose logs. */
  verbose?: boolean;
}

export class AlanClient {
  private ws: WebSocket | null = null;
  private baseUrl: string;
  private wsUrl: string;
  private eventHandlers: Map<string, EventHandler<unknown>[]> = new Map();
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 5;
  private reconnectDelay = 1000;
  private options: Required<AlanClientOptions>;
  private daemon: DaemonManager | null = null;
  private isRemote = false;
  private currentSessionId: string | null = null;
  private reconnectTimer: NodeJS.Timeout | null = null;
  private reconnectEnabled = true;
  private connectionVersion = 0;
  private lastEventId: string | null = null;
  private seenEventIds: string[] = [];
  private seenEventSet: Set<string> = new Set();
  private maxSeenEventIds = 4096;
  private maxReplayEvents = 20000;

  constructor(options: AlanClientOptions = {}) {
    this.options = {
      url: options.url ?? installChannelDescriptor(resolveInstallChannel()).daemonWsUrl,
      autoManageDaemon: options.autoManageDaemon ?? true,
      verbose: options.verbose ?? false,
    };

    // Detect whether this is a remote connection (not localhost/127.0.0.1).
    this.isRemote =
      !this.options.url.includes("127.0.0.1") && !this.options.url.includes("localhost");

    // Convert HTTP URL to WebSocket URL
    this.baseUrl = this.options.url.replace(/^ws/, "http").replace(/\/$/, "");
    this.wsUrl = this.options.url.replace(/^http/, "ws").replace(/\/$/, "");
  }

  /**
   * Ensure the daemon is running (local mode).
   */
  async ensureDaemon(): Promise<void> {
    if (this.isRemote || !this.options.autoManageDaemon) {
      return;
    }

    const daemonUrl = new URL(this.baseUrl);
    this.daemon = getDaemon({
      host: daemonUrl.hostname,
      port: Number(daemonUrl.port || "80"),
      verbose: this.options.verbose,
    });
    const status = await this.daemon.start();

    if (this.options.verbose) {
      console.log(`[alan] daemon ${status.state}${status.pid ? ` (pid: ${status.pid})` : ""}`);
    }
  }

  // Event emitter implementation
  public on<K extends keyof ClientEvents>(event: K, handler: ClientEvents[K]): void {
    if (!this.eventHandlers.has(event)) {
      this.eventHandlers.set(event, []);
    }
    this.eventHandlers.get(event)!.push(handler as EventHandler<unknown>);
  }

  public off<K extends keyof ClientEvents>(event: K, handler: ClientEvents[K]): void {
    const handlers = this.eventHandlers.get(event);
    if (handlers) {
      const index = handlers.indexOf(handler as EventHandler<unknown>);
      if (index > -1) {
        handlers.splice(index, 1);
      }
    }
  }

  private emit<K extends keyof ClientEvents>(event: K, ...args: Parameters<ClientEvents[K]>): void {
    const handlers = this.eventHandlers.get(event);
    if (handlers) {
      handlers.forEach((handler) => (handler as (...args: unknown[]) => void)(...args));
    }
  }

  private resetReplayState(): void {
    this.lastEventId = null;
    this.seenEventIds = [];
    this.seenEventSet.clear();
  }

  private restoreReplayState(snapshot: ReplayStateSnapshot): void {
    const seenEventIds = snapshot.seenEventIds.slice(-this.maxSeenEventIds);
    this.lastEventId = snapshot.lastEventId;
    this.seenEventIds = [...seenEventIds];
    this.seenEventSet = new Set(seenEventIds);
    if (this.lastEventId && !this.seenEventSet.has(this.lastEventId)) {
      this.rememberEventId(this.lastEventId);
    }
  }

  public captureReplayState(): ReplayStateSnapshot | null {
    if (!this.currentSessionId) {
      return null;
    }

    return {
      sessionId: this.currentSessionId,
      lastEventId: this.lastEventId,
      seenEventIds: [...this.seenEventIds],
    };
  }

  private rememberEventId(eventId: string): void {
    if (this.seenEventSet.has(eventId)) return;
    this.seenEventSet.add(eventId);
    this.seenEventIds.push(eventId);
    if (this.seenEventIds.length > this.maxSeenEventIds) {
      const removed = this.seenEventIds.shift();
      if (removed) {
        this.seenEventSet.delete(removed);
      }
    }
  }

  private emitEnvelope(envelope: EventEnvelope): void {
    const eventId = envelope.event_id;
    if (eventId && this.seenEventSet.has(eventId)) {
      return;
    }
    if (eventId) {
      this.rememberEventId(eventId);
      this.lastEventId = eventId;
    }
    this.emit("event", envelope);
  }

  private async replayMissedEvents(sessionId: string, version: number): Promise<void> {
    if (!this.lastEventId) {
      return;
    }

    let afterEventId: string | null = this.lastEventId;
    let replayedEvents = 0;
    const pageLimit = 200;
    while (afterEventId) {
      if (version !== this.connectionVersion) {
        return;
      }

      const params = new URLSearchParams({
        limit: String(pageLimit),
        after_event_id: afterEventId,
      });
      const response = await fetch(
        `${this.baseUrl}${apiPaths.sessionEventsRead(sessionId)}?${params.toString()}`,
      );
      if (!response.ok) {
        throw await this.buildReplayReadError(sessionId, response);
      }
      const page = (await response.json()) as ReadEventsResponse;
      if (!Array.isArray(page.events)) {
        throw new Error("Failed to replay missed events: malformed read-events page");
      }

      if (page.gap) {
        const oldest = page.oldest_event_id ?? "unknown";
        const latest = page.latest_event_id ?? "unknown";
        this.emit(
          "error",
          new Error(
            `Event replay gap detected (oldest=${oldest}, latest=${latest}). Some recent events may have been evicted from server buffer.`,
          ),
        );
        if (page.events.length === 0) {
          throw new Error("Event replay gap detected but no replayable events were returned");
        }
      }

      if (page.events.length === 0) {
        return;
      }

      for (const envelope of page.events) {
        this.emitEnvelope(envelope);
      }
      replayedEvents += page.events.length;
      if (replayedEvents > this.maxReplayEvents) {
        throw new Error(
          `Replay exceeded safety limit (${this.maxReplayEvents} events); stopping to avoid unbounded catch-up loop.`,
        );
      }

      const pageLastEventId = page.events[page.events.length - 1]?.event_id ?? null;
      if (!pageLastEventId) {
        throw new Error(
          "Failed to replay missed events: replay page contains event without event_id",
        );
      }
      if (
        pageLastEventId === afterEventId ||
        page.events.length < pageLimit ||
        (page.latest_event_id !== undefined &&
          page.latest_event_id !== null &&
          pageLastEventId === page.latest_event_id)
      ) {
        return;
      }
      afterEventId = pageLastEventId;
    }
  }

  private async buildReplayReadError(sessionId: string, response: Response): Promise<Error> {
    const errorMsg = await this.readErrorMessage(response);
    if (response.status !== 404) {
      return new Error(`Failed to replay missed events: ${errorMsg}`);
    }

    const availability = await this.probeReplayUnavailableState(sessionId);
    switch (availability) {
      case "active":
        return new Error(
          `Session ${sessionId} is still registered, but its replay stream is unavailable. Retry reconnect or reopen the session.`,
        );
      case "archived":
        return new Error(
          `Session ${sessionId} is archived and no longer has a live replay stream. Open its saved transcript or fork a new session.`,
        );
      case "missing":
        return new Error(
          `Session ${sessionId} is no longer active on the daemon. It may have expired, been deleted, or been lost after daemon restart.`,
        );
      default:
        return new Error(
          `Failed to replay missed events: ${errorMsg}. Session availability could not be confirmed.`,
        );
    }
  }

  private async probeReplayUnavailableState(sessionId: string): Promise<ReplayUnavailableState> {
    try {
      const session = await this.getSession(sessionId);
      return session.active === false ? "archived" : "active";
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (message.includes("404") || message.toLowerCase().includes("not found")) {
        return "missing";
      }
      return "unknown";
    }
  }

  private async readErrorMessage(response: Response): Promise<string> {
    let errorMsg = response.statusText || `HTTP ${response.status}`;
    try {
      const errData = (await response.json()) as ErrorResponse;
      if (typeof errData.message === "string" && errData.message.trim()) {
        errorMsg = errData.message;
      } else if (typeof errData.error === "string" && errData.error.trim()) {
        errorMsg = errData.error;
      }
    } catch {
      // ignore JSON parse errors
    }
    return errorMsg;
  }

  // HTTP API methods
  public async createSession(request?: CreateSessionRequest): Promise<CreateSessionResponse> {
    await this.ensureDaemon();

    const response = await fetch(`${this.baseUrl}${apiPaths.sessions()}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(request || {}),
    });

    if (!response.ok) {
      let errorMsg = response.statusText;
      try {
        const errData = await response.json();
        if (errData && (errData as any).error) {
          errorMsg = (errData as any).error;
        }
      } catch {
        // ignore JSON parse error
      }
      throw new Error(`Failed to create session: ${errorMsg}`);
    }

    const data = (await response.json()) as CreateSessionResponse;
    this.emit("session_created", data.session_id);
    return data;
  }

  public async listSessions(): Promise<SessionListItem[]> {
    await this.ensureDaemon();

    const response = await fetch(`${this.baseUrl}${apiPaths.sessions()}`);

    if (!response.ok) {
      throw new Error(`Failed to list sessions: ${response.statusText}`);
    }

    const data = (await response.json()) as SessionListResponse;
    return data.sessions;
  }

  public async deleteSession(sessionId: string): Promise<void> {
    await this.ensureDaemon();

    const response = await fetch(`${this.baseUrl}${apiPaths.session(sessionId)}`, {
      method: "DELETE",
    });

    if (!response.ok) {
      throw new Error(
        `Failed to delete session ${sessionId}: ${await this.readErrorMessage(response)}`,
      );
    }
  }

  public async getSession(sessionId: string): Promise<SessionReadResponse> {
    await this.ensureDaemon();

    const response = await fetch(`${this.baseUrl}${apiPaths.sessionRead(sessionId)}`);

    if (!response.ok) {
      throw new Error(`Failed to get session: ${await this.readErrorMessage(response)}`);
    }

    return (await response.json()) as SessionReadResponse;
  }

  public async listChildRuns(sessionId: string): Promise<ChildRunRecord[]> {
    await this.ensureDaemon();

    const response = await fetch(`${this.baseUrl}${apiPaths.sessionChildRuns(sessionId)}`);

    if (!response.ok) {
      throw new Error(`Failed to list child runs: ${await this.readErrorMessage(response)}`);
    }

    const data = (await response.json()) as ChildRunListResponse;
    return data.child_runs;
  }

  public async getChildRun(sessionId: string, childRunId: string): Promise<ChildRunRecord> {
    await this.ensureDaemon();

    const response = await fetch(
      `${this.baseUrl}${apiPaths.sessionChildRun(sessionId, childRunId)}`,
    );

    if (!response.ok) {
      throw new Error(`Failed to get child run: ${await this.readErrorMessage(response)}`);
    }

    const data = (await response.json()) as ChildRunResponse;
    return data.child_run;
  }

  public async terminateChildRun(
    sessionId: string,
    childRunId: string,
    request: TerminateChildRunRequest,
  ): Promise<ChildRunRecord> {
    await this.ensureDaemon();

    const response = await fetch(
      `${this.baseUrl}${apiPaths.sessionChildRunTerminate(sessionId, childRunId)}`,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(request),
      },
    );

    if (!response.ok) {
      throw new Error(`Failed to terminate child run: ${await this.readErrorMessage(response)}`);
    }

    const data = (await response.json()) as ChildRunResponse;
    return data.child_run;
  }

  public async submitOperation(sessionId: string, op: Op): Promise<void> {
    await this.ensureDaemon();

    const response = await fetch(`${this.baseUrl}${apiPaths.sessionSubmit(sessionId)}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ op }),
    });

    if (!response.ok) {
      throw new Error(`Failed to submit operation: ${response.statusText}`);
    }
  }

  public async setClientCapabilities(
    sessionId: string,
    capabilities: ClientCapabilities,
  ): Promise<void> {
    await this.submitOperation(sessionId, {
      type: "set_client_capabilities",
      capabilities,
    });
  }

  public async getConnectionCatalog(): Promise<ConnectionCatalogResponse> {
    await this.ensureDaemon();

    const response = await fetch(`${this.baseUrl}${apiPaths.connectionsCatalog()}`);
    if (!response.ok) {
      throw new Error(
        `Failed to read connection catalog: ${await this.readErrorMessage(response)}`,
      );
    }

    return (await response.json()) as ConnectionCatalogResponse;
  }

  public async listConnections(): Promise<ConnectionListResponse> {
    await this.ensureDaemon();

    const response = await fetch(`${this.baseUrl}${apiPaths.connections()}`);
    if (!response.ok) {
      throw new Error(`Failed to list connections: ${await this.readErrorMessage(response)}`);
    }

    return (await response.json()) as ConnectionListResponse;
  }

  public async getConnectionCurrent(workspaceDir?: string): Promise<ConnectionCurrentState> {
    await this.ensureDaemon();

    const url = new URL(`${this.baseUrl}${apiPaths.connectionsCurrent()}`);
    if (workspaceDir) {
      url.searchParams.set("workspace_dir", workspaceDir);
    }

    const response = await fetch(url);
    if (!response.ok) {
      throw new Error(
        `Failed to read connection selection state: ${await this.readErrorMessage(response)}`,
      );
    }

    return (await response.json()) as ConnectionCurrentState;
  }

  public async getConnection(profileId: string): Promise<ConnectionProfileSummary> {
    await this.ensureDaemon();

    const response = await fetch(`${this.baseUrl}${apiPaths.connection(profileId)}`);
    if (!response.ok) {
      throw new Error(
        `Failed to read connection ${profileId}: ${await this.readErrorMessage(response)}`,
      );
    }

    return (await response.json()) as ConnectionProfileSummary;
  }

  public async createConnection(
    request: CreateConnectionRequest,
  ): Promise<ConnectionProfileSummary> {
    await this.ensureDaemon();

    const response = await fetch(`${this.baseUrl}${apiPaths.connections()}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(request),
    });
    if (!response.ok) {
      throw new Error(`Failed to create connection: ${await this.readErrorMessage(response)}`);
    }

    return (await response.json()) as ConnectionProfileSummary;
  }

  public async updateConnection(
    profileId: string,
    request: UpdateConnectionRequest,
  ): Promise<ConnectionProfileSummary> {
    await this.ensureDaemon();

    const response = await fetch(`${this.baseUrl}${apiPaths.connection(profileId)}`, {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(request),
    });
    if (!response.ok) {
      throw new Error(
        `Failed to update connection ${profileId}: ${await this.readErrorMessage(response)}`,
      );
    }

    return (await response.json()) as ConnectionProfileSummary;
  }

  public async removeConnection(profileId: string): Promise<boolean> {
    await this.ensureDaemon();

    const response = await fetch(`${this.baseUrl}${apiPaths.connection(profileId)}`, {
      method: "DELETE",
    });
    if (!response.ok) {
      throw new Error(
        `Failed to remove connection ${profileId}: ${await this.readErrorMessage(response)}`,
      );
    }

    const data = (await response.json()) as { removed: boolean };
    return data.removed;
  }

  public async activateConnection(profileId: string): Promise<ConnectionProfileSummary> {
    await this.ensureDaemon();

    const response = await fetch(`${this.baseUrl}${apiPaths.connectionActivate(profileId)}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({}),
    });
    if (!response.ok) {
      throw new Error(
        `Failed to activate connection ${profileId}: ${await this.readErrorMessage(response)}`,
      );
    }

    return (await response.json()) as ConnectionProfileSummary;
  }

  public async setConnectionDefault(
    request: SetConnectionDefaultRequest,
  ): Promise<ConnectionCurrentState> {
    await this.ensureDaemon();

    const response = await fetch(`${this.baseUrl}${apiPaths.connectionsDefaultSet()}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(request),
    });
    if (!response.ok) {
      throw new Error(`Failed to update default profile: ${await this.readErrorMessage(response)}`);
    }

    return (await response.json()) as ConnectionCurrentState;
  }

  public async clearConnectionDefault(
    request: ClearConnectionDefaultRequest = {},
  ): Promise<ConnectionCurrentState> {
    await this.ensureDaemon();

    const response = await fetch(`${this.baseUrl}${apiPaths.connectionsDefaultClear()}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(request),
    });
    if (!response.ok) {
      throw new Error(`Failed to clear default profile: ${await this.readErrorMessage(response)}`);
    }

    return (await response.json()) as ConnectionCurrentState;
  }

  public async pinConnection(request: PinConnectionRequest): Promise<ConnectionCurrentState> {
    await this.ensureDaemon();

    const response = await fetch(`${this.baseUrl}${apiPaths.connectionsPin()}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(request),
    });
    if (!response.ok) {
      throw new Error(
        `Failed to pin profile ${request.profile_id}: ${await this.readErrorMessage(response)}`,
      );
    }

    return (await response.json()) as ConnectionCurrentState;
  }

  public async unpinConnection(request: UnpinConnectionRequest): Promise<ConnectionCurrentState> {
    await this.ensureDaemon();

    const response = await fetch(`${this.baseUrl}${apiPaths.connectionsUnpin()}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(request),
    });
    if (!response.ok) {
      throw new Error(`Failed to clear connection pin: ${await this.readErrorMessage(response)}`);
    }

    return (await response.json()) as ConnectionCurrentState;
  }

  public async getConnectionCredentialStatus(
    profileId: string,
  ): Promise<ConnectionCredentialStatus> {
    await this.ensureDaemon();

    const response = await fetch(
      `${this.baseUrl}${apiPaths.connectionCredentialStatus(profileId)}`,
    );
    if (!response.ok) {
      throw new Error(
        `Failed to read credential status for ${profileId}: ${await this.readErrorMessage(response)}`,
      );
    }

    return (await response.json()) as ConnectionCredentialStatus;
  }

  public async setConnectionSecret(
    profileId: string,
    secret: string,
  ): Promise<ConnectionCredentialStatus> {
    await this.ensureDaemon();

    const response = await fetch(
      `${this.baseUrl}${apiPaths.connectionCredentialSecret(profileId)}`,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ secret }),
      },
    );
    if (!response.ok) {
      throw new Error(
        `Failed to set secret for ${profileId}: ${await this.readErrorMessage(response)}`,
      );
    }

    return (await response.json()) as ConnectionCredentialStatus;
  }

  public async startConnectionBrowserLogin(
    profileId: string,
    request?: StartConnectionBrowserLoginRequest,
  ): Promise<StartConnectionBrowserLoginResponse> {
    await this.ensureDaemon();

    const response = await fetch(
      `${this.baseUrl}${apiPaths.connectionBrowserLoginStart(profileId)}`,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(request ?? {}),
      },
    );
    if (!response.ok) {
      throw new Error(
        `Failed to start browser login for ${profileId}: ${await this.readErrorMessage(response)}`,
      );
    }

    return (await response.json()) as StartConnectionBrowserLoginResponse;
  }

  public async startConnectionDeviceLogin(
    profileId: string,
  ): Promise<StartConnectionDeviceLoginResponse> {
    await this.ensureDaemon();

    const response = await fetch(
      `${this.baseUrl}${apiPaths.connectionDeviceLoginStart(profileId)}`,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({}),
      },
    );
    if (!response.ok) {
      throw new Error(
        `Failed to start device login for ${profileId}: ${await this.readErrorMessage(response)}`,
      );
    }

    return (await response.json()) as StartConnectionDeviceLoginResponse;
  }

  public async completeConnectionDeviceLogin(
    profileId: string,
    loginId: string,
  ): Promise<ConnectionLoginSuccessResponse> {
    await this.ensureDaemon();

    const response = await fetch(
      `${this.baseUrl}${apiPaths.connectionDeviceLoginComplete(profileId)}`,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ login_id: loginId }),
      },
    );
    if (!response.ok) {
      throw new Error(
        `Failed to complete device login for ${profileId}: ${await this.readErrorMessage(response)}`,
      );
    }

    return (await response.json()) as ConnectionLoginSuccessResponse;
  }

  public async logoutConnection(profileId: string): Promise<ConnectionLogoutResponse> {
    await this.ensureDaemon();

    const response = await fetch(
      `${this.baseUrl}${apiPaths.connectionCredentialLogout(profileId)}`,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({}),
      },
    );
    if (!response.ok) {
      throw new Error(
        `Failed to logout connection ${profileId}: ${await this.readErrorMessage(response)}`,
      );
    }

    return (await response.json()) as ConnectionLogoutResponse;
  }

  public async testConnection(profileId: string): Promise<ConnectionTestResponse> {
    await this.ensureDaemon();

    const response = await fetch(`${this.baseUrl}${apiPaths.connectionTest(profileId)}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({}),
    });
    if (!response.ok) {
      throw new Error(
        `Failed to test connection ${profileId}: ${await this.readErrorMessage(response)}`,
      );
    }

    return (await response.json()) as ConnectionTestResponse;
  }

  /**
   * Connect to the daemon (HTTP health check).
   * Does not create a WebSocket connection; that happens in `connectToSession`.
   */
  public async connect(): Promise<void> {
    // In local mode, ensure the daemon is started first.
    if (!this.isRemote && this.options.autoManageDaemon) {
      await this.ensureDaemon();
    }

    // Wait for daemon readiness via health checks.
    const startTime = Date.now();
    const timeout = 10000;
    const checkInterval = 200;

    while (Date.now() - startTime < timeout) {
      if (await this.isDaemonRunning()) {
        this.reconnectAttempts = 0;
        this.emit("connected");
        return;
      }
      await new Promise((r) => setTimeout(r, checkInterval));
    }

    throw new Error("Failed to connect to daemon: health check timeout");
  }

  public async connectToSession(
    sessionId: string,
    options: ConnectToSessionOptions = {},
  ): Promise<void> {
    // In local mode, ensure the daemon is started first.
    if (!this.isRemote && this.options.autoManageDaemon) {
      await this.ensureDaemon();
    }

    // Switching session should not trigger reconnect from an old socket close.
    this.reconnectEnabled = false;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }

    if (this.ws) {
      this.ws.removeAllListeners();
      this.ws.close();
      this.ws = null;
    }

    const previousSessionId = this.currentSessionId;
    const replayState = options.replayState;
    if (replayState?.sessionId === sessionId) {
      this.restoreReplayState(replayState);
    } else if (previousSessionId !== sessionId) {
      this.resetReplayState();
    }
    this.currentSessionId = sessionId;
    const version = ++this.connectionVersion;
    const wsUrl = `${this.wsUrl}${apiPaths.sessionWebSocket(sessionId)}`;

    return new Promise((resolve, reject) => {
      let ready = false;
      let settled = false;
      let initializationFailure: Error | null = null;
      const queuedEnvelopes: EventEnvelope[] = [];
      const resolveOnce = () => {
        if (settled) return;
        settled = true;
        resolve();
      };
      const rejectOnce = (error: Error) => {
        if (settled) return;
        settled = true;
        reject(error);
      };

      try {
        this.ws = new WebSocket(wsUrl);

        this.ws.on("open", () => {
          if (version !== this.connectionVersion) return;
          void (async () => {
            try {
              await this.replayMissedEvents(sessionId, version);
            } catch (error) {
              const replayError = error as Error;
              initializationFailure = replayError;
              this.emit("error", replayError);
              rejectOnce(replayError);
              if (version === this.connectionVersion) {
                this.ws?.close();
              }
              return;
            }

            if (version !== this.connectionVersion) {
              return;
            }
            ready = true;
            this.reconnectEnabled = true;
            this.reconnectAttempts = 0;
            for (const envelope of queuedEnvelopes.splice(0)) {
              this.emitEnvelope(envelope);
            }
            this.emit("connected");
            resolveOnce();
          })();
        });

        this.ws.on("message", (data: Buffer) => {
          if (version !== this.connectionVersion) return;
          try {
            const envelope = JSON.parse(data.toString()) as EventEnvelope;
            if (ready) {
              this.emitEnvelope(envelope);
            } else {
              queuedEnvelopes.push(envelope);
            }
          } catch (error) {
            console.error("Failed to parse message:", error);
          }
        });

        this.ws.on("close", () => {
          if (version !== this.connectionVersion) return;
          if (ready) {
            this.emit("disconnected");
          }
          if (!ready) {
            rejectOnce(
              initializationFailure ??
                new Error("WebSocket closed before initialization completed"),
            );
          }
          this.attemptReconnect(version);
        });

        this.ws.on("error", (error: Error) => {
          if (version !== this.connectionVersion) return;
          this.emit("error", error);
          rejectOnce(error);
        });
      } catch (error) {
        rejectOnce(error as Error);
      }
    });
  }

  public disconnect(): void {
    this.reconnectEnabled = false;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.connectionVersion++;
    if (this.ws) {
      this.ws.removeAllListeners();
      this.ws.close();
      this.ws = null;
    }
    this.currentSessionId = null;
  }

  /**
   * Full shutdown (stop daemon in auto-managed mode).
   */
  async shutdown(): Promise<void> {
    this.disconnect();

    if (this.daemon && !this.isRemote) {
      await this.daemon.stop();
    }
  }

  private attemptReconnect(version: number): void {
    if (!this.reconnectEnabled) return;
    if (!this.currentSessionId) return; // No active session, do not reconnect.
    if (version !== this.connectionVersion) return;

    if (this.reconnectAttempts >= this.maxReconnectAttempts) {
      this.emit("error", new Error("Max reconnection attempts reached"));
      return;
    }

    this.reconnectAttempts++;
    const delay = this.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1);

    this.reconnectTimer = setTimeout(() => {
      // Reconnect using the saved session ID.
      if (this.currentSessionId && version === this.connectionVersion) {
        this.connectToSession(this.currentSessionId).catch(() => {
          // Error handled in connectToSession
        });
      }
    }, delay);
  }

  // Convenience methods
  public async sendMessage(sessionId: string, content: string): Promise<void> {
    await this.submitOperation(sessionId, {
      type: "turn",
      parts: [{ type: "text", text: content }],
    });
  }

  public async startTask(sessionId: string, input: string): Promise<void> {
    await this.submitOperation(sessionId, {
      type: "turn",
      parts: [{ type: "text", text: input }],
    });
  }

  public async sendInput(sessionId: string, content: string): Promise<void> {
    await this.submitOperation(sessionId, {
      type: "input",
      parts: [{ type: "text", text: content }],
    });
  }

  public async resume(sessionId: string, requestId: string, content: unknown): Promise<void> {
    await this.submitOperation(sessionId, {
      type: "resume",
      request_id: requestId,
      content: toResumeContent(content),
    });
  }

  public async interrupt(sessionId: string): Promise<void> {
    await this.submitOperation(sessionId, {
      type: "interrupt",
    });
  }

  public async compact(sessionId: string, focus?: string): Promise<void> {
    await this.submitOperation(sessionId, {
      type: "compact_with_options",
      ...(focus ? { focus } : {}),
    });
  }

  public async rollback(sessionId: string, turns: number): Promise<void> {
    await this.submitOperation(sessionId, {
      type: "rollback",
      turns,
    });
  }

  /**
   * Check whether the daemon is reachable.
   */
  async isDaemonRunning(): Promise<boolean> {
    try {
      const response = await fetch(`${this.baseUrl}${apiPaths.health()}`, {
        signal: AbortSignal.timeout(1000),
      });
      return response.ok;
    } catch {
      return false;
    }
  }

  /**
   * Get daemon status (if auto-managed).
   */
  getDaemonStatus() {
    return this.daemon?.getStatus();
  }
}
