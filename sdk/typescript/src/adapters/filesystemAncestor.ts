/**
 * Filesystem-backed {@link AncestorStore}.
 *
 * Stores each ancestor as a single JSON file under a root directory:
 *
 * ```text
 * <root>/
 * ├─ <sanitized_entity_type>/
 * │  ├─ <blake3(canonical_id)[0..16]-hex>.json
 * │  └─ ...
 * └─ ...
 * ```
 *
 * `canonicalId` is hashed (BLAKE3, first 16 bytes → 32 lowercase hex chars) to
 * produce the filename so arbitrary user-supplied strings (slashes, colons,
 * unicode) can't escape the root or collide with filesystem semantics. The
 * file's contents carry the original key, the canonical value, and the
 * updated-at timestamp.
 *
 * # Cross-runtime compatibility (plan §6)
 *
 * Filename derivation — `sanitize(entityType)` + BLAKE3 hex truncation — is
 * byte-identical to the Rust implementation. On-disk JSON uses the Rust
 * serde field names (`entity_type`, `canonical_id`, `canonical`,
 * `updated_at_ms`) so an ancestor written by the Rust runtime is readable by
 * the TS runtime and vice versa. Verified by cross-language fixtures in
 * `spec/vectors/filesystem-filenames.json`.
 *
 * # Atomicity
 *
 * Writes go to a `.json.tmp` sibling, `fsync`, then `rename` into place so an
 * interrupted write can't leave a half-written ancestor on disk. `fs.rename`
 * is atomic on POSIX. On Windows it's atomic within a single volume but NOT
 * across volumes — callers on Windows should ensure the store root and
 * system temp live on the same volume, or accept that a crash mid-rename may
 * require manual cleanup of leftover `.json.tmp` files.
 */

import { blake3 } from "@noble/hashes/blake3";
import { mkdir, readFile, rename, rm, writeFile, open } from "node:fs/promises";
import { dirname, join } from "node:path";
import { z } from "zod";
import { jsonValueSchema } from "../domain/types.js";
import {
  AncestorEntry,
  type AncestorKey,
  AncestorStoreError,
  type AncestorStore,
} from "../ports/ancestor.js";

export class FilesystemAncestorStore implements AncestorStore {
  private readonly root: string;

  private constructor(root: string) {
    this.root = root;
  }

  /** Open a store rooted at `root`. Creates the directory if missing. */
  static async open(root: string): Promise<FilesystemAncestorStore> {
    try {
      await mkdir(root, { recursive: true });
    } catch (e) {
      throw wrapIo(e);
    }
    return new FilesystemAncestorStore(root);
  }

  /**
   * Absolute path where the entry for `key` is (or will be) stored. Exposed
   * so tests and cross-runtime checks can assert layout compatibility.
   */
  pathFor(key: AncestorKey): string {
    const entityDir = join(this.root, sanitize(key.entityType));
    const file = `${hashId(key.canonicalId)}.json`;
    return join(entityDir, file);
  }

  async get(key: AncestorKey): Promise<AncestorEntry | undefined> {
    const path = this.pathFor(key);
    let bytes: Buffer;
    try {
      bytes = await readFile(path);
    } catch (e) {
      if (isNotFound(e)) return undefined;
      throw wrapIo(e);
    }
    let onDisk: OnDisk;
    try {
      onDisk = onDiskSchema.parse(JSON.parse(bytes.toString("utf8")));
    } catch (e) {
      throw wrapSerde(e);
    }
    return new AncestorEntry(onDisk.entry.canonical, onDisk.entry.updated_at_ms);
  }

  async put(key: AncestorKey, entry: AncestorEntry): Promise<void> {
    const path = this.pathFor(key);
    await ensureParent(path);

    const payload: OnDisk = {
      key: {
        entity_type: key.entityType,
        canonical_id: key.canonicalId,
      },
      entry: {
        canonical: entry.canonical,
        updated_at_ms: entry.updatedAtMs,
      },
    };

    let bytes: string;
    try {
      bytes = JSON.stringify(payload);
    } catch (e) {
      throw wrapSerde(e);
    }

    // Atomic write: write to a sibling `.json.tmp`, fsync, then rename.
    // `rename` is atomic on POSIX; on Windows this is best-effort within a
    // single volume (see module doc).
    const tmp = `${path}.tmp`;
    try {
      await writeFile(tmp, bytes);
      // fsync so the rename can't expose a truncated file on power loss.
      const handle = await open(tmp, "r+");
      try {
        await handle.sync();
      } finally {
        await handle.close();
      }
      await rename(tmp, path);
    } catch (e) {
      // Best-effort cleanup of the temp file; ignore secondary failures.
      try {
        await rm(tmp, { force: true });
      } catch {
        // swallow — primary error below is what the caller cares about
      }
      throw wrapIo(e);
    }
  }
}

// ---------------------------------------------------------------------------
// Internal helpers — exported for cross-language fixture tests.
// ---------------------------------------------------------------------------

/**
 * Mirrors Rust `sanitize()`: allow `[A-Za-z0-9_-]`, replace anything else
 * with `_`, and return `"_"` for an empty string. Exposed for the
 * cross-language filename fixture test.
 */
export function sanitize(raw: string): string {
  let out = "";
  for (const ch of raw) {
    if (isAsciiAlnumOrDashUnderscore(ch)) {
      out += ch;
    } else {
      out += "_";
    }
  }
  if (out.length === 0) return "_";
  return out;
}

/**
 * Mirrors Rust `hash_id()`: BLAKE3 of the UTF-8 bytes, first 16 bytes
 * lowercase-hex-encoded. Exposed for the cross-language filename fixture
 * test.
 */
export function hashId(raw: string): string {
  const digest = blake3(new TextEncoder().encode(raw));
  let out = "";
  for (let i = 0; i < 16; i++) {
    const b = digest[i]!;
    out += HEX[(b >> 4) & 0xf];
    out += HEX[b & 0xf];
  }
  return out;
}

const HEX = "0123456789abcdef";

function isAsciiAlnumOrDashUnderscore(ch: string): boolean {
  if (ch.length !== 1) return false;
  const c = ch.charCodeAt(0);
  return (
    (c >= 0x30 && c <= 0x39) || // 0-9
    (c >= 0x41 && c <= 0x5a) || // A-Z
    (c >= 0x61 && c <= 0x7a) || // a-z
    c === 0x2d || // -
    c === 0x5f // _
  );
}

const onDiskSchema = z.object({
  key: z.object({ entity_type: z.string(), canonical_id: z.string() }),
  entry: z.object({ canonical: jsonValueSchema, updated_at_ms: z.number() }),
});
type OnDisk = z.infer<typeof onDiskSchema>;

async function ensureParent(path: string): Promise<void> {
  const parent = dirname(path);
  try {
    await mkdir(parent, { recursive: true });
  } catch (e) {
    throw wrapIo(e);
  }
}

function isNotFound(e: unknown): boolean {
  return (
    typeof e === "object" &&
    e !== null &&
    "code" in e &&
    (e as { code: unknown }).code === "ENOENT"
  );
}

function wrapIo(e: unknown): AncestorStoreError {
  const msg = e instanceof Error ? e.message : String(e);
  return new AncestorStoreError(`io: ${msg}`);
}

function wrapSerde(e: unknown): AncestorStoreError {
  const msg = e instanceof Error ? e.message : String(e);
  return new AncestorStoreError(`serde: ${msg}`);
}
