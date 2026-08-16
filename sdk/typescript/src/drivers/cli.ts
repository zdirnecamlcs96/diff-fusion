/**
 * CLI driver — mirrors the Rust `diffusion` binary (see `src/drivers/cli.rs`
 * + `src/main.rs`).
 *
 * Subcommand surface (single command today):
 *
 *   diffusion diff <a> <b> --schema <path> [--format-a <id>] [--format-b <id>]
 *
 * Output layout matches the Rust output line-for-line so existing scripts can
 * parse either runtime's output interchangeably:
 *
 *   Transformed to CIF:        (cyan)
 *   CIF A:                     (dimmed)
 *   <pretty-printed JSON>
 *                              (blank line)
 *   CIF B:                     (dimmed)
 *   <pretty-printed JSON>
 *                              (blank line)
 *   ✓ No differences found.    (green bold)   — OR —
 *   ✗ Differences found:       (yellow bold)
 *     <path>: <oldValue> → <newValue>  (blue : red  dim-arrow  green)
 *
 * Exit status: `0` on any successful parse (with or without diffs). `1` on
 * IO/parse/transform failure, with a human-readable error on stderr. Matches
 * Rust's `Box<dyn Error>` propagation behaviour (process exit non-zero).
 *
 * # Colour handling
 *
 * `picocolors` auto-detects TTY + the `FORCE_COLOR` / `NO_COLOR` env vars the
 * same way Rust's `colored` crate does, so piping to a file produces raw text
 * on both runtimes.
 *
 * # Testability
 *
 * The module is structured so tests can drive it without spawning a child
 * process: {@link run} accepts an `io` seam for `stdout`, `stderr`, and file
 * reading, and it returns the exit code rather than calling `process.exit()`.
 * The shebang entry (`bin/diffFusion.ts`) is the only place that touches
 * `process`.
 */

import { readFileSync } from "node:fs";
import { Command } from "commander";
import pc from "picocolors";
import type { JsonValue } from "../domain/types.js";
import { compareJson } from "../domain/compare.js";
import { toCif } from "../application/transform.js";

/** I/O seam — lets tests capture output and file reads without a subprocess. */
export interface CliIO {
  stdout: (line: string) => void;
  stderr: (line: string) => void;
  readFile: (path: string) => string;
}

/** Default I/O: write to the real process streams; read UTF-8 files from disk. */
export const defaultIO: CliIO = {
  stdout: (line) => process.stdout.write(`${line}\n`),
  stderr: (line) => process.stderr.write(`${line}\n`),
  readFile: (path) => readFileSync(path, "utf8"),
};

/**
 * Build a configured `commander` program. Extracted so tests can inspect the
 * argument layout (flag names, defaults) without running any action.
 *
 * The returned program is pre-wired with action handlers that close over the
 * supplied `io` seam. Caller runs it via `program.parseAsync(argv)` or
 * `program.parse(argv)`.
 */
export function buildProgram(io: CliIO = defaultIO): {
  program: Command;
  state: { exitCode: number };
} {
  const state = { exitCode: 0 };
  const program = new Command();
  program
    .name("diffusion")
    .description("A JSON diff and transformer CLI tool")
    // The Rust binary doesn't advertise a version; keep parity.
    .helpOption("-h, --help", "Show help");

  program
    .command("diff")
    .description("Compare two JSON files")
    .argument("<a>", "First JSON file path")
    .argument("<b>", "Second JSON file path")
    .requiredOption("-s, --schema <path>", "Schema file defining CIF (Common Intermediate Format)")
    .option("--format-a <id>", "Format identifier for file A", "format_a")
    .option("--format-b <id>", "Format identifier for file B", "format_b")
    .action(
      (
        a: string,
        b: string,
        opts: { schema: string; formatA: string; formatB: string },
      ) => {
        try {
          runDiff(io, {
            a,
            b,
            schema: opts.schema,
            formatA: opts.formatA,
            formatB: opts.formatB,
          });
        } catch (e) {
          io.stderr(formatError(e));
          state.exitCode = 1;
        }
      },
    );

  return { program, state };
}

