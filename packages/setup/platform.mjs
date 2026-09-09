import { chmodSync } from "node:fs";
import { fileURLToPath } from "node:url";

const LINUX_PLATFORM = "linux";
const UNIX_EXECUTABLE_MODE = 0o700;
export const BINARY_OVERRIDE_ENVIRONMENT_VARIABLE = "WORKLOGGER_MCP_BINARY";
const SUPPORTED_BINARIES = Object.freeze({
  "linux:x64": Object.freeze({
    directory: "linux-x64",
    executable: "worklogger-mcp",
  }),
  "win32:x64": Object.freeze({
    directory: "windows-x64",
    executable: "worklogger-mcp.exe",
  }),
});

export function platformBinary(platform, architecture) {
  const descriptor = SUPPORTED_BINARIES[`${platform}:${architecture}`];
  if (descriptor) return descriptor;
  throw new Error("Este instalador soporta Linux x64 y Windows x64.");
}

export function bundledBinaryPath(moduleUrl, platform, architecture) {
  const descriptor = platformBinary(platform, architecture);
  const relativePath = `./bin/${descriptor.directory}/${descriptor.executable}`;
  return fileURLToPath(new URL(relativePath, moduleUrl));
}

export function ensureExecutable(binary, platform) {
  if (platform === LINUX_PLATFORM) chmodSync(binary, UNIX_EXECUTABLE_MODE);
}
