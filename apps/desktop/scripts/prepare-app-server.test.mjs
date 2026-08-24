import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import {
  resolveAppServerSource,
  sidecarFileName,
} from "./prepare-app-server.mjs";

const desktopRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);

describe("App Server sidecar preparation", () => {
  it("uses Tauri's target-triple suffix before the Windows extension", () => {
    expect(sidecarFileName("x86_64-pc-windows-msvc", "win32")).toBe(
      "codex-app-server-x86_64-pc-windows-msvc.exe",
    );
  });

  it("prefers RIVLOOM_APP_SERVER_PATH over worktree build outputs", () => {
    const overridePath = path.resolve("D:/artifacts/codex-app-server.exe");
    const inspectedPaths = [];

    const source = resolveAppServerSource({
      overridePath,
      repoRoot: path.resolve("C:/worktree"),
      profile: "debug",
      platform: "win32",
      isFile(candidate) {
        inspectedPaths.push(candidate);
        return true;
      },
    });

    expect(source).toBe(overridePath);
    expect(inspectedPaths).toEqual([overridePath]);
  });

  it("reports how to provide or build a missing App Server", () => {
    expect(() =>
      resolveAppServerSource({
        repoRoot: path.resolve("C:/worktree"),
        profile: "debug",
        platform: "win32",
        isFile: () => false,
      }),
    ).toThrow(/RIVLOOM_APP_SERVER_PATH.*cargo build.*codex-app-server/s);
  });

  it("ignores generated binaries while retaining the ignore file", () => {
    const ignoreFile = readFileSync(
      path.join(desktopRoot, "src-tauri", "binaries", ".gitignore"),
      "utf8",
    );

    expect(ignoreFile.replaceAll("\r\n", "\n")).toBe("*\n!.gitignore\n");
  });
});
