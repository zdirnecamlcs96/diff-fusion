//! Two-way reconciliation end to end — via the `SyncEngine` facade.
//!
//! This example imports **only** the facade, the policy types, and the
//! adapter type. No `Orchestrator`, no `Arc<dyn …>`, no
//! `InMemoryAncestorStore`, no `PolicyMap::with_default(...)` ceremony.
//! Everything internal stays internal.
//!
//! Run with:
//!
//! ```bash
//! cargo run --example two_way_sync
//! ```

use diff_fusion::adapters::test_memory::TestMemoryAdapter;
use diff_fusion::domain::idempotency::idempotency_key;
use diff_fusion::application::policy::{Additive, OwnedBy, StateMachine, StateTransition};
use diff_fusion::ports::system::SystemPort;
use diff_fusion::CycleOutcome;
use diff_fusion::drivers::sync_engine::SyncEngine;
use serde_json::json;

const ENTITY: &str = "purchase_order";
const PO_ID: &str = "PO-42";

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("==== diff-fusion · two-way sync (facade) ====\n");

    // ---------------------------------------------------------------
    // Set up two systems. Users only touch the adapter type.
    // ---------------------------------------------------------------
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

    println!("BEFORE sync:");
    print_side(&erp, "  erp").await;
    print_side(&inv, "  inv").await;
    println!();

    // ---------------------------------------------------------------
    // Build the engine. This is the user-facing API — one chain of
    // builder calls, no Arc wrapping, no orchestrator import.
    // ---------------------------------------------------------------
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

    // ---------------------------------------------------------------
    // Shadow run first (dry run) — the facade calls it `preview`.
    // ---------------------------------------------------------------
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
    println!();

    // ---------------------------------------------------------------
    // Real cycle.
    // ---------------------------------------------------------------
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

    println!("\nAFTER sync:");
    print_side(&erp, "  erp").await;
    print_side(&inv, "  inv").await;

    println!("\nEscalation queue depth: {}", engine.escalation_depth());

    // Replay is a NoOp — ancestor advanced, no drift remains.
    let replay = engine.sync(ENTITY, PO_ID).await.unwrap();
    println!("Replay outcome: {replay:?}");
}

/// Test-only helper: overwrite an adapter's current view.
async fn update(port: &TestMemoryAdapter, new_value: serde_json::Value) {
    let current = port
        .find_by_canonical_id(ENTITY, PO_ID)
        .await
        .unwrap()
        .expect("seeded entity");
    let ik = idempotency_key(PO_ID, "upsert", &new_value);
    port.upsert(ENTITY, PO_ID, &new_value, current.version.as_deref(), &ik)
        .await
        .unwrap();
}

async fn print_side(port: &TestMemoryAdapter, label: &str) {
    let ext = port.find_by_canonical_id(ENTITY, PO_ID).await.unwrap().unwrap();
    let (val, _) = port.fetch(ENTITY, &ext).await.unwrap();
    println!("{label} ({}): {val}", port.system_type());
}
