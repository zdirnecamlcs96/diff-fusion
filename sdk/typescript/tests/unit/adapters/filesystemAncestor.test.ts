import { mkdir, mkdtemp, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { readFileSync } from "node:fs";
import { afterAll, describe, expect, it } from "vitest";
import {
  FilesystemAncestorStore,
  hashId,
  sanitize,
} from "../../../src/adapters/filesystemAncestor.js";
import {
  AncestorEntry,
  AncestorKey,
  AncestorStoreError,
} from "../../../src/ports/ancestor.js";

// Keep a list of temp dirs to clean up on suite exit; a test crash shouldn't
// leave ballast under $TMPDIR.
const createdDirs: string[] = [];

async function freshDir(suffix: string): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), `diff-fusion-fs-ancestor-${suffix}-`));
  createdDirs.push(dir);
  return dir;
}

afterAll(async () => {
  for (const d of createdDirs) {
    await rm(d, { recursive: true, force: true });
  }
});

// ---------------------------------------------------------------------------
// Ported Rust integration tests (tests/integration/filesystem_ancestor_tests.rs)
// ---------------------------------------------------------------------------

describe("FilesystemAncestorStore — ported Rust integration tests", () => {
  it("put_then_get_roundtrips", async () => {
    const dir = await freshDir("roundtrip");
    const store = await FilesystemAncestorStore.open(dir);

    const key = new AncestorKey("purchase_order", "PO-1");
    const entry = new AncestorEntry({ total: 100 }, 1_700_000_000_000);

    await store.put(key, entry);
    const got = await store.get(key);
    expect(got).toBeDefined();
    expect(got!.canonical).toEqual({ total: 100 });
    expect(got!.updatedAtMs).toBe(1_700_000_000_000);
  });

  it("get_returns_none_for_missing", async () => {
    const dir = await freshDir("missing");
    const store = await FilesystemAncestorStore.open(dir);

    const key = new AncestorKey("invoice", "INV-999");
    expect(await store.get(key)).toBeUndefined();
  });

  it("state_survives_reopen", async () => {
    const dir = await freshDir("reopen");
    const key = new AncestorKey("purchase_order", "PO-42");
    const entry = new AncestorEntry({ price: 50 }, 1_000);

    {
      const store = await FilesystemAncestorStore.open(dir);
      await store.put(key, entry);
    }

    // Fresh store instance pointing at the same directory.
    const reopened = await FilesystemAncestorStore.open(dir);
    const got = await reopened.get(key);
    expect(got).toBeDefined();
    expect(got!.canonical).toEqual({ price: 50 });
    expect(got!.updatedAtMs).toBe(1_000);
  });

  it("different_entity_types_do_not_collide", async () => {
    const dir = await freshDir("collision");
    const store = await FilesystemAncestorStore.open(dir);

    const k1 = new AncestorKey("purchase_order", "X");
    const k2 = new AncestorKey("invoice", "X");

    await store.put(k1, new AncestorEntry({ kind: "po" }, 1));
    await store.put(k2, new AncestorEntry({ kind: "inv" }, 2));

    expect((await store.get(k1))!.canonical).toEqual({ kind: "po" });
    expect((await store.get(k2))!.canonical).toEqual({ kind: "inv" });
  });

  it("put_overwrites_existing_entry", async () => {
    const dir = await freshDir("overwrite");
    const store = await FilesystemAncestorStore.open(dir);

    const key = new AncestorKey("item", "SKU-1");
    await store.put(key, new AncestorEntry({ v: 1 }, 1));
    await store.put(key, new AncestorEntry({ v: 2 }, 2));

    expect((await store.get(key))!.canonical).toEqual({ v: 2 });
  });

  it("ids_with_path_unsafe_chars_are_handled", async () => {
    const dir = await freshDir("unsafe-chars");
    const store = await FilesystemAncestorStore.open(dir);

    const key = new AncestorKey("thing", "customer/42:abc");
    const entry = new AncestorEntry({ ok: true }, 1);
    await store.put(key, entry);
    const got = await store.get(key);
    expect(got).toBeDefined();
    expect(got!.canonical).toEqual({ ok: true });
  });
});

