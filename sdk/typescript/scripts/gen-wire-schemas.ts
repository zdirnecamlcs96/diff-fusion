/**
 * Generates zod v4 schemas + `z.infer` types from `spec/schema/*.json`
 * (draft-07, schemars 0.8 output; see `scripts/schema-to-md.py`'s header for
 * the exact construct set this is scoped to — no `allOf`, no >2-branch
 * `anyOf`, no `$defs`).
 *
 * `transformSchema` is the pure JSON-Schema -> TS-source transform (no I/O),
 * exported so `tests/unit/generated/schemasDrift.test.ts` can re-run it
 * in-memory against the committed schema JSON and diff the result against
 * the committed generated files. `main()` is the CLI entry point run via
 * `npm run gen:wire-schemas`.
 */
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// ---------- schema JSON shape (only the fields this generator reads) ----------

type SchemaFragment = true | SchemaObject;

interface SchemaObject {
  $ref?: string;
  type?: string;
  enum?: string[];
  items?: SchemaFragment;
  properties?: Record<string, SchemaFragment>;
  required?: string[];
  additionalProperties?: SchemaFragment;
  default?: unknown;
  minimum?: number;
  anyOf?: SchemaObject[];
  oneOf?: SchemaObject[];
  title?: string;
  definitions?: Record<string, SchemaObject>;
}

interface FileConfig {
  schemaFile: string;
  outFile: string;
  /** Override for the root export name when it differs from `title`. */
  rootExportName?: string;
  /** Definition name -> module specifier to import instead of emitting locally. */
  crossFile?: Record<string, string>;
}

export const CONFIGS: readonly FileConfig[] = [
  { schemaFile: "wire-changelog.schema.json", outFile: "wireChangelog.ts" },
  {
    schemaFile: "merge-outcome.schema.json",
    outFile: "mergeOutcome.ts",
    rootExportName: "MergeOutcome",
  },
  { schemaFile: "policy-config.schema.json", outFile: "policyConfig.ts" },
  {
    schemaFile: "policy-document.schema.json",
    outFile: "policyDocument.ts",
    crossFile: { MergePolicyRef: "./policyConfig.js" },
  },
];

// ---------- indentation toolkit ----------
//
// Every builder below returns a string assuming it starts at column 0
// ("depth-0-relative"); embedding it one level deeper (as an object field's
// value, or as an array element) means shifting every line by exactly one
// level. Because the shift is a uniform per-line prefix, this composes
// correctly at any nesting depth without each builder needing to know its
// final depth.

function indentBlock(block: string, levels: number): string {
  const pad = "  ".repeat(levels);
  return block
    .split("\n")
    .map((l) => (l === "" ? "" : pad + l))
    .join("\n");
}

interface FieldSpec {
  name: string;
  expr: string; // zod expression, depth-0-relative
  tsType: string;
  optional: boolean;
}

function emitField(f: FieldSpec): string {
  const [first, ...rest] = f.expr.split("\n");
  const contIndented = rest.map((l) => (l === "" ? "" : `  ${l}`));
  return [`${f.name}: ${first}`, ...contIndented].join("\n") + ",";
}

function renderZodObject(fields: readonly FieldSpec[]): string {
  const body = fields.map(emitField).join("\n");
  return `z.object({\n${indentBlock(body, 1)}\n})`;
}

function renderArray(items: readonly string[]): string {
  const body = items.map((it) => `${it},`).join("\n");
  return `[\n${indentBlock(body, 1)}\n]`;
}

// ---------- name helpers ----------

function lowerFirst(s: string): string {
  return s.charAt(0).toLowerCase() + s.slice(1);
}

function refName(ref: string): string {
  return ref.replace("#/definitions/", "");
}

// ---------- generic $ref collection (for reachability / emission order) ----------

