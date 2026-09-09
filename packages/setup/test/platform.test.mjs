import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmod, mkdtemp, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

import {
  BINARY_OVERRIDE_ENVIRONMENT_VARIABLE,
  ensureExecutable,
  platformBinary,
} from "../platform.mjs";

const OWNER_EXECUTABLE_MASK = 0o100;
const OWNER_READ_WRITE_MODE = 0o600;
const SHARED_EXECUTABLE_MODE = 0o750;
const UNIX_PERMISSION_MASK = 0o777;
const ENTRYPOINT = fileURLToPath(new URL("../index.mjs", import.meta.url));

test("selects the bundled binary for Windows and Linux x64", () => {
  assert.deepEqual(platformBinary("win32", "x64"), {
    directory: "windows-x64",
    executable: "worklogger-mcp.exe",
  });
  assert.deepEqual(platformBinary("linux", "x64"), {
    directory: "linux-x64",
    executable: "worklogger-mcp",
  });
});

test("rejects platforms without an official binary", () => {
  assert.throws(
    () => platformBinary("linux", "arm64"),
    /Linux x64 y Windows x64/,
  );
});

test(
  "makes the Linux binary executable before launching it",
  { skip: process.platform !== "linux" },
  async () => {
    const directory = await mkdtemp(join(tmpdir(), "worklogger-bootstrap-"));
    const binary = join(directory, "worklogger-mcp");
    await writeFile(binary, "server");
    await chmod(binary, OWNER_READ_WRITE_MODE);

    await ensureExecutable(binary, "linux");

    const metadata = await stat(binary);
    assert.notEqual(metadata.mode & OWNER_EXECUTABLE_MASK, 0);
    await rm(directory, { recursive: true });
  },
);

test(
  "preserves permissions on an explicitly overridden binary",
  { skip: process.platform !== "linux" },
  async () => {
    const directory = await mkdtemp(join(tmpdir(), "worklogger-override-"));
    const binary = join(directory, "custom-server");
    await writeFile(binary, "#!/bin/sh\nexit 0\n");
    await chmod(binary, SHARED_EXECUTABLE_MODE);

    const environment = {
      ...process.env,
      [BINARY_OVERRIDE_ENVIRONMENT_VARIABLE]: binary,
    };
    const result = spawnSync(process.execPath, [ENTRYPOINT, "--help"], { env: environment });

    assert.equal(result.status, 0);
    assert.equal((await stat(binary)).mode & UNIX_PERMISSION_MASK, SHARED_EXECUTABLE_MODE);
    await rm(directory, { recursive: true });
  },
);
