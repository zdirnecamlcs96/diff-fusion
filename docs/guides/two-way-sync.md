---
layout: default
title: Two-way sync walkthrough
parent: Guides
nav_order: 1
---

# Two-way sync walkthrough

This walks through `core/examples/two_way_sync.rs` end to end — two systems, three fields, three different merge policies, a preview, a real sync, and a replay. Run it yourself:

```bash
cd core/
cargo run --example two_way_sync
```

The TypeScript mirror lives at `sdk/typescript/examples/twoWaySync.ts`:

```bash
cd sdk/typescript/
npx tsx examples/twoWaySync.ts
```

## Set up two systems and seed divergent state

The example uses `TestMemoryAdapter` (`core/src/adapters/test_memory.rs`) for both sides — no network, but a real `SystemPort` implementation with optimistic concurrency and idempotency dedup. It seeds both sides with the same starting document, then simulates independent drift on each:

```rust
let erp = TestMemoryAdapter::new("erp");
let inv = TestMemoryAdapter::new("inv");

let starting = json!({
    "price": 100,
    "qty_recv": 5,
    "status": "open",
});
erp.seed(ENTITY, PO_ID, starting.clone());
inv.seed(ENTITY, PO_ID, starting.clone());

// Simulate drift on both sides.
update(&erp, json!({ "price": 120, "qty_recv": 6, "status": "closed" })).await;
update(&inv, json!({ "price": 999, "qty_recv": 7, "status": "closed" })).await;
```

After this, `erp` and `inv` disagree on `price` and `qty_recv`, but agree on `status` (both moved `open → closed`). This is the shape a three-way diff needs: an ancestor (`starting`) plus two current views.

## Install a policy per field

The `SyncEngine::builder` (`core/src/drivers/sync_engine.rs`) takes both adapters and installs one merge policy per path:

```rust
let engine = SyncEngine::builder(erp.clone(), inv.clone())
    .policy("price", Box::new(OwnedBy::new("erp")))
    .policy("qty_recv", Box::new(Additive))
    .policy(
        "status",
        Box::new(StateMachine::new([
            StateTransition::new("open", "closed"),
            StateTransition::new("open", "cancelled"),
        ])),
    )
    .seed_ancestor(ENTITY, PO_ID, starting.clone())
    .build();
```

Why each policy fits the field it's attached to (see [Concepts]({{ site.baseurl }}/concepts) for the merge policy tier vocabulary):

- **`price` → `OwnedBy::new("erp")`** (`core/src/application/policy/owned_by.rs`). Price is a financial fact that should have one source of truth. The ERP is authoritative; `inv`'s change to `999` is discarded rather than merged. Per that module's doc comment, owner-based policies are the single most effective conflict-reduction strategy — they eliminate the majority of conflicts before they can even arise, because a non-owner's edit is never a candidate for "correct" in the first place.
- **`qty_recv` → `Additive`** (`core/src/application/policy/additive.rs`). Quantity received is a counter — `erp` recorded some units in, `inv` recorded others, and the correct merged value is `ancestor + delta_erp + delta_inv`, not "pick a winner". `Additive` is only defined for numeric fields; non-numeric input becomes a conflict rather than a guess.
- **`status` → `StateMachine`** (`core/src/application/policy/state_machine.rs`). Status is an enum with legal transitions. Declaring `open → closed` and `open → cancelled` as the only allowed moves means a corrupt transition (e.g. `closed → open`) can never silently take effect. Here both sides made the *same* legal transition, so the policy resolves cleanly; if they'd diverged onto different branches, it would escalate rather than guess which branch is "right".

## Preview before writing

`SyncEngine::preview` (`ShadowReport`) computes the same three-way diff and policy resolution as a real sync, but writes nothing and doesn't advance the ancestor:

```rust
let preview = engine.preview(ENTITY, PO_ID).await.unwrap();
println!("PREVIEW (no writes):");
if let Some(w) = &preview.would_write {
    println!("  would write: {w}");
} else {
    let conflicts = &preview.resolution.conflicts;
    println!("  would escalate — {} conflicts", conflicts.len());
    for c in conflicts {
        println!("    · {} — {}", c.path, c.reason);
    }
}
```