function collectRefs(node: unknown): string[] {
  const found = new Set<string>();
  const walk = (n: unknown): void => {
    if (Array.isArray(n)) {
      for (const item of n) walk(item);
      return;
    }
    if (n && typeof n === "object") {
      for (const [k, v] of Object.entries(n as Record<string, unknown>)) {
        if (k === "$ref" && typeof v === "string") {
          found.add(refName(v));
        } else {
          walk(v);
        }
      }
    }
  };
  walk(node);
  return [...found];
}

// ---------- schema fragment -> zod/ts (leaf positions: array items, map values) ----------

interface FileCtx {
  crossFile: Record<string, string>;
  crossFileUsed: Set<string>;
  needsJsonValue: boolean;
}

function fragmentToParts(schema: SchemaFragment, ctx: FileCtx): { zod: string; ts: string } {
  if (schema === true) {
    ctx.needsJsonValue = true;
    return { zod: "jsonValueSchema", ts: "JsonValue" };
  }
  if (schema.$ref !== undefined) {
    const name = refName(schema.$ref);
    if (ctx.crossFile[name]) ctx.crossFileUsed.add(name);
    return { zod: `${lowerFirst(name)}Schema`, ts: name };
  }
  if (schema.type === "string" && Array.isArray(schema.enum)) {
    const vals = schema.enum;
    return {
      zod: `z.enum([${vals.map((v) => JSON.stringify(v)).join(", ")}])`,
      ts: vals.map((v) => JSON.stringify(v)).join(" | "),
    };
  }
  if (schema.type === "array" && schema.items !== undefined) {
    const sub = fragmentToParts(schema.items, ctx);
    return { zod: `z.array(${sub.zod})`, ts: `${sub.ts}[]` };
  }
  if (schema.type === "object" && schema.additionalProperties !== undefined) {
    // Caller applies `.default(...)` uniformly (see buildObjectFields) — every
    // map-typed property in these schemas carries `"default": {}` alongside.
    const sub = fragmentToParts(schema.additionalProperties, ctx);
    return { zod: `z.record(z.string(), ${sub.zod})`, ts: `Record<string, ${sub.ts}>` };
  }
  if (schema.type === "integer") {
    let zod = "z.number().int()";
    if (schema.minimum === 0) zod += ".nonnegative()";
    return { zod, ts: "number" };
  }
  if (schema.type === "string") return { zod: "z.string()", ts: "string" };
  if (schema.type === "boolean") return { zod: "z.boolean()", ts: "boolean" };
  throw new Error(`gen-wire-schemas: unsupported schema fragment: ${JSON.stringify(schema)}`);
}

// ---------- object properties -> FieldSpec[] ----------

function detectDiscriminatorKey(branches: readonly SchemaObject[]): string {
  const first = branches[0];
  if (!first?.properties) throw new Error("gen-wire-schemas: oneOf branch has no properties");
  const commonKeys = Object.keys(first.properties).filter((k) =>
    branches.every((b) => b.properties && k in b.properties),
  );
  for (const key of commonKeys) {
    const ok = branches.every((b) => {
      const req = new Set(b.required ?? []);
      const prop = b.properties?.[key];
      return req.has(key) && prop !== true && Array.isArray(prop?.enum) && prop.enum.length === 1;
    });
    if (ok) return key;
  }
  throw new Error("gen-wire-schemas: no discriminator key found in oneOf branches");
}

