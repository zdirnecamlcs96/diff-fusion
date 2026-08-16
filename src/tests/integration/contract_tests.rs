//! Shared contract test suite for every `SystemPort` adapter.
//!
//! An adapter is "done" when it passes every test in [`run_contract_suite`].
//! No judgement calls — the suite is the forcing function that keeps per-
//! adapter implementations honest about the port's invariants.
//!
//! New adapters re-use this file by adding a small driver at the bottom
//! that calls `run_contract_suite(<my_adapter>)`.

use diff_fusion::adapters::test_memory::TestMemoryAdapter;
use diff_fusion::domain::error::SyncError;
use diff_fusion::domain::idempotency::idempotency_key;
use diff_fusion::ports::system::SystemPort;
use serde_json::json;

/// Run every behavioral check against `port`. Tokio-flavored, serial — adapters
/// that require special test setup should wrap this in their own harness.
pub async fn run_contract_suite<P: SystemPort>(port: &P) {
    find_returns_none_for_missing(port).await;
    upsert_then_find(port).await;
    idempotent_repeat_is_noop(port).await;
    stale_version_is_rejected(port).await;
    fetch_roundtrips_canonical(port).await;
    expect_version_on_missing_record_is_stale(port).await;
}

async fn find_returns_none_for_missing<P: SystemPort>(port: &P) {
    let r = port
        .find_by_canonical_id("contract_entity", "contract_id_missing")
        .await
        .expect("find_by_canonical_id must not error on missing");
    assert!(r.is_none(), "missing entity should yield None, got {:?}", r);
}

async fn upsert_then_find<P: SystemPort>(port: &P) {
    let payload = json!({"contract_field": "found"});
    let k = idempotency_key("contract_id_1", "upsert", &payload);
    let inserted = port
        .upsert("contract_entity", "contract_id_1", &payload, None, &k)
        .await
        .expect("first upsert should succeed");
    assert_eq!(inserted.system, port.system_type());

    let found = port
        .find_by_canonical_id("contract_entity", "contract_id_1")
        .await
        .expect("find must succeed after upsert")
        .expect("find must return Some after upsert");
    assert_eq!(
        found.external_id, inserted.external_id,
        "find must return the ref created by upsert"
    );
}

async fn idempotent_repeat_is_noop<P: SystemPort>(port: &P) {
    let payload = json!({"contract_field": "idempotent"});
    let k = idempotency_key("contract_id_idem", "upsert", &payload);
    let r1 = port
        .upsert("contract_entity", "contract_id_idem", &payload, None, &k)
        .await
        .unwrap();
    let r2 = port
        .upsert("contract_entity", "contract_id_idem", &payload, None, &k)
        .await
        .unwrap();
    assert_eq!(
        r1, r2,
        "replaying the same idempotency key must not create a second record or advance the version"
    );
}

async fn stale_version_is_rejected<P: SystemPort>(port: &P) {
    let payload = json!({"n": 1});
    let k1 = idempotency_key("contract_id_stale", "upsert", &payload);
    let r1 = port
        .upsert("contract_entity", "contract_id_stale", &payload, None, &k1)
        .await
        .unwrap();
    let k2 = idempotency_key("contract_id_stale", "upsert", &json!({"n": 2}));
    let err = port
        .upsert(
            "contract_entity",
            "contract_id_stale",
            &json!({"n": 2}),
            Some("version-that-will-never-match"),
            &k2,
        )
        .await
        .expect_err("upsert with wrong expect_version must fail");
    assert!(
        matches!(err, SyncError::StaleWrite { .. }),
        "expected StaleWrite, got {err:?}"
    );
    // Ensure the adapter did not advance after the rejected write.
    let after = port
        .find_by_canonical_id("contract_entity", "contract_id_stale")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.version, r1.version, "rejected write must not bump version");
}

async fn fetch_roundtrips_canonical<P: SystemPort>(port: &P) {
    let payload = json!({"x": 42});
    let k = idempotency_key("contract_id_fetch", "upsert", &payload);
    let r = port
        .upsert("contract_entity", "contract_id_fetch", &payload, None, &k)
        .await
        .unwrap();
    let (val, _ref) = port.fetch("contract_entity", &r).await.unwrap();
    assert_eq!(val, payload, "fetch must return exactly the canonical we wrote");
}

async fn expect_version_on_missing_record_is_stale<P: SystemPort>(port: &P) {
    let k = idempotency_key("contract_id_no_record", "upsert", &json!({}));
    let err = port
        .upsert(
            "contract_entity",
            "contract_id_no_record",
            &json!({}),
            Some("1"),
            &k,
        )
        .await
        .expect_err(
            "upsert with expect_version but no existing record must fail \
             rather than silently creating",
        );
    assert!(matches!(err, SyncError::StaleWrite { .. }));
}

// --- Drivers: one per adapter under test -----------------------------------

#[tokio::test]
async fn test_memory_adapter_passes_contract() {
    let adapter = TestMemoryAdapter::new("contract_sys_a");
    run_contract_suite(&adapter).await;
}

#[tokio::test]
async fn test_memory_adapter_passes_contract_under_other_name() {
    // Same adapter implementation, different system_type label — the
    // contract must hold regardless of the external identifier.
    let adapter = TestMemoryAdapter::new("contract_sys_b");
    run_contract_suite(&adapter).await;
}