// ---------------------------------------------------------------------------
// Unit tests for the filename derivation helpers.
// ---------------------------------------------------------------------------

describe("sanitize", () => {
  it("passes through alphanumerics, dash, and underscore", () => {
    expect(sanitize("Abc-123_def")).toBe("Abc-123_def");
  });

  it("replaces unsafe chars with '_'", () => {
    expect(sanitize("a/b:c d.e")).toBe("a_b_c_d_e");
  });

  it("returns '_' for the empty string", () => {
    expect(sanitize("")).toBe("_");
  });

  it("replaces unicode with '_' (ASCII-only allowlist)", () => {
    // Each code point yields one '_'; character iteration here uses
    // string iteration which yields code points, matching Rust's `chars()`.
    expect(sanitize("füñ")).toBe("f__");
  });
});

describe("hashId", () => {
  it("produces 32 lowercase hex chars", () => {
    const h = hashId("anything");
    expect(h.length).toBe(32);
    expect(/^[0-9a-f]{32}$/.test(h)).toBe(true);
  });

  it("is deterministic", () => {
    expect(hashId("PO-1")).toBe(hashId("PO-1"));
  });

  it("differs across inputs", () => {
    expect(hashId("PO-1")).not.toBe(hashId("PO-2"));
  });
});

// ---------------------------------------------------------------------------
// Cross-language filename fixtures — byte-identical to Rust.
// ---------------------------------------------------------------------------

type FilenameVector = {
  entityType: string;
  canonicalId: string;
  expectedRelPath: string;
  expectedEntityDir: string;
  expectedFile: string;
};

const fixtureUrl = new URL(
  "../../../../../spec/vectors/filesystem-filenames.json",
  import.meta.url,
);
const filenameVectors = JSON.parse(
  readFileSync(fileURLToPath(fixtureUrl), "utf8"),
) as FilenameVector[];

describe("cross-language filename vectors", () => {
  it("fixture contains at least 10 vectors", () => {
    expect(filenameVectors.length).toBeGreaterThanOrEqual(10);
  });

  for (const [i, vec] of filenameVectors.entries()) {
    it(`vector #${i} sanitize(${JSON.stringify(vec.entityType)}) matches Rust`, () => {
      expect(sanitize(vec.entityType)).toBe(vec.expectedEntityDir);
    });

    it(`vector #${i} hashId(${JSON.stringify(vec.canonicalId)}) matches Rust`, () => {
      // `expectedFile` is `<hash>.json`; extract the hash portion to compare.
      const expectedHash = vec.expectedFile.replace(/\.json$/, "");
      expect(hashId(vec.canonicalId)).toBe(expectedHash);
    });
  }

  it("pathFor() produces identical relative paths under a given root", async () => {
    const dir = await freshDir("filename-vectors");
    const store = await FilesystemAncestorStore.open(dir);

    for (const vec of filenameVectors) {
      const key = new AncestorKey(vec.entityType, vec.canonicalId);
      const abs = store.pathFor(key);
      // Strip the store root + platform separator; compare the rest against
      // the fixture's forward-slash canonical form.
      const rel = abs.slice(dir.length + 1).split(/[/\\]/).join("/");
      expect(rel).toBe(vec.expectedRelPath);
    }
  });
});

// ---------------------------------------------------------------------------
// On-disk format compatibility with the Rust runtime.
// ---------------------------------------------------------------------------

