import { describe, expect, test } from "bun:test";
import { resolveAlanBinaryFromCandidates } from "./daemon.js";

describe("daemon binary resolution", () => {
  test("returns null when only PATH fallback exists but command is unavailable", () => {
    const resolved = resolveAlanBinaryFromCandidates(
      ["alan"],
      () => false,
      () => false,
    );
    expect(resolved).toBeNull();
  });

  test("returns PATH fallback when command is available", () => {
    const resolved = resolveAlanBinaryFromCandidates(
      ["alan"],
      () => false,
      () => true,
    );
    expect(resolved).toBe("alan");
  });

  test("returns dev PATH fallback when alan-dev command is available", () => {
    const resolved = resolveAlanBinaryFromCandidates(
      ["alan-dev"],
      () => false,
      (command) => command === "alan-dev",
    );
    expect(resolved).toBe("alan-dev");
  });

  test("ignores directory-like candidate and returns runnable file candidate", () => {
    const resolved = resolveAlanBinaryFromCandidates(
      ["/tmp/not-a-binary", "/tmp/alan-bin"],
      (path) => path === "/tmp/alan-bin",
      () => false,
    );
    expect(resolved).toBe("/tmp/alan-bin");
  });

  test("prefers explicit runnable path over PATH fallback", () => {
    const resolved = resolveAlanBinaryFromCandidates(
      ["/tmp/alan-bin", "alan"],
      (path) => path === "/tmp/alan-bin",
      () => true,
    );
    expect(resolved).toBe("/tmp/alan-bin");
  });
});
