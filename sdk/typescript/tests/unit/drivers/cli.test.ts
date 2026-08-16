import { readFileSync } from "node:fs";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import { run, type CliIO } from "../../../src/drivers/cli.js";

// ---------------------------------------------------------------------------
// Test scaffolding: a CliIO that captures output into strings, plus helpers
// that drop schema + data fixtures into a throwaway temp dir.
// ---------------------------------------------------------------------------

const createdDirs: string[] = [];

afterAll(async () => {
  for (const d of createdDirs) {
    await rm(d, { recursive: true, force: true });
  }
});

async function tempDir(suffix: string): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), `diff-fusion-cli-${suffix}-`));
  createdDirs.push(dir);
  return dir;
}

function captureIO(): CliIO & { out: string[]; err: string[] } {
  const out: string[] = [];
  const err: string[] = [];
  return {
    stdout: (line) => out.push(line),
    stderr: (line) => err.push(line),
    readFile: (path) => readFileSync(path, "utf8"),
    out,
    err,
  };
}

// Minimal schema reused across several tests.
const baseSchema = {
  cif_schema: {
    product_name: { type: "string", required: true },
    price: { type: "number", required: true },
  },
  transformations: {
    format_a: {
      product_name: { source_path: "name", type: "string" },
      price: { source_path: "price", type: "number" },
    },
    format_b: {
      product_name: { source_path: "title", type: "string" },
      price: { source_path: "cost", type: "number" },
    },
  },
};

