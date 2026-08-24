import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, statSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const defaultDesktopRoot = path.resolve(scriptDirectory, "..");
const defaultRepoRoot = path.resolve(defaultDesktopRoot, "..", "..");

export function sidecarFileName(targetTriple, platform = process.platform) {
  const normalizedTriple = targetTriple.trim();
  if (!normalizedTriple || !/^[A-Za-z0-9_.-]+$/.test(normalizedTriple)) {
    throw new Error(
      `Invalid Rust target triple: ${JSON.stringify(targetTriple)}`,
    );
  }

  const extension = platform === "win32" ? ".exe" : "";
  return `codex-app-server-${normalizedTriple}${extension}`;
}

export function resolveAppServerSource({
  overridePath,
  repoRoot,
  profile = "debug",
  platform = process.platform,
  isFile = isRegularFile,
}) {
  if (profile !== "debug" && profile !== "release") {
    throw new Error(
      `Unsupported App Server profile ${JSON.stringify(profile)}; use debug or release.`,
    );
  }

  if (overridePath?.trim()) {
    const resolvedOverride = path.resolve(overridePath);
    if (!isFile(resolvedOverride)) {
      throw new Error(
        `RIVLOOM_APP_SERVER_PATH does not point to a file: ${resolvedOverride}`,
      );
    }
    return resolvedOverride;
  }

  const executableName =
    platform === "win32" ? "codex-app-server.exe" : "codex-app-server";
  const worktreeBuild = path.join(
    repoRoot,
    "codex-rs",
    "target",
    profile,
    executableName,
  );
  if (isFile(worktreeBuild)) {
    return worktreeBuild;
  }

  throw new Error(
    `App Server binary not found at ${worktreeBuild}. ` +
      "Set RIVLOOM_APP_SERVER_PATH to an existing binary or run " +
      `cargo build -p codex-app-server in ${path.join(repoRoot, "codex-rs")}.`,
  );
}

export function prepareAppServer({
  env = process.env,
  repoRoot = defaultRepoRoot,
  desktopRoot = defaultDesktopRoot,
  platform = process.platform,
  targetTriple = rustHostTargetTriple(),
} = {}) {
  const source = resolveAppServerSource({
    overridePath: env.RIVLOOM_APP_SERVER_PATH,
    repoRoot,
    profile: env.RIVLOOM_APP_SERVER_PROFILE ?? "debug",
    platform,
  });
  const binaryDirectory = path.join(desktopRoot, "src-tauri", "binaries");
  const destination = path.join(
    binaryDirectory,
    sidecarFileName(targetTriple, platform),
  );

  mkdirSync(binaryDirectory, { recursive: true });
  if (path.resolve(source) !== path.resolve(destination)) {
    copyFileSync(source, destination);
  }

  return { source, destination, targetTriple };
}

function rustHostTargetTriple() {
  const executableName = process.platform === "win32" ? "rustc.exe" : "rustc";
  const candidates = [
    process.env.RUSTC,
    "rustc",
    path.join(homedir(), ".cargo", "bin", executableName),
  ].filter((candidate, index, values) => {
    return candidate && values.indexOf(candidate) === index;
  });

  let targetTriple = "";
  for (const candidate of candidates) {
    try {
      targetTriple = execFileSync(candidate, ["--print", "host-tuple"], {
        encoding: "utf8",
        windowsHide: true,
      }).trim();
      if (targetTriple) {
        break;
      }
    } catch (error) {
      if (error?.code !== "ENOENT") {
        throw error;
      }
    }
  }

  if (!targetTriple) {
    throw new Error(
      "rustc was not found or did not report a host target triple; install Rust 1.84 or newer.",
    );
  }
  return targetTriple;
}

function isRegularFile(candidate) {
  try {
    return statSync(candidate).isFile();
  } catch {
    return false;
  }
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  try {
    const prepared = prepareAppServer();
    console.log(
      `Prepared ${prepared.source} as ${prepared.destination} (${prepared.targetTriple}).`,
    );
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.error(`Failed to prepare the App Server sidecar: ${message}`);
    process.exitCode = 1;
  }
}
