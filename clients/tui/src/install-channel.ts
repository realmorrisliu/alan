import { basename } from "node:path";

export type InstallChannelId = "stable" | "dev";

export interface InstallChannelDescriptor {
  id: InstallChannelId;
  cliName: string;
  tuiName: string;
  alanHomeDirName: string;
  globalSkillsParentDirName: string;
  daemonBind: string;
  daemonUrl: string;
  daemonWsUrl: string;
}

const STABLE_DESCRIPTOR: InstallChannelDescriptor = {
  id: "stable",
  cliName: "alan",
  tuiName: "alan-tui",
  alanHomeDirName: ".alan",
  globalSkillsParentDirName: ".agents",
  daemonBind: "0.0.0.0:8090",
  daemonUrl: "http://127.0.0.1:8090",
  daemonWsUrl: "ws://127.0.0.1:8090",
};

const DEV_DESCRIPTOR: InstallChannelDescriptor = {
  id: "dev",
  cliName: "alan-dev",
  tuiName: "alan-dev-tui",
  alanHomeDirName: ".alan-dev",
  globalSkillsParentDirName: ".agents-dev",
  daemonBind: "127.0.0.1:8091",
  daemonUrl: "http://127.0.0.1:8091",
  daemonWsUrl: "ws://127.0.0.1:8091",
};

export function installChannelDescriptor(channel: InstallChannelId): InstallChannelDescriptor {
  return channel === "dev" ? DEV_DESCRIPTOR : STABLE_DESCRIPTOR;
}

export function resolveInstallChannel(
  env: NodeJS.ProcessEnv = process.env,
  executablePath: string | undefined = process.argv[1],
): InstallChannelId {
  const override = env.ALAN_INSTALL_CHANNEL?.trim();
  if (override === "stable" || override === "dev") {
    return override;
  }

  const executableName = executablePath ? basename(executablePath).replace(/\.exe$/, "") : "";
  return executableName === "alan-dev" || executableName === "alan-dev-tui" ? "dev" : "stable";
}