/**
 * Parse `argv` and execute the matching subcommand. Returns the exit code
 * that `process.exit` should receive (0 success, 1 failure). Never throws —
 * errors from the action handler are caught and reported on stderr.
 */
export async function run(argv: readonly string[], io: CliIO = defaultIO): Promise<number> {
  const { program, state } = buildProgram(io);
  // commander's default behaviour on parse errors (missing required flag,
  // unknown option) is to print to stderr and `process.exit(1)`. Override to
  // surface through the `io` seam and the `state.exitCode` channel. Subcommand
  // help uses a separate exit path, so we apply the override recursively.
  applyExitOverride(program);
  applyOutputConfig(program, io);

  try {
    await program.parseAsync(argv as string[], { from: "user" });
  } catch (e) {
    // commander's CommanderError carries its own exitCode (1 for argument
    // errors, 0 when the user asked for --help). Preserve it.
    const err = e as { code?: string; exitCode?: number };
    if (typeof err.exitCode === "number") {
      return err.exitCode;
    }
    io.stderr(formatError(e));
    return 1;
  }
  return state.exitCode;
}

// ---------------------------------------------------------------------------
// `diff` subcommand — action handler.
// ---------------------------------------------------------------------------

interface DiffArgs {
  a: string;
  b: string;
  schema: string;
  formatA: string;
  formatB: string;
}

function runDiff(io: CliIO, args: DiffArgs): void {
  const jsonA = parseJsonFile(io, args.a);
  const jsonB = parseJsonFile(io, args.b);
  const schema = parseJsonFile(io, args.schema);

  const cifA = toCif(jsonA, schema, args.formatA);
  if (!cifA.ok) throw new Error(cifA.error);
  const cifB = toCif(jsonB, schema, args.formatB);
  if (!cifB.ok) throw new Error(cifB.error);

  io.stdout(pc.cyan("Transformed to CIF:"));
  io.stdout(pc.dim("CIF A:"));
  io.stdout(JSON.stringify(cifA.value, null, 2));
  io.stdout("");
  io.stdout(pc.dim("CIF B:"));
  io.stdout(JSON.stringify(cifB.value, null, 2));
  io.stdout("");

  const diff = compareJson(cifA.value, cifB.value);
  if (diff.length === 0) {
    io.stdout(pc.green(pc.bold("✓ No differences found.")));
    return;
  }

  io.stdout(pc.yellow(pc.bold("✗ Differences found:")));
  for (const [path, [oldVal, newVal]] of diff) {
    io.stdout(
      `  ${pc.blue(path)}: ${pc.red(JSON.stringify(oldVal))} ${pc.dim("→")} ${pc.green(JSON.stringify(newVal))}`,
    );
  }
}

function parseJsonFile(io: CliIO, path: string): JsonValue {
  let text: string;
  try {
    text = io.readFile(path);
  } catch (e) {
    throw new Error(`failed to read '${path}': ${messageOf(e)}`);
  }
  try {
    // ponytail: no zod here — JSON.parse's output space IS JsonValue, so validating
    // is a tautological walk with no rejection power, at real cost on large files.
    return JSON.parse(text) as JsonValue;
  } catch (e) {
    throw new Error(`failed to parse JSON in '${path}': ${messageOf(e)}`);
  }
}

function formatError(e: unknown): string {
  return `error: ${messageOf(e)}`;
}

function messageOf(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}

function applyExitOverride(cmd: Command): void {
  cmd.exitOverride();
  for (const sub of cmd.commands) applyExitOverride(sub);
}

function applyOutputConfig(cmd: Command, io: CliIO): void {
  cmd.configureOutput({
    writeOut: (str) => io.stdout(str.replace(/\n$/, "")),
    writeErr: (str) => io.stderr(str.replace(/\n$/, "")),
  });
  for (const sub of cmd.commands) applyOutputConfig(sub, io);
}
