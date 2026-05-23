/**
 * Daemon Manager - manages alan daemon lifecycle via `alan daemon` commands.
 *
 * This keeps the TUI aligned with the current packaging model where `alan`
 * is the only shipped binary and daemon runs as `alan daemon start`.
 */

import { spawn, spawnSync } from "node:child_process";
import { statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  installChannelDescriptor,
  resolveInstallChannel,
  type InstallChannelId,
} from "./install-channel.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

function pushLog(buffer: string[], line: string, max: number): void {
  if (!line) return;
  buffer.push(line);
  if (buffer.length > max) {
    buffer.shift();
  }
}

function candidateAlanPaths(channel: InstallChannelId): string[] {
  const descriptor = installChannelDescriptor(channel);
  const platform = process.platform;
  const exeSuffix = platform === "win32" ? ".exe" : "";
  const commandName = `${descriptor.cliName}${exeSuffix}`;
  const scriptPath = process.argv[1] ? resolve(process.argv[1]) : null;
  const scriptDir = scriptPath ? dirname(scriptPath) : null;

  const candidates = [
    // Explicit override
    process.env.ALAN_CLI_PATH,
    // Adjacent to the running TUI script (production install)
    scriptDir ? join(scriptDir, commandName) : undefined,
    // Relative to this source file
    join(__dirname, `../${commandName}`),
    join(__dirname, commandName),
    // Development builds from repo root
    join(resolve(__dirname, "../../../"), `target/release/${commandName}`),
    join(resolve(__dirname, "../../../"), `target/debug/${commandName}`),
    join(process.cwd(), `target/release/${commandName}`),
    join(process.cwd(), `target/debug/${commandName}`),
    join(resolve(__dirname, "../../../"), `target/release/alan${exeSuffix}`),
    join(resolve(__dirname, "../../../"), `target/debug/alan${exeSuffix}`),
    join(process.cwd(), `target/release/alan${exeSuffix}`),
    join(process.cwd(), `target/debug/alan${exeSuffix}`),
    // PATH fallback
    descriptor.cliName,
    "alan",
  ];

  const unique = new Set<string>();
  for (const raw of candidates) {
    if (!raw) continue;
    const candidate = raw === "alan" || raw === descriptor.cliName ? raw : resolve(raw);
    if (!unique.has(candidate)) {
      unique.add(candidate);
    }
  }

  return [...unique];
}

function isRunnableBinaryPath(path: string): boolean {
  try {
    return statSync(path).isFile();
  } catch {
    return false;
  }
}

function isCommandAvailable(command: string): boolean {
  try {
    const result = spawnSync(command, ["--version"], {
      stdio: "ignore",
      timeout: 1000,
    });
    return !result.error;
  } catch {
    return false;
  }
}

export function resolveAlanBinaryFromCandidates(
  candidates: string[],
  runnablePathCheck: (path: string) => boolean = isRunnableBinaryPath,
  commandAvailableCheck: (command: string) => boolean = isCommandAvailable,
): string | null {
  for (const candidate of candidates) {
    if (candidate === "alan") {
      if (commandAvailableCheck(candidate)) {
        return candidate;
      }
      continue;
    }

    if (runnablePathCheck(candidate)) {
      return candidate;
    }
  }

  return null;
}

