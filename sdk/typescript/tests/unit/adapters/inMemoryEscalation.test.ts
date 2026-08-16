import { describe, expect, it } from "vitest";
import type {
  FieldChange,
} from "../../../src/domain/diff/threeWay.js";
import type {
  UnresolvedConflict,
} from "../../../src/application/policy/index.js";
import { EscalationItem } from "../../../src/ports/escalation.js";
import { InMemoryEscalationQueue } from "../../../src/adapters/inMemoryEscalation.js";

function item(path: string): EscalationItem {
  const change: FieldChange = {
    path,
    oldValue: 0,
    newFromA: 1,
    newFromB: 2,
    source: "both",
  };
  const conflict: UnresolvedConflict = {
    path,
    reason: "divergent",
    class: "PolicyConflict",
    change,
  };
  return new EscalationItem("purchase_order", "PO-1", [conflict], 1);
}

describe("push_appends_items", () => {
  it("len grows and snapshot preserves order", async () => {
    const q = new InMemoryEscalationQueue();
    expect(await q.len()).toBe(0);
    expect(await q.isEmpty()).toBe(true);

    await q.push(item("price"));
    await q.push(item("qty"));

    expect(await q.len()).toBe(2);
    expect(await q.isEmpty()).toBe(false);
    const snap = q.snapshot();
    expect(snap).toHaveLength(2);
    expect(snap[0]?.conflicts[0]?.path).toBe("price");
    expect(snap[1]?.conflicts[0]?.path).toBe("qty");
  });
});

describe("snapshot is a defensive copy", () => {
  it("mutating the snapshot does not affect the queue", async () => {
    const q = new InMemoryEscalationQueue();
    await q.push(item("price"));
    const snap = q.snapshot();
    // Attempt to tamper — should not reflect back into the queue.
    (snap as unknown as EscalationItem[]).push(item("bogus"));
    expect(await q.len()).toBe(1);
  });
});

describe("isEmpty", () => {
  it("is true for a fresh queue and false after push", async () => {
    const q = new InMemoryEscalationQueue();
    expect(await q.isEmpty()).toBe(true);
    await q.push(item("any"));
    expect(await q.isEmpty()).toBe(false);
  });
});