function buildObjectFields(schema: SchemaObject, ctx: FileCtx, tagKey?: string): FieldSpec[] {
  const required = new Set(schema.required ?? []);
  const props = schema.properties ?? {};
  const fields: FieldSpec[] = [];

  for (const [name, prop] of Object.entries(props)) {
    if (name === tagKey && prop !== true && Array.isArray(prop.enum)) {
      const value = prop.enum[0];
      fields.push({
        name,
        expr: `z.literal(${JSON.stringify(value)})`,
        tsType: JSON.stringify(value),
        optional: false,
      });
      continue;
    }

    if (prop === true) {
      ctx.needsJsonValue = true;
      const isRequired = required.has(name);
      fields.push({
        name,
        expr: isRequired ? "jsonValueSchema" : "jsonValueSchema.optional()",
        tsType: "JsonValue",
        optional: !isRequired,
      });
      continue;
    }

    // 2-branch nullable anyOf: [X, null].
    if (
      Array.isArray(prop.anyOf) &&
      prop.anyOf.length === 2 &&
      prop.anyOf.some((b) => b.type === "null")
    ) {
      const other = prop.anyOf.find((b) => b.type !== "null");
      if (!other) throw new Error(`gen-wire-schemas: malformed nullable anyOf on '${name}'`);
      const parts = fragmentToParts(other, ctx);
      if ("default" in prop) {
        // schemars `Option<T>` override used only to dodge an allOf-wrapping
        // bug (see declaration.rs) — the runtime field is non-nullable with
        // a concrete default, not a genuine optional.
        fields.push({
          name,
          expr: `${parts.zod}.default(${JSON.stringify(prop.default)})`,
          tsType: parts.ts,
          optional: false,
        });
      } else {
        // Genuine optional-nullable: absent-or-explicit-null both collapse
        // to `undefined` in the parsed shape. Matches the hand-written
        // idiom in application/policy/declaration.ts's `default` field.
        fields.push({
          name,
          expr: `${parts.zod}\n  .nullish()\n  .transform((v) => v ?? undefined)\n  .optional()`,
          tsType: `${parts.ts} | undefined`,
          optional: true,
        });
      }
      continue;
    }

    const parts = fragmentToParts(prop, ctx);
    if ("default" in prop) {
      fields.push({
        name,
        expr: `${parts.zod}.default(${JSON.stringify(prop.default)})`,
        tsType: parts.ts,
        optional: false,
      });
    } else if (required.has(name)) {
      fields.push({ name, expr: parts.zod, tsType: parts.ts, optional: false });
    } else {
      fields.push({
        name,
        expr: `${parts.zod}.optional()`,
        tsType: `${parts.ts} | undefined`,
        optional: true,
      });
    }
  }

  return fields;
}

// ---------- one named definition (or the root) -> a full export block ----------

function emitDefinition(
  name: string,
  schema: SchemaObject,
  ctx: FileCtx,
  isRecursive: boolean,
): string {
  const constName = `${lowerFirst(name)}Schema`;

  if (isRecursive) {
    const fields = buildObjectFields(schema, ctx);
    const ifaceBody = fields
      .map((f) => `  ${f.name}${f.optional ? "?" : ""}: ${f.tsType.replace(" | undefined", "")};`)
      .join("\n");
    const objBlock = indentBlock(renderZodObject(fields), 1);
    return [
      `export interface ${name} {`,
      ifaceBody,
      `}`,
      ``,
      `export const ${constName}: z.ZodType<${name}> = z.lazy(() =>\n${objBlock},\n);`,
    ].join("\n");
  }

  if (schema.type === "string" && Array.isArray(schema.enum)) {
    const vals = schema.enum;
    return [
      `export const ${constName} = z.enum([${vals.map((v) => JSON.stringify(v)).join(", ")}]);`,
      `export type ${name} = z.infer<typeof ${constName}>;`,
    ].join("\n");
  }

  if (Array.isArray(schema.oneOf)) {
    const allSplitEnum = schema.oneOf.every(
      (b) => b.type === "string" && Array.isArray(b.enum) && !b.properties,
    );
    if (allSplitEnum) {
      const vals = schema.oneOf.flatMap((b) => b.enum ?? []);
      return [
        `export const ${constName} = z.enum([${vals.map((v) => JSON.stringify(v)).join(", ")}]);`,
        `export type ${name} = z.infer<typeof ${constName}>;`,
      ].join("\n");
    }
    const tagKey = detectDiscriminatorKey(schema.oneOf);
    const branches = schema.oneOf.map((b) => renderZodObject(buildObjectFields(b, ctx, tagKey)));
    const union = `z.discriminatedUnion(${JSON.stringify(tagKey)}, ${renderArray(branches)})`;
    return [
      `export const ${constName} = ${union};`,
      `export type ${name} = z.infer<typeof ${constName}>;`,
    ].join("\n");
  }

  if (schema.type === "object" && schema.properties) {
    const fields = buildObjectFields(schema, ctx);
    return [
      `export const ${constName} = ${renderZodObject(fields)};`,
      `export type ${name} = z.infer<typeof ${constName}>;`,
    ].join("\n");
  }

  throw new Error(`gen-wire-schemas: unsupported definition shape for '${name}'`);
}

