import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { forwardedArguments } from "../arguments.mjs";

const manifestUrl = new URL("../package.json", import.meta.url);

test("leaves the default command to the Rust CLI", () => {
  assert.deepEqual(forwardedArguments(["node", "index.mjs"]), []);
});

test("forwards every argument to the Rust CLI", () => {
  assert.deepEqual(
    forwardedArguments(["node", "index.mjs", "uninstall", "unexpected"]),
    ["uninstall", "unexpected"],
  );
});

test("publishes the public package with the canonical command", async () => {
  const manifest = JSON.parse(await readFile(manifestUrl, "utf8"));

  assert.equal(manifest.name, "@santiv343/worklogger");
  assert.equal(manifest.bin.worklogger, "index.mjs");
  assert.equal(manifest.publishConfig.access, "public");
  assert.equal(manifest.publishConfig.registry, "https://registry.npmjs.org");
  assert.equal(manifest.license, "MIT");
  assert.ok(manifest.files.includes("LICENSE"));
  assert.ok(manifest.files.includes("bin/linux-x64/worklogger-mcp"));
  assert.ok(manifest.files.includes("bin/windows-x64/worklogger-mcp.exe"));
});
