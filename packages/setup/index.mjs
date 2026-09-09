#!/usr/bin/env node

import { existsSync } from "node:fs";
import { spawnSync } from "node:child_process";

import { forwardedArguments } from "./arguments.mjs";
import {
  BINARY_OVERRIDE_ENVIRONMENT_VARIABLE,
  bundledBinaryPath,
  ensureExecutable,
} from "./platform.mjs";

const overriddenBinary = process.env[BINARY_OVERRIDE_ENVIRONMENT_VARIABLE];
const binary = overriddenBinary || resolveBundledBinary();
if (!existsSync(binary)) {
  fail(`No se encontró el servidor MCP en ${binary}`);
}
if (!overriddenBinary) ensureExecutable(binary, process.platform);

const argumentsToForward = forwardedArguments(process.argv);
const result = spawnSync(binary, argumentsToForward, { stdio: "inherit" });
if (result.error) {
  fail(result.error.message);
}
process.exitCode = result.status ?? 1;

function resolveBundledBinary() {
  try {
    return bundledBinaryPath(import.meta.url, process.platform, process.arch);
  } catch (error) {
    fail(error instanceof Error ? error.message : String(error));
  }
}

function fail(message) {
  console.error(`Worklogger MCP: ${message}`);
  process.exit(1);
}
