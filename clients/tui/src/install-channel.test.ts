import { describe, expect, test } from "bun:test";
import { installChannelDescriptor, resolveInstallChannel } from "./install-channel.js";

describe("install channel", () => {
  test("stable descriptor preserves public defaults", () => {
    expect(installChannelDescriptor("stable")).toEqual({
      id: "stable",
      cliName: "alan",
      tuiName: "alan-tui",
      alanHomeDirName: ".alan",
      globalSkillsParentDirName: ".agents",
      daemonBind: "0.0.0.0:8090",
      daemonUrl: "http://127.0.0.1:8090",
      daemonWsUrl: "ws://127.0.0.1:8090",
    });
  });

  test("dev descriptor uses isolated local defaults", () => {
    expect(installChannelDescriptor("dev")).toEqual({
      id: "dev",
      cliName: "alan-dev",
      tuiName: "alan-dev-tui",
      alanHomeDirName: ".alan-dev",
      globalSkillsParentDirName: ".agents-dev",
      daemonBind: "127.0.0.1:8091",
      daemonUrl: "http://127.0.0.1:8091",
      daemonWsUrl: "ws://127.0.0.1:8091",
    });
  });

  test("environment override wins over executable name", () => {
    expect(resolveInstallChannel({ ALAN_INSTALL_CHANNEL: "dev" }, "alan-tui")).toBe("dev");
    expect(resolveInstallChannel({ ALAN_INSTALL_CHANNEL: "stable" }, "alan-dev-tui")).toBe(
      "stable",
    );
  });

  test("executable name selects dev channel", () => {
    expect(
      resolveInstallChannel({}, "/Applications/Alan Dev.app/Contents/MacOS/alan-dev-tui"),
    ).toBe("dev");
    expect(resolveInstallChannel({}, "alan-tui")).toBe("stable");
  });
});
