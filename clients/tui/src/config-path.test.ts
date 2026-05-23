import { describe, expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  defaultHostConfigPath,
  isExistingConfigFile,
  resolveAgentdUrlOverride,
  resolveConfigPathCandidates,
  selectExistingConfigPath,
  shouldRunFirstTimeSetup,
} from "./config-path.js";

describe("config path resolution", () => {
  const home = "/Users/tester";
  const canonicalPath = `${home}/.alan/agents/default/agent.toml`;

  test("uses default config path when override is unset", () => {
    const candidates = resolveConfigPathCandidates(home, {});
    expect(candidates).toEqual([canonicalPath]);
  });

  test("adds override before default when ALAN_CONFIG_PATH is set", () => {
    const override = "/tmp/custom.toml";
    const candidates = resolveConfigPathCandidates(home, {
      ALAN_CONFIG_PATH: override,
    });
    expect(candidates).toEqual([override, canonicalPath]);
  });

  test("expands ~/ override path", () => {
    const candidates = resolveConfigPathCandidates(home, {
      ALAN_CONFIG_PATH: "~/custom.toml",
    });
    expect(candidates).toEqual([`${home}/custom.toml`, canonicalPath]);
  });

  test("selects default path when override does not exist", () => {
    const candidates = resolveConfigPathCandidates(home, {
      ALAN_CONFIG_PATH: "/tmp/missing.toml",
    });
    const existing = selectExistingConfigPath(candidates, (path) => path === canonicalPath);
    expect(existing).toBe(canonicalPath);
  });

  test("does not run setup when canonical config exists and override is missing", () => {
    const candidates = resolveConfigPathCandidates(home, {
      ALAN_CONFIG_PATH: "/tmp/missing.toml",
    });
    const needsSetup = shouldRunFirstTimeSetup(candidates, (path) => path === canonicalPath);
    expect(needsSetup).toBe(false);
  });

  test("returns no existing config when canonical config is missing", () => {
    const candidates = resolveConfigPathCandidates(home, {});
    const existing = selectExistingConfigPath(candidates, () => false);
    expect(existing).toBeNull();
  });

  test("isExistingConfigFile returns false for directory and true for regular file", () => {
    const tempRoot = mkdtempSync(join(tmpdir(), "alan-config-path-"));
    const configDir = join(tempRoot, "config-dir");
    const configFile = join(tempRoot, "config.toml");
    mkdirSync(configDir);
    writeFileSync(configFile, 'connection_profile = "gemini"\n');

    expect(isExistingConfigFile(configDir)).toBe(false);
    expect(isExistingConfigFile(configFile)).toBe(true);

    rmSync(tempRoot, { recursive: true, force: true });
  });

  test("defaultHostConfigPath returns canonical host location", () => {
    expect(defaultHostConfigPath(home)).toBe(`${home}/.alan/host.toml`);
  });

  test("dev channel uses isolated config and host paths", () => {
    expect(resolveConfigPathCandidates(home, {}, "dev")).toEqual([
      `${home}/.alan-dev/agents/default/agent.toml`,
    ]);
    expect(defaultHostConfigPath(home, "dev")).toBe(`${home}/.alan-dev/host.toml`);
  });

  test("resolveAgentdUrlOverride trims non-empty overrides", () => {
    expect(
      resolveAgentdUrlOverride({
        ALAN_AGENTD_URL: " http://127.0.0.1:9123 ",
      }),
    ).toBe("http://127.0.0.1:9123");
  });

  test("resolveAgentdUrlOverride ignores blank overrides", () => {
    expect(resolveAgentdUrlOverride({ ALAN_AGENTD_URL: "   " })).toBeNull();
    expect(resolveAgentdUrlOverride({})).toBeNull();
  });
});
