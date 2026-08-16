#!/usr/bin/env node
/**
 * Executable entry point for the `diff-fusion` CLI.
 *
 * Parses `process.argv`, invokes the driver in `drivers/cli`, and exits with
 * the returned code. This file is the only place that touches `process.*` so
 * tests can drive the CLI without spawning a child (see `drivers/cli.ts::run`).
 */

import { run } from "../drivers/cli.js";

// commander expects argv *without* the leading "node <script>" pair; strip.
const userArgs = process.argv.slice(2);

run(userArgs).then(
  (code) => process.exit(code),
  (e: unknown) => {
    process.stderr.write(`fatal: ${e instanceof Error ? e.message : String(e)}\n`);
    process.exit(1);
  },
);