async function runCommand(
  bin: string,
  args: string[],
  env: Record<string, string>,
  cwd: string,
  timeoutMs: number,
): Promise<{ code: number | null; output: string }> {
  return new Promise((resolve, reject) => {
    const child = spawn(bin, args, {
      env,
      cwd,
      stdio: ["ignore", "pipe", "pipe"],
    });

    let output = "";
    const timer = setTimeout(() => {
      child.kill("SIGKILL");
      reject(new Error(`Command timed out: ${bin} ${args.join(" ")}`));
    }, timeoutMs);

    child.on("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });

    child.stdout?.on("data", (chunk: Buffer) => {
      output += chunk.toString();
    });

    child.stderr?.on("data", (chunk: Buffer) => {
      output += chunk.toString();
    });

    child.on("close", (code) => {
      clearTimeout(timer);
      resolve({ code, output: output.trim() });
    });
  });
}

export interface DaemonConfig {
  /** Bound port. */
  port?: number;
  /** Bound host. */
  host?: string;
  /** Working directory for subprocess calls. */
  cwd?: string;
  /** Extra environment variables. */
  env?: Record<string, string>;
  /** Startup timeout in milliseconds. */
  startupTimeout?: number;
  /** Verbose logging for troubleshooting. */
  verbose?: boolean;
  /** Active install channel. */
  installChannel?: InstallChannelId;
}

export interface DaemonStatus {
  state: "stopped" | "starting" | "running" | "error";
  pid?: number;
  url: string;
  error?: string;
}

export class DaemonManager {
  private config: Required<DaemonConfig>;
  private status: DaemonStatus = { state: "stopped", url: "" };
  private startedByTui = false;
  private logBuffer: string[] = [];
  private maxLogBuffer = 100;
  private installChannel: InstallChannelId;

  constructor(config: DaemonConfig = {}) {
    this.installChannel = config.installChannel ?? resolveInstallChannel();
    const descriptor = installChannelDescriptor(this.installChannel);
    const defaultUrl = new URL(descriptor.daemonUrl);
    this.config = {
      port: config.port ?? Number(defaultUrl.port || "80"),
      host: config.host ?? defaultUrl.hostname,
      cwd: config.cwd ?? process.cwd(),
      env: config.env ?? {},
      startupTimeout: config.startupTimeout ?? 10000,
      verbose: config.verbose ?? false,
      installChannel: this.installChannel,
    };
    this.status.url = `http://${this.config.host}:${this.config.port}`;
  }

  private async findAlanBinary(): Promise<string | null> {
    return resolveAlanBinaryFromCandidates(candidateAlanPaths(this.installChannel));
  }

  async isRunning(): Promise<boolean> {
    try {
      const response = await fetch(`${this.status.url}/health`, {
        signal: AbortSignal.timeout(1000),
      });
      return response.ok;
    } catch {
      return false;
    }
  }

  private async waitForReady(timeoutMs: number): Promise<void> {
    const start = Date.now();
    while (Date.now() - start < timeoutMs) {
      if (await this.isRunning()) {
        return;
      }
      await new Promise((r) => setTimeout(r, 150));
    }
    const logs = this.logBuffer.slice(-10).join("\n");
    throw new Error(`Daemon startup timed out. Recent logs:\n${logs}`);
  }

  async start(): Promise<DaemonStatus> {
    if (this.status.state === "running" && (await this.isRunning())) {
      return this.status;
    }
    if (this.status.state === "starting") {
      throw new Error("daemon is already starting");
    }

    if (await this.isRunning()) {
      this.status = { state: "running", url: this.status.url };
      this.startedByTui = false;
      return this.status;
    }

    this.status = { state: "starting", url: this.status.url };
    this.logBuffer = [];

    const alanBin = await this.findAlanBinary();
    if (!alanBin) {
      const error = "Cannot find `alan` binary. Install with `just install` or set ALAN_CLI_PATH.";
      this.status = { state: "error", url: this.status.url, error };
      throw new Error(error);
    }

    const env: Record<string, string> = {
      ...process.env,
      ...this.config.env,
      ALAN_INSTALL_CHANNEL: this.installChannel,
      BIND_ADDRESS: `${this.config.host}:${this.config.port}`,
    };

    if (this.config.verbose) {
      pushLog(this.logBuffer, `[daemon] start via: ${alanBin} daemon start`, this.maxLogBuffer);
    }

    try {
      const { code, output } = await runCommand(
        alanBin,
        ["daemon", "start"],
        env,
        this.config.cwd,
        this.config.startupTimeout,
      );

      if (output) {
        for (const line of output.split(/\r?\n/)) {
          pushLog(this.logBuffer, line, this.maxLogBuffer);
        }
      }

      if (code !== 0) {
        const error = output || `alan daemon start failed with code ${code}`;
        this.status = { state: "error", url: this.status.url, error };
        throw new Error(error);
      }

      await this.waitForReady(this.config.startupTimeout);
      this.status = { state: "running", url: this.status.url };
      this.startedByTui = true;
      return this.status;
    } catch (error) {
      const message = (error as Error).message;
      this.status = { state: "error", url: this.status.url, error: message };
      throw error;
    }
  }

  async stop(): Promise<void> {
    // Never stop a daemon we did not start.
    if (!this.startedByTui) {
      this.status = (await this.isRunning())
        ? { state: "running", url: this.status.url }
        : { state: "stopped", url: this.status.url };
      return;
    }

    const alanBin = await this.findAlanBinary();
    if (!alanBin) {
      this.status = { state: "stopped", url: this.status.url };
      this.startedByTui = false;
      return;
    }

    const env: Record<string, string> = {
      ...process.env,
      ...this.config.env,
      ALAN_INSTALL_CHANNEL: this.installChannel,
      BIND_ADDRESS: `${this.config.host}:${this.config.port}`,
    };

    try {
      const { output } = await runCommand(alanBin, ["daemon", "stop"], env, this.config.cwd, 8000);
      if (output) {
        for (const line of output.split(/\r?\n/)) {
          pushLog(this.logBuffer, line, this.maxLogBuffer);
        }
      }
    } finally {
      this.startedByTui = false;
      this.status = (await this.isRunning())
        ? { state: "running", url: this.status.url }
        : { state: "stopped", url: this.status.url };
    }
  }

  getStatus(): DaemonStatus {
    return { ...this.status };
  }

  getLogs(): string[] {
    return [...this.logBuffer];
  }
}

let globalDaemon: DaemonManager | null = null;

export function getDaemon(config?: DaemonConfig): DaemonManager {
  if (!globalDaemon) {
    globalDaemon = new DaemonManager(config);
  }
  return globalDaemon;
}

export async function ensureDaemon(config?: DaemonConfig): Promise<DaemonManager> {
  const daemon = getDaemon(config);

  if (daemon.getStatus().state !== "running") {
    await daemon.start();
  }

  return daemon;
}

export async function stopGlobalDaemon(): Promise<void> {
  if (globalDaemon) {
    await globalDaemon.stop();
    globalDaemon = null;
  }
}