// ---------- per-file driver ----------

export function transformSchema(config: FileConfig, schemaJson: SchemaObject): string {
  const ctx: FileCtx = {
    crossFile: config.crossFile ?? {},
    crossFileUsed: new Set(),
    needsJsonValue: false,
  };
  const defs = schemaJson.definitions ?? {};

  const order: string[] = [];
  const visited = new Set<string>();
  const recursive = new Set<string>();

  const visit = (n: string, stack: Set<string>): void => {
    if (ctx.crossFile[n]) {
      ctx.crossFileUsed.add(n);
      return;
    }
    if (visited.has(n)) return;
    if (stack.has(n)) {
      recursive.add(n);
      return;
    }
    if (!(n in defs)) {
      throw new Error(`gen-wire-schemas: unresolved $ref '${n}' in ${config.schemaFile}`);
    }
    stack.add(n);
    for (const r of collectRefs(defs[n])) visit(r, stack);
    stack.delete(n);
    visited.add(n);
    order.push(n);
  };

  const { definitions: _definitions, ...rootOnly } = schemaJson;
  for (const r of collectRefs(rootOnly)) visit(r, new Set());

  const rootName = config.rootExportName ?? schemaJson.title;
  if (!rootName) throw new Error(`gen-wire-schemas: ${config.schemaFile} has no title`);

  const blocks = order.map((n) => {
    const def = defs[n];
    if (!def) throw new Error(`gen-wire-schemas: missing definition '${n}'`);
    return emitDefinition(n, def, ctx, recursive.has(n));
  });
  blocks.push(emitDefinition(rootName, schemaJson, ctx, false));

  const importLines = [`import { z } from "zod";`];
  if (ctx.needsJsonValue) {
    importLines.push(`import { jsonValueSchema } from "../domain/types.js";`);
  }
  for (const [defName, module] of Object.entries(ctx.crossFile)) {
    if (!ctx.crossFileUsed.has(defName)) continue;
    importLines.push(`import { ${lowerFirst(defName)}Schema, type ${defName} } from "${module}";`);
  }

  const header = `// Code generated from spec/schema/${config.schemaFile} by sdk/typescript/scripts/gen-wire-schemas.ts; DO NOT EDIT.`;
  return `${[header, "", importLines.join("\n"), "", blocks.join("\n\n")].join("\n")}\n`;
}

// ---------- CLI entry point ----------

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const TS_ROOT = path.resolve(SCRIPT_DIR, "..");
const SCHEMA_DIR = path.resolve(TS_ROOT, "../../spec/schema");
const OUT_DIR = path.resolve(TS_ROOT, "src/generated");

export function formatWithBiome(source: string, stdinFilePath: string): string {
  return execFileSync("npx", ["biome", "format", `--stdin-file-path=${stdinFilePath}`], {
    cwd: TS_ROOT,
    input: source,
    encoding: "utf8",
  });
}

function main(): void {
  mkdirSync(OUT_DIR, { recursive: true });
  for (const config of CONFIGS) {
    const schemaJson = JSON.parse(
      readFileSync(path.join(SCHEMA_DIR, config.schemaFile), "utf8"),
    ) as SchemaObject;
    const raw = transformSchema(config, schemaJson);
    const formatted = formatWithBiome(raw, config.outFile);
    writeFileSync(path.join(OUT_DIR, config.outFile), formatted);
  }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main();
}
