/**
 * Shared contract test suite for every `SystemPort` adapter.
 *
 * An adapter is "done" when it passes every check in {@link runContractSuite}.
 * No judgement calls — the suite is the forcing function that keeps per-adapter
 * implementations honest about the port's invariants.
 *
 * New adapters reuse this file by adding a tiny `.contract.test.ts` driver
 * that calls `runContractSuite(() => new MyAdapter(...))`.
 *
 * Port of Rust `tests/integration/contract_tests.rs`.
 */

import { describe, expect, it } from "vitest";
import { SyncError } from "../../src/domain/error.js";
import { idempotencyKey } from "../../src/domain/idempotency.js";
import type { SystemPort } from "../../src/ports/system.js";

/**
 * Run every behavioural check against an adapter.
 *
 * The factory is called fresh for every `it()` so checks cannot observe each
 * other's state. This mirrors the Rust suite's habit of reseeding per test.
 */
export function runContractSuite(
  label: string,
  makeAdapter: () => SystemPort,
): void {
  describe(`SystemPort contract: ${label}`, () => {
    it("find_by_canonical_id returns undefined for missing entity", async () => {
      const port = makeAdapter();
      const r = await port.findByCanonicalId(
        "contract_entity",
        "contract_id_missing",
      );
      expect(r).toBeUndefined();
    });

    it("upsert then find returns the ref created by upsert", async () => {
      const port = makeAdapter();
      const payload = { contract_field: "found" };
      const k = idempotencyKey("contract_id_1", "upsert", payload);
      const inserted = await port.upsert(
        "contract_entity",
        "contract_id_1",
        payload,
        undefined,
        k,
      );
      expect(inserted.system).toBe(port.systemType());

      const found = await port.findByCanonicalId(
        "contract_entity",
        "contract_id_1",
      );
      expect(found).toBeDefined();
      expect(found!.externalId).toBe(inserted.externalId);
    });

    // C2 (stale expectVersion -> StaleWrite) and C3 (idempotency-key replay
    // is a no-op) now live in ports/conformance.ts — see that module's harness.

    it("fetch round-trips the canonical payload", async () => {
      const port = makeAdapter();
      const payload = { x: 42 };
      const k = idempotencyKey("contract_id_fetch", "upsert", payload);
      const r = await port.upsert(
        "contract_entity",
        "contract_id_fetch",
        payload,
        undefined,
        k,
      );
      const { canonical } = await port.fetch("contract_entity", r);
      expect(canonical).toEqual(payload);
    });

    it("expectVersion on a missing record is a stale write", async () => {
      const port = makeAdapter();
      const payload = {};
      const k = idempotencyKey("contract_id_no_record", "upsert", payload);
      await expect(
        port.upsert(
          "contract_entity",
          "contract_id_no_record",
          payload,
          "1",
          k,
        ),
      ).rejects.toSatisfy(
        (e) => e instanceof SyncError && e.kind === "StaleWrite",
      );
    });
  });
}