describe("on-disk JSON format", () => {
  it("uses Rust serde field names so Rust runtime can read TS writes", async () => {
    const dir = await freshDir("ondisk");
    const store = await FilesystemAncestorStore.open(dir);

    const key = new AncestorKey("purchase_order", "PO-9");
    const entry = new AncestorEntry({ total: 42 }, 1_234_567);
    await store.put(key, entry);

    const path = store.pathFor(key);
    const parsed = JSON.parse(await readFile(path, "utf8")) as {
      key: { entity_type: string; canonical_id: string };
      entry: { canonical: unknown; updated_at_ms: number };
    };

    expect(parsed.key.entity_type).toBe("purchase_order");
    expect(parsed.key.canonical_id).toBe("PO-9");
    expect(parsed.entry.canonical).toEqual({ total: 42 });
    expect(parsed.entry.updated_at_ms).toBe(1_234_567);
  });

  it("rejects a malformed on-disk payload as AncestorStoreError", async () => {
    const dir = await freshDir("ondisk-malformed");
    const store = await FilesystemAncestorStore.open(dir);

    const key = new AncestorKey("purchase_order", "PO-bad");
    const path = store.pathFor(key);

    // Missing `entry.canonical` — structurally invalid on-disk payload.
    await mkdir(dirname(path), { recursive: true });
    await writeFile(
      path,
      JSON.stringify({
        key: { entity_type: "purchase_order", canonical_id: "PO-bad" },
        entry: { updated_at_ms: 1 },
      }),
    );

    await expect(store.get(key)).rejects.toBeInstanceOf(AncestorStoreError);
  });

  it("reads a Rust-shaped on-disk payload (TS can consume Rust writes)", async () => {
    const dir = await freshDir("ondisk-read");
    const store = await FilesystemAncestorStore.open(dir);

    const key = new AncestorKey("purchase_order", "PO-7");
    const path = store.pathFor(key);

    // Hand-write the exact JSON a Rust process would have produced.
    await mkdir(dirname(path), { recursive: true });
    await writeFile(
      path,
      JSON.stringify({
        key: { entity_type: "purchase_order", canonical_id: "PO-7" },
        entry: { canonical: { shape: "rust" }, updated_at_ms: 99 },
      }),
    );

    const got = await store.get(key);
    expect(got).toBeDefined();
    expect(got!.canonical).toEqual({ shape: "rust" });
    expect(got!.updatedAtMs).toBe(99);
  });
});

// ---------------------------------------------------------------------------
// Atomicity sanity check: leftover `.json.tmp` shouldn't corrupt subsequent reads.
// ---------------------------------------------------------------------------

describe("atomic write", () => {
  it("ignores a stale sibling .tmp file on read", async () => {
    const dir = await freshDir("stale-tmp");
    const store = await FilesystemAncestorStore.open(dir);
    const key = new AncestorKey("item", "SKU-stale");
    await store.put(key, new AncestorEntry({ v: 1 }, 1));

    // Simulate a crash mid-write: drop a bogus `.json.tmp` next to the real file.
    const path = store.pathFor(key);
    await writeFile(`${path}.tmp`, "not-valid-json");

    const got = await store.get(key);
    expect(got).toBeDefined();
    expect(got!.canonical).toEqual({ v: 1 });
  });

  it("cleans the tmp file up on successful put (no .tmp siblings linger)", async () => {
    const dir = await freshDir("no-leftover");
    const store = await FilesystemAncestorStore.open(dir);
    const key = new AncestorKey("item", "SKU-clean");
    await store.put(key, new AncestorEntry({ v: 1 }, 1));

    const path = store.pathFor(key);
    const files = await readdir(dirname(path));
    // The directory should hold exactly the canonical .json, no .tmp leftovers.
    expect(files.filter((f) => f.endsWith(".tmp"))).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// Error surface: opening under an unopenable root bubbles out as
// AncestorStoreError, not a raw ENOTDIR/EACCES.
// ---------------------------------------------------------------------------

describe("error wrapping", () => {
  it("wraps an io failure on put as AncestorStoreError", async () => {
    // Put the store under a path that we immediately make a file, forcing
    // mkdir under it to fail.
    const dir = await freshDir("io-fail");
    const store = await FilesystemAncestorStore.open(dir);

    // Create a plain file where the entity-type directory wants to live.
    const entityDir = join(dir, "item");
    await writeFile(entityDir, "blocker");

    const key = new AncestorKey("item", "SKU-err");
    await expect(
      store.put(key, new AncestorEntry({}, 1)),
    ).rejects.toBeInstanceOf(AncestorStoreError);
  });
});