// Strip ANSI colour codes so assertions don't need to know picocolors encoding.
// picocolors auto-suppresses colour when stdout isn't a TTY (vitest case), but
// if a CI environment forces colour on, we still want assertions to work.
function stripAnsi(s: string): string {
  // eslint-disable-next-line no-control-regex
  return s.replace(/\x1B\[[0-9;]*[A-Za-z]/g, "");
}

function plainLines(captured: string[]): string[] {
  return captured.map(stripAnsi);
}

// ---------------------------------------------------------------------------
// `diff` subcommand — golden-path behaviour.
// ---------------------------------------------------------------------------

describe("cli diff — golden path", () => {
  it("prints the CIF A / CIF B blocks and no-difference banner on matching inputs", async () => {
    const dir = await tempDir("nodiff");
    const schemaPath = join(dir, "schema.json");
    const aPath = join(dir, "a.json");
    const bPath = join(dir, "b.json");
    await writeFile(schemaPath, JSON.stringify(baseSchema));
    await writeFile(aPath, JSON.stringify({ name: "Widget", price: 19.99 }));
    await writeFile(bPath, JSON.stringify({ name: "Widget", price: 19.99 }));

    const io = captureIO();
    const code = await run(
      ["diff", aPath, bPath, "--schema", schemaPath, "--format-a", "format_a", "--format-b", "format_a"],
      io,
    );

    expect(code).toBe(0);
    expect(io.err).toEqual([]);
    const lines = plainLines(io.out);
    // Layout: header, "CIF A:", pretty JSON, blank, "CIF B:", pretty JSON, blank, banner.
    expect(lines[0]).toBe("Transformed to CIF:");
    expect(lines[1]).toBe("CIF A:");
    expect(lines[lines.length - 1]).toBe("✓ No differences found.");
  });

  it("emits a dotted-path diff when values differ", async () => {
    const dir = await tempDir("diff");
    const schemaPath = join(dir, "schema.json");
    const aPath = join(dir, "a.json");
    const bPath = join(dir, "b.json");
    await writeFile(schemaPath, JSON.stringify(baseSchema));
    await writeFile(aPath, JSON.stringify({ name: "Widget", price: 19.99 }));
    await writeFile(bPath, JSON.stringify({ name: "Widget", price: 24.99 }));

    const io = captureIO();
    const code = await run(
      ["diff", aPath, bPath, "--schema", schemaPath, "--format-a", "format_a", "--format-b", "format_a"],
      io,
    );

    expect(code).toBe(0);
    const lines = plainLines(io.out);
    expect(lines).toContain("✗ Differences found:");
    const diffLine = lines.find((l) => l.trimStart().startsWith("price:"));
    expect(diffLine).toBeDefined();
    // Value formatting is `JSON.stringify` on each side; arrow is `→`.
    expect(diffLine).toMatch(/price: 19\.99 → 24\.99/);
  });

  it("defaults --format-a and --format-b to 'format_a' / 'format_b'", async () => {
    const dir = await tempDir("defaults");
    const schemaPath = join(dir, "schema.json");
    const aPath = join(dir, "a.json");
    const bPath = join(dir, "b.json");
    await writeFile(schemaPath, JSON.stringify(baseSchema));
    await writeFile(aPath, JSON.stringify({ name: "Widget", price: 5 }));
    await writeFile(bPath, JSON.stringify({ title: "Widget", cost: 5 }));

    const io = captureIO();
    const code = await run(["diff", aPath, bPath, "--schema", schemaPath], io);

    expect(code).toBe(0);
    expect(plainLines(io.out)).toContain("✓ No differences found.");
  });

  it("transforms sides with different formats when --format-a / --format-b differ", async () => {
    const dir = await tempDir("heterogeneous");
    const schemaPath = join(dir, "schema.json");
    const aPath = join(dir, "a.json");
    const bPath = join(dir, "b.json");
    await writeFile(schemaPath, JSON.stringify(baseSchema));
    await writeFile(aPath, JSON.stringify({ name: "Alpha", price: 10 }));
    await writeFile(bPath, JSON.stringify({ title: "Beta", cost: 10 }));

    const io = captureIO();
    const code = await run(
      [
        "diff",
        aPath,
        bPath,
        "--schema",
        schemaPath,
        "--format-a",
        "format_a",
        "--format-b",
        "format_b",
      ],
      io,
    );

    expect(code).toBe(0);
    const lines = plainLines(io.out);
    expect(lines).toContain("✗ Differences found:");
    // Only `product_name` differs — Alpha vs Beta after transformation.
    expect(
      lines.some((l) => l.trimStart().startsWith("product_name:")),
    ).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Error handling.
// ---------------------------------------------------------------------------

describe("cli diff — errors", () => {
  it("exits non-zero when a JSON file can't be read", async () => {
    const dir = await tempDir("missing");
    const schemaPath = join(dir, "schema.json");
    await writeFile(schemaPath, JSON.stringify(baseSchema));

    const io = captureIO();
    const code = await run(
      [
        "diff",
        join(dir, "does-not-exist-a.json"),
        join(dir, "does-not-exist-b.json"),
        "--schema",
        schemaPath,
      ],
      io,
    );

    expect(code).toBe(1);
    expect(io.err.join("\n")).toMatch(/failed to read/);
  });

  it("exits non-zero when a JSON file is malformed", async () => {
    const dir = await tempDir("malformed");
    const schemaPath = join(dir, "schema.json");
    const aPath = join(dir, "a.json");
    const bPath = join(dir, "b.json");
    await writeFile(schemaPath, JSON.stringify(baseSchema));
    await writeFile(aPath, "{not json");
    await writeFile(bPath, JSON.stringify({ name: "x", price: 1 }));

    const io = captureIO();
    const code = await run(
      ["diff", aPath, bPath, "--schema", schemaPath],
      io,
    );

    expect(code).toBe(1);
    expect(io.err.join("\n")).toMatch(/failed to parse JSON/);
  });

  it("exits non-zero when the schema lacks the requested format", async () => {
    const dir = await tempDir("no-format");
    const schemaPath = join(dir, "schema.json");
    const aPath = join(dir, "a.json");
    const bPath = join(dir, "b.json");
    await writeFile(schemaPath, JSON.stringify(baseSchema));
    await writeFile(aPath, JSON.stringify({ name: "x", price: 1 }));
    await writeFile(bPath, JSON.stringify({ name: "x", price: 1 }));

    const io = captureIO();
    const code = await run(
      [
        "diff",
        aPath,
        bPath,
        "--schema",
        schemaPath,
        "--format-a",
        "no_such_format",
      ],
      io,
    );

    expect(code).toBe(1);
    expect(io.err.length).toBeGreaterThan(0);
  });

  it("exits non-zero when --schema is missing", async () => {
    const io = captureIO();
    const code = await run(["diff", "a.json", "b.json"], io);
    // commander's "required option missing" path exits with code 1.
    expect(code).toBe(1);
  });

  it("exits non-zero when an unknown subcommand is used", async () => {
    const io = captureIO();
    const code = await run(["bogus-command"], io);
    expect(code).not.toBe(0);
  });
});

// ---------------------------------------------------------------------------
// Help / invocation surface.
// ---------------------------------------------------------------------------

describe("cli help", () => {
  it("prints help and exits zero for --help", async () => {
    const io = captureIO();
    const code = await run(["--help"], io);
    expect(code).toBe(0);
    const text = [...io.out, ...io.err].join("\n");
    expect(text).toMatch(/diffusion/);
    expect(text).toMatch(/diff/);
  });

  it("prints subcommand help for `diff --help`", async () => {
    const io = captureIO();
    const code = await run(["diff", "--help"], io);
    expect(code).toBe(0);
    const text = [...io.out, ...io.err].join("\n");
    expect(text).toMatch(/--schema/);
    expect(text).toMatch(/--format-a/);
    expect(text).toMatch(/--format-b/);
  });
});
