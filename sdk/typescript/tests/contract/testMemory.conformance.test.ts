/**
 * Driver: run the shared SystemPort conformance harness against
 * {@link TestMemoryAdapter}.
 *
 * Ports `core/src/adapters/test_memory.rs`'s
 * `passes_the_system_port_conformance_harness` test.
 */

import { describe, it } from "vitest";
import { TestMemoryAdapter } from "../../src/adapters/testMemory.js";
import type { JsonValue } from "../../src/domain/types.js";
import { type RawAccess, assertSystemPortContract } from "../../src/ports/conformance.js";
import type { ExternalRef } from "../../src/ports/system.js";

// `seed()` already bypasses upsert; `fetch()` + `findByCanonicalId()`
// already return the raw stored value verbatim (this adapter doesn't do
// CIF↔native translation), so both sides of `RawAccess` fall straight
// through to existing SystemPort methods.
class RawView implements RawAccess {
  constructor(private readonly adapter: TestMemoryAdapter) {}

  async seedRaw(entityType: string, canonicalId: string, native: JsonValue): Promise<ExternalRef> {
    return this.adapter.seed(entityType, canonicalId, native);
  }

  async readRaw(entityType: string, canonicalId: string): Promise<JsonValue> {
    const ref = await this.adapter.findByCanonicalId(entityType, canonicalId);
    if (ref === undefined) {
      throw new Error("seedRaw must have created a findable record");
    }
    const { canonical } = await this.adapter.fetch(entityType, ref);
    return canonical;
  }
}

describe("SystemPort conformance: TestMemoryAdapter", () => {
  it("passes the conformance harness", async () => {
    const adapter = new TestMemoryAdapter("sys_a");
    await assertSystemPortContract(adapter, new RawView(adapter));
  });
});
