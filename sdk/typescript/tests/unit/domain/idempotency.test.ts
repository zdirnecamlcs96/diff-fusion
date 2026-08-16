import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import fc from "fast-check";
import { describe, expect, it } from "vitest";
import {
  canonicalStringify,
  idempotencyKey,
  idempotencyKeyHex,
} from "../../../src/domain/idempotency.js";
import type { JsonValue } from "../../../src/domain/types.js";

// ---------------------------------------------------------------------------
// Ported Rust #[test] blocks (src/domain/idempotency.rs)
// ---------------------------------------------------------------------------

describe("idempotencyKey — ported Rust tests", () => {
  it("deterministic_for_same_inputs", () => {
    const p: JsonValue = { qty: 5, sku: "A1" };
    const k1 = idempotencyKey("PO-1", "upsert", p);
    const k2 = idempotencyKey("PO-1", "upsert", p);
    expect(k1).toEqual(k2);
  });

  it("key_order_in_payload_does_not_matter", () => {
    const p1: JsonValue = { a: 1, b: 2 };
    const p2: JsonValue = { b: 2, a: 1 };
    expect(idempotencyKey("ID", "upsert", p1)).toEqual(
      idempotencyKey("ID", "upsert", p2),
    );
  });

  it("different_canonical_id_changes_key", () => {
    const p: JsonValue = { x: 1 };
    expect(idempotencyKey("PO-1", "upsert", p)).not.toEqual(
      idempotencyKey("PO-2", "upsert", p),
    );
  });

  it("different_operation_changes_key", () => {
    const p: JsonValue = { x: 1 };
    expect(idempotencyKey("PO-1", "upsert", p)).not.toEqual(
      idempotencyKey("PO-1", "delete", p),
    );
  });

  it("different_payload_changes_key", () => {
    expect(idempotencyKey("PO-1", "upsert", { x: 1 })).not.toEqual(
      idempotencyKey("PO-1", "upsert", { x: 2 }),
    );
  });

  it("length_prefix_prevents_boundary_collision", () => {
    // Without length-prefixing, ("a", "bc", ...) and ("ab", "c", ...)
    // could hash the same if fields were simply concatenated.
    const p: JsonValue = null;
    expect(idempotencyKey("a", "bc", p)).not.toEqual(idempotencyKey("ab", "c", p));
  });

  it("hex_form_is_64_chars_lowercase", () => {
    const h = idempotencyKeyHex("PO-1", "upsert", {});
    expect(h.length).toBe(64);
    expect(/^[0-9a-f]{64}$/.test(h)).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// canonicalStringify invariants
// ---------------------------------------------------------------------------

describe("canonicalStringify", () => {
  it("sorts object keys lexicographically", () => {
    expect(canonicalStringify({ b: 2, a: 1 })).toBe('{"a":1,"b":2}');
  });

  it("preserves array order", () => {
    expect(canonicalStringify([3, 1, 2])).toBe("[3,1,2]");
  });

  it("recurses into nested objects and arrays", () => {
    expect(
      canonicalStringify({ z: [{ b: 2, a: 1 }], y: { d: 4, c: 3 } }),
    ).toBe('{"y":{"c":3,"d":4},"z":[{"a":1,"b":2}]}');
  });

  it("emits null, booleans, and empty structures identically to Rust", () => {
    expect(canonicalStringify(null)).toBe("null");
    expect(canonicalStringify(true)).toBe("true");
    expect(canonicalStringify(false)).toBe("false");
    expect(canonicalStringify({})).toBe("{}");
    expect(canonicalStringify([])).toBe("[]");
  });
});

// ---------------------------------------------------------------------------
// Property tests
// ---------------------------------------------------------------------------

describe("property tests", () => {
  const scalar: fc.Arbitrary<JsonValue> = fc.oneof(
    fc.constant(null),
    fc.boolean(),
    fc.integer({ min: -1_000_000, max: 1_000_000 }),
    fc.string({ maxLength: 10 }),
  );

  const jsonValue: fc.Arbitrary<JsonValue> = fc.letrec<{
    tree: JsonValue;
  }>((tie) => ({
    tree: fc.oneof(
      { maxDepth: 3, depthIdentifier: "json" },
      scalar,
      fc.array(tie("tree"), { maxLength: 4 }),
      fc.dictionary(fc.string({ maxLength: 6 }), tie("tree"), {
        maxKeys: 4,
      }) as fc.Arbitrary<JsonValue>,
    ),
  })).tree;

  // Deep-clone helper that shuffles object key insertion order without altering
  // semantic value — used to prove canonicalStringify is permutation-invariant.
  function permuteKeys(v: JsonValue, rng: () => number): JsonValue {
    if (v === null || typeof v !== "number" && typeof v !== "object") {
      return v;
    }
    if (Array.isArray(v)) {
      return v.map((x) => permuteKeys(x, rng));
    }
    if (typeof v === "object") {
      const keys = Object.keys(v);
      // Fisher–Yates shuffle driven by the provided PRNG.
      for (let i = keys.length - 1; i > 0; i--) {
        const j = Math.floor(rng() * (i + 1));
        [keys[i], keys[j]] = [keys[j]!, keys[i]!];
      }
      const out: { [k: string]: JsonValue } = {};
      for (const k of keys) {
        out[k] = permuteKeys(v[k] as JsonValue, rng);
      }
      return out;
    }
    return v;
  }

  it("canonicalStringify is invariant under object-key permutation", () => {
    fc.assert(
      fc.property(jsonValue, fc.integer({ min: 0, max: 2 ** 31 - 1 }), (v, seed) => {
        // Mulberry32 PRNG so the shuffle is deterministic per example.
        let state = seed >>> 0;
        const rng = () => {
          state = (state + 0x6d2b79f5) >>> 0;
          let t = state;
          t = Math.imul(t ^ (t >>> 15), t | 1);
          t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
          return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
        };
        const permuted = permuteKeys(v, rng);
        expect(canonicalStringify(permuted)).toBe(canonicalStringify(v));
      }),
      { numRuns: 200 },
    );
  });

  it("idempotencyKey is deterministic for identical inputs", () => {
    fc.assert(
      fc.property(
        fc.string({ minLength: 1, maxLength: 16 }),
        fc.string({ minLength: 1, maxLength: 10 }),
        jsonValue,
        (id, op, payload) => {
          const k1 = idempotencyKey(id, op, payload);
          const k2 = idempotencyKey(id, op, payload);
          expect(k1).toEqual(k2);
        },
      ),
      { numRuns: 200 },
    );
  });

  it("idempotencyKey is invariant under payload object-key permutation", () => {
    fc.assert(
      fc.property(
        fc.string({ minLength: 1, maxLength: 16 }),
        fc.string({ minLength: 1, maxLength: 10 }),
        jsonValue,
        fc.integer({ min: 0, max: 2 ** 31 - 1 }),
        (id, op, payload, seed) => {
          let state = seed >>> 0;
          const rng = () => {
            state = (state + 0x6d2b79f5) >>> 0;
            let t = state;
            t = Math.imul(t ^ (t >>> 15), t | 1);
            t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
            return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
          };
          const permuted = permuteKeys(payload, rng);
          expect(idempotencyKey(id, op, permuted)).toEqual(
            idempotencyKey(id, op, payload),
          );
        },
      ),
      { numRuns: 200 },
    );
  });
});

// ---------------------------------------------------------------------------
// Cross-language golden vectors (Phase 9 slice)
//
// These vectors were generated by the Rust implementation via
// `cargo run --example gen_idempotency_vectors`. Each TS output MUST match
// byte-for-byte — divergence means the two runtimes will mint different keys
// for the same push, which silently breaks replay safety.
// ---------------------------------------------------------------------------

type Vector = {
  canonicalId: string;
  operation: string;
  payload: JsonValue;
  canonicalPayloadJson: string;
  expectedHex: string;
};

const fixtureUrl = new URL(
  "../../../../../spec/vectors/idempotency-vectors.json",
  import.meta.url,
);
const vectors = JSON.parse(readFileSync(fileURLToPath(fixtureUrl), "utf8")) as Vector[];

describe("cross-language vectors", () => {
  it("fixture contains the expected number of vectors", () => {
    // Keep the count honest — if someone regenerates fewer vectors by mistake,
    // this test flags it.
    expect(vectors.length).toBeGreaterThanOrEqual(10);
  });

  for (const [i, vec] of vectors.entries()) {
    it(`vector #${i} [${vec.canonicalId} / ${vec.operation}] matches Rust hex`, () => {
      expect(idempotencyKeyHex(vec.canonicalId, vec.operation, vec.payload)).toBe(
        vec.expectedHex,
      );
    });

    it(`vector #${i} canonical payload JSON matches Rust serialisation`, () => {
      expect(canonicalStringify(vec.payload)).toBe(vec.canonicalPayloadJson);
    });
  }
});
