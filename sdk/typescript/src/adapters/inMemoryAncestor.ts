/**
 * In-memory `AncestorStore` — reference impl used by tests and the default
 * `SyncEngine` configuration.
 *
 * Not durable. Real deployments implement the interface against filesystem,
 * Postgres, Redis, etc. This one exists so nothing blocks on a storage
 * decision before the hot path is proven.
 *
 * Node's event loop is single-threaded, so no lock is needed around the
 * backing map — the Rust `Mutex<HashMap>` was load-bearing for thread
 * safety, not for cycle ordering. Methods are still `async` per plan §6 so
 * the interface can host Postgres/Redis-backed adapters later without
 * rippling through callers.
 */

import {
  type AncestorEntry,
  type AncestorKey,
  type AncestorStore,
} from "../ports/ancestor.js";

export class InMemoryAncestorStore implements AncestorStore {
  // Keys are composed from (entityType, canonicalId). A NUL separator keeps
  // distinct `(a, "b\0c")` / `(a\0b, "c")` pairs from colliding — neither
  // entity types nor canonical ids are expected to contain control chars.
  private readonly entries = new Map<string, AncestorEntry>();

  async get(key: AncestorKey): Promise<AncestorEntry | undefined> {
    return this.entries.get(composite(key));
  }

  async put(key: AncestorKey, entry: AncestorEntry): Promise<void> {
    this.entries.set(composite(key), entry);
  }

}

function composite(key: AncestorKey): string {
  return `${key.entityType}\x00${key.canonicalId}`;
}
