import { statSync } from "node:fs";
import { join } from "node:path";
import {
  installChannelDescriptor,
  resolveInstallChannel,
  type InstallChannelId,
} from "./install-channel.js";

function expandHomePath(path: string, homeDir: string): string {
  if (!path.startsWith("~/")) {
    return path;
  }
  return join(homeDir, path.slice(2));
}

function dedupe(paths: string[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const path of paths) {
    if (!seen.has(path)) {
      seen.add(path);
      out.push(path);
    }
  }
  return out;
}

export function resolveConfigPathCandidates(
  homeDir: string,
  env: NodeJS.ProcessEnv = process.env,
  channel: InstallChannelId = resolveInstallChannel(env),
): string[] {
  // Offline first-run setup mirror of alan_runtime::AgentRootLayout.
  // Online flows should display daemon-returned canonical paths instead.
  const descriptor = installChannelDescriptor(channel);
  const canonicalPath = join(
    homeDir,
    descriptor.alanHomeDirName,
    "agents",
    "default",
    "agent.toml",
  );
  const overrideRaw = env.ALAN_CONFIG_PATH?.trim();
  if (!overrideRaw) {
    return [canonicalPath];
  }

  const overridePath = expandHomePath(overrideRaw, homeDir);
  return dedupe([overridePath, canonicalPath]);
}

export function defaultHostConfigPath(
  homeDir: string,
  channel: InstallChannelId = resolveInstallChannel(),
): string {
  return join(homeDir, installChannelDescriptor(channel).alanHomeDirName, "host.toml");
}

export function resolveAgentdUrlOverride(env: NodeJS.ProcessEnv = process.env): string | null {
  const raw = env.ALAN_AGENTD_URL?.trim();
  return raw ? raw : null;
}

export function selectExistingConfigPath(
  candidates: string[],
  isConfigFile: (path: string) => boolean,
): string | null {
  for (const candidate of candidates) {
    if (isConfigFile(candidate)) {
      return candidate;
    }
  }
  return null;
}

export function shouldRunFirstTimeSetup(
  candidates: string[],
  isConfigFile: (path: string) => boolean,
): boolean {
  return selectExistingConfigPath(candidates, isConfigFile) === null;
}

export function isExistingConfigFile(path: string): boolean {
  try {
    return statSync(path).isFile();
  } catch {
    return false;
  }
}