This is the dry-run mode: safe to call as often as you like, useful for validating a new adapter or a new policy set before it touches real data.

## Sync, and read the outcome

`SyncEngine::sync` runs the real cycle — merge, push to whichever side(s) are stale, then advance the ancestor last (only after pushes confirm). It returns a `CycleOutcome`:

```rust
let outcome = engine.sync(ENTITY, PO_ID).await.unwrap();
match &outcome {
    CycleOutcome::NoOp => println!("Nothing to do."),
    CycleOutcome::Synced { pushed_to } => {
        println!("Synced. Pushed to: {pushed_to:?}");
    }
    CycleOutcome::Escalated { conflicts } => {
        println!("Escalated — {} conflict(s) queued:", conflicts.len());
        for c in conflicts {
            println!("  · {} — {}", c.path, c.reason);
        }
    }
}
```

For this example, resolution is clean on all three fields, so `outcome` is `Synced { pushed_to: [...] }` — the merged document lands on whichever side(s) hadn't already converged to it, using each adapter's `expect_version` and a fresh idempotency key per push.

## Replay is a no-op

Run the same cycle again immediately and nothing changes — the ancestor already reflects the merged state, so the three-way diff finds no drift on either side:

```rust
// Replay is a NoOp — ancestor advanced, no drift remains.
let replay = engine.sync(ENTITY, PO_ID).await.unwrap();
println!("Replay outcome: {replay:?}");
```

`replay` is `CycleOutcome::NoOp`. This is what makes retries and periodic re-sync safe: re-running a cycle after nothing has moved costs a diff, not a write.

## TypeScript equivalent

The same shape in `sdk/typescript/examples/twoWaySync.ts`:

```typescript
const engine = SyncEngine.builder(erp, inv)
  .policy("price", new OwnedBy("erp"))
  .policy("qty_recv", new Additive())
  .policy(
    "status",
    new StateMachine([
      new StateTransition("open", "closed"),
      new StateTransition("open", "cancelled"),
    ]),
  )
  .seedAncestor(ENTITY, PO_ID, starting)
  .build();

// Shadow run — compute the merge without writing.
const preview = await engine.preview(ENTITY, PO_ID);

// Real cycle.
const outcome = await engine.sync(ENTITY, PO_ID);
switch (outcome.kind) {
  case "NoOp":
    console.log("Nothing to do.");
    break;
  case "Synced":
    console.log(`Synced. Pushed to: ${JSON.stringify(outcome.pushedTo)}`);
    break;
  case "Escalated":
    console.log(`Escalated — ${outcome.conflicts.length} conflict(s) queued:`);
    for (const c of outcome.conflicts) {
      console.log(`  · ${c.path} — ${c.reason}`);
    }
    break;
}
```

Note `CycleOutcome` is a discriminated union with a `kind` tag on the TS side (`"NoOp" | "Synced" | "Escalated"`) versus a Rust enum matched with `matches!`/`match` — same shape, idiomatic per language.

## Fields with no policy

If a field changes and no policy was registered for its path, the orchestrator does not guess. It routes the change straight to the escalation queue as an `UnresolvedConflict` tagged with `ConflictClass::NoPolicy` (`core/src/application/policy/mod.rs`):

```rust
let Some(policy) = policies.lookup(&change.path) else {
    out.conflicts.push(UnresolvedConflict {
        path: change.path.clone(),
        reason: format!("no policy declared for path '{}'", change.path),
        class: ConflictClass::NoPolicy,
        change: change.clone(),
    });
    continue;
};
```

`ConflictClass` has two other members for the same queue: `PolicyConflict` (a declared policy ran and explicitly rejected the change — e.g. `StateMachine` seeing an illegal transition) and `InvariantViolation` (a Tier-2 invariant rejected an otherwise-resolved value). Keeping these as distinct tags lets a caller give `NoPolicy` items a different disposition (probably: "someone forgot to configure this field") than a genuine `PolicyConflict` that needs a human decision.
