import { describe, expect, it } from "vitest";
import {
  build,
  buildPolicyDocument,
  parsePolicyDocument,
  type MergePolicyRef,
  type PolicyDocument,
  type TransitionRef,
} from "../../../../src/application/policy/declaration.js";
import {
  MergeContext,
  PolicyMap,
  resolve,
} from "../../../../src/application/policy/index.js";
import { SetByKey } from "../../../../src/application/policy/structural.js";
import { threeWayDiff } from "../../../../src/domain/diff/threeWay.js";
import type { JsonValue } from "../../../../src/domain/types.js";

describe("application/policy/declaration", () => {
  it("owned_by roundtrips through JSON", () => {
    const decl: MergePolicyRef = { kind: "owned_by", system: "netsuite" };
    const j = JSON.parse(JSON.stringify(decl));
    expect(j).toEqual({ kind: "owned_by", system: "netsuite" });

    const parsed = j as MergePolicyRef;
    expect(parsed).toEqual(decl);
  });

  it("state_machine roundtrips through JSON", () => {
    const decl: MergePolicyRef = {
      kind: "state_machine",
      transitions: [
        { from: "draft", to: "open" },
        { from: "open", to: "closed" },
      ],
    };
    const j = JSON.parse(JSON.stringify(decl));
    const parsed = j as MergePolicyRef;
    expect(parsed).toEqual(decl);
  });

  it("declarations drive resolution", () => {
    const decls = new Map<string, MergePolicyRef>();
    decls.set("price", { kind: "owned_by", system: "pricing" });
    decls.set("qty", { kind: "additive" });

    const policies = new PolicyMap();
    for (const [path, decl] of decls) {
      policies.with(path, build(decl));
    }

    const anc: JsonValue = { price: 10, qty: 5 };
    const a: JsonValue = { price: 15, qty: 6 };
    const b: JsonValue = { price: 99, qty: 7 };
    const log = threeWayDiff(anc, a, b);

    const ctx = new MergeContext("pricing", "ops");
    const r = resolve(log, policies, ctx);

    expect(r.conflicts).toEqual([]);
    expect(r.resolved.get("price")).toBe(15);
    expect(r.resolved.get("qty")).toBe(8);
  });

  it("additive declaration builds a functional policy", () => {
    const decl: MergePolicyRef = { kind: "additive" };
    const policy = build(decl);
    expect(policy.name()).toBe("additive");

    const anc: JsonValue = { n: 10 };
    const a: JsonValue = { n: 12 };
    const b: JsonValue = { n: 11 };
    const log = threeWayDiff(anc, a, b);
    const ctx = new MergeContext("a", "b");

    const outcome = policy.merge(log.changes[0]!, ctx);
    expect(outcome.kind).toBe("Resolved");
    if (outcome.kind === "Resolved") {
      expect(outcome.value).toBe(13);
    }
  });

  it("append declaration builds a functional policy", () => {
    const decl: MergePolicyRef = { kind: "append" };
    const policy = build(decl);
    expect(policy.name()).toBe("append");
  });

  it("state_machine declaration builds a functional policy with transitions", () => {
    const transitions: TransitionRef[] = [{ from: "draft", to: "open" }];
    const decl: MergePolicyRef = { kind: "state_machine", transitions };
    const policy = build(decl);
    expect(policy.name()).toBe("state_machine");

    const anc: JsonValue = { s: "draft" };
    const a: JsonValue = { s: "open" };
    const log = threeWayDiff(anc, a, anc);
    const ctx = new MergeContext("a", "b");
    const outcome = policy.merge(log.changes[0]!, ctx);
    expect(outcome.kind).toBe("Resolved");
    if (outcome.kind === "Resolved") {
      expect(outcome.value).toBe("open");
    }
  });

  it("policy document roundtrips through JSON", () => {
    const doc = parsePolicyDocument({
      fields: {
        price: { kind: "owned_by", system: "netsuite" },
        notes: { kind: "append" },
      },
      default: { kind: "additive" },
    });

    expect(Object.keys(doc.fields)).toHaveLength(2);
    expect(doc.default).toEqual({ kind: "additive" });

    const j = JSON.parse(JSON.stringify(doc));
    const parsed = parsePolicyDocument(j);
    expect(parsed).toEqual(doc);
  });

  it("policy document defaults to empty fields and no default", () => {
    const doc = parsePolicyDocument({});
    expect(doc.fields).toEqual({});
    expect(doc.default).toBeUndefined();
    expect(JSON.stringify(doc)).toBe('{"fields":{}}');
  });

  it("policy document build drives resolution", () => {
    const doc = parsePolicyDocument({
      fields: { price: { kind: "owned_by", system: "pricing" } },
      default: { kind: "additive" },
    });
    const policies = buildPolicyDocument(doc);

    const anc: JsonValue = { price: 10, qty: 5 };
    const a: JsonValue = { price: 15, qty: 6 };
    const b: JsonValue = { price: 99, qty: 7 };
    const log = threeWayDiff(anc, a, b);
    const ctx = new MergeContext("pricing", "ops");

    const r = resolve(log, policies, ctx);
    expect(r.conflicts).toEqual([]);
    expect(r.resolved.get("price")).toBe(15);
    expect(r.resolved.get("qty")).toBe(8);
  });

  it("policy document build() resolves all six MergePolicyRef kinds", () => {
    const doc: PolicyDocument = {
      fields: {
        price: { kind: "owned_by", system: "netsuite" },
        qty: { kind: "additive" },
        notes: { kind: "append" },
        status: {
          kind: "state_machine",
          transitions: [{ from: "draft", to: "open" }],
        },
        escalated: {
          kind: "last_write_wins",
          reason: "vendor feed wins",
          timestamp_a: 100,
          timestamp_b: 50,
        },
        lineItems: new SetByKey(["sku"], "warehouse", "channel").toRef(),
      },
    };
    const policies = buildPolicyDocument(doc);

    expect(policies.lookup("price")!.name()).toBe("owned_by");
    expect(policies.lookup("qty")!.name()).toBe("additive");
    expect(policies.lookup("notes")!.name()).toBe("append");
    expect(policies.lookup("status")!.name()).toBe("state_machine");
    expect(policies.lookup("escalated")!.name()).toBe("last_write_wins");
    expect(policies.lookup("lineItems")!.name()).toBe("set_by_key");
  });

  it("build() applies SetByKey's full config from a declaration, including recursive nested", () => {
    const doc = parsePolicyDocument({
      fields: {
        groups: {
          kind: "set_by_key",
          identity: ["gid"],
          a_anchor: "gid",
          b_anchor: "gid",
          on_both_changed: "Union",
          nested: {
            items: { identity: ["sku"], a_anchor: "sku", b_anchor: "sku" },
          },
        },
      },
    });
    const policies = buildPolicyDocument(doc);
    const policy = policies.lookup("groups") as SetByKey;
    expect(policy.onBothChanged).toBe("Union");
    expect(policy.nested.get("items")).toBeInstanceOf(SetByKey);

    const anc: JsonValue = {
      groups: [{ gid: "G1", items: [{ sku: "A", q: 1 }] }],
    };
    const a: JsonValue = {
      groups: [{ gid: "G1", items: [{ sku: "A", q: 1 }, { sku: "B", q: 2 }] }],
    };
    const b: JsonValue = {
      groups: [{ gid: "G1", items: [{ sku: "A", q: 1 }, { sku: "C", q: 3 }] }],
    };
    const log = threeWayDiff(anc, a, b);
    const ctx = new MergeContext("a", "b");
    const outcome = policy.merge(log.changes[0]!, ctx);
    expect(outcome.kind).toBe("Resolved");
    if (outcome.kind !== "Resolved") throw new Error("expected Resolved");
    const arr = outcome.value as Array<{ items: Array<{ sku: string }> }>;
    expect(arr).toHaveLength(1);
    expect(arr[0]!.items.map((i) => i.sku).sort()).toEqual(["A", "B", "C"]);
  });

  it("policy document knob fidelity: parse(JSON(x)) -> build -> toRef equals x for non-default SetByKey knobs", () => {
    const policy = new SetByKey(["sku"], "warehouse", "channel");
    policy.onAddedInA = "Exclude";
    policy.onRemovedInB = "Remove";
    policy.onBothChanged = "PreferB";
    policy.preferAOnFieldConflict = false;
    policy.nested.set("subLines", new SetByKey(["lineSku"], "subA", "subB"));
    const ref = policy.toRef();

    const doc: PolicyDocument = { fields: { lineItems: ref } };
    const json = JSON.parse(JSON.stringify(doc));
    const parsed = parsePolicyDocument(json);
    const policies = buildPolicyDocument(parsed);

    expect((policies.lookup("lineItems") as SetByKey).toRef()).toEqual(ref);
  });

  it("parsePolicyDocument rejects a malformed document", () => {
    expect(() => parsePolicyDocument("nope")).toThrow();
    expect(() => parsePolicyDocument({ fields: "nope" })).toThrow();
    expect(() =>
      buildPolicyDocument({ fields: { x: { kind: "bogus" } as unknown as MergePolicyRef } }),
    ).toThrow();
  });

  it("parsePolicyDocument rejects a bad kind at parse time, not just at build()", () => {
    expect(() =>
      parsePolicyDocument({ fields: { x: { kind: "bogus" } } }),
    ).toThrow();
  });
});
