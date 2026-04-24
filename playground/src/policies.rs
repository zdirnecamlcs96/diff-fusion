//! Playground-scoped demo tests for the core `SetByKey` policy.
//!
//! The composite-identity + anchor + nested variant that used to live
//! here has been promoted into `diff_fusion::application::policy::SetByKey`
//! (see `src/application/policy/structural.rs`). Keeping this file only
//! for the purchase-order demo tests that the playground's UI-driven
//! `pipeline.rs` complements.

#[cfg(test)]
mod po_fulfillment_grn_demo {
    //! Demonstrates that a single purchase-order document bundling BOTH
    //! `deliveryFullfillment` (fulfillment) and `deliveryOrder` (GRN) arrays
    //! can be reconciled by binding `SetByKey` to each path with its own
    //! identity + anchor + on-conflict strategy. The orchestrator resolves
    //! both paths within one cycle and the stale side is pushed via
    //! `SystemPort::upsert` — adapter-returned errors (e.g. NetSuite
    //! rejecting an over-receive) propagate as `SyncError` with the
    //! ancestor left untouched for retry.
    use diff_fusion::adapters::test_memory::TestMemoryAdapter;
    use diff_fusion::application::policy::{OnBothChanged, SetByKey};
    use diff_fusion::ports::system::SystemPort;
    use diff_fusion::{SyncEngine, SyncOutcome};
    use serde_json::json;

    const ENTITY: &str = "purchase_order";
    const ID: &str = "PO-1";

    /// Fulfillment policy: identity by (lineId), each side has its own
    /// stable local ID field, union on both-sides edits.
    fn fulfillment_policy() -> SetByKey {
        let mut p = SetByKey::new(
            vec!["lineId".into()],
            "internalFulfillmentId",
            "nsFulfillmentId",
        );
        p.on_both_changed = OnBothChanged::Union;
        p
    }

    /// GRN policy: identity by (grnId), union by default so compatible
    /// enrichments merge cleanly. Per-test overrides flip to Escalate
    /// when we need to verify the over-receive case.
    fn grn_policy_union() -> SetByKey {
        let mut p = SetByKey::new(vec!["grnId".into()], "internalGrnId", "nsGrnId");
        p.on_both_changed = OnBothChanged::Union;
        p
    }

    fn grn_policy_escalate() -> SetByKey {
        let mut p = SetByKey::new(vec!["grnId".into()], "internalGrnId", "nsGrnId");
        p.on_both_changed = OnBothChanged::Escalate;
        p
    }

    #[tokio::test]
    async fn fulfillment_and_grn_changes_flow_in_one_cycle() {
        // A (internal) adds a second fulfillment line.
        // B (netsuite) enriches the existing GRN line with a NetSuite ref.
        // One cycle should converge both sides.
        let a = TestMemoryAdapter::new("internal");
        let b = TestMemoryAdapter::new("netsuite");

        let ancestor = json!({
            "deliveryFullfillment": [
                {"lineId": "F1", "internalFulfillmentId": "IF1", "nsFulfillmentId": "NS-F1", "items": 3}
            ],
            "deliveryOrder": [
                {"grnId": "G1", "internalGrnId": "IG1", "nsGrnId": "NS-G1", "fullfillmentId": "F1", "receivedQty": 3}
            ]
        });

        a.seed(
            ENTITY,
            ID,
            json!({
                "deliveryFullfillment": [
                    {"lineId": "F1", "internalFulfillmentId": "IF1", "nsFulfillmentId": "NS-F1", "items": 3},
                    {"lineId": "F2", "internalFulfillmentId": "IF2", "items": 2}
                ],
                "deliveryOrder": [
                    {"grnId": "G1", "internalGrnId": "IG1", "nsGrnId": "NS-G1", "fullfillmentId": "F1", "receivedQty": 3}
                ]
            }),
        );

        b.seed(
            ENTITY,
            ID,
            json!({
                "deliveryFullfillment": [
                    {"lineId": "F1", "internalFulfillmentId": "IF1", "nsFulfillmentId": "NS-F1", "items": 3}
                ],
                "deliveryOrder": [
                    {"grnId": "G1", "internalGrnId": "IG1", "nsGrnId": "NS-G1", "fullfillmentId": "F1", "receivedQty": 3, "netSuiteRef": "GRN-001"}
                ]
            }),
        );

        let engine = SyncEngine::builder(a.clone(), b.clone())
            .policy("deliveryFullfillment", Box::new(fulfillment_policy()))
            .policy("deliveryOrder", Box::new(grn_policy_union()))
            .seed_ancestor(ENTITY, ID, ancestor)
            .build();

        let outcome = engine.sync(ENTITY, ID).await.expect("sync ok");
        assert!(
            matches!(outcome, SyncOutcome::Synced { .. }),
            "expected Synced, got {outcome:?}"
        );

        for (port, label) in [(&a, "internal"), (&b, "netsuite")] {
            let ext = port
                .find_by_canonical_id(ENTITY, ID)
                .await
                .unwrap()
                .unwrap();
            let (val, _) = port.fetch(ENTITY, &ext).await.unwrap();

            let fulfillments = val["deliveryFullfillment"].as_array().unwrap();
            let fids: Vec<&str> = fulfillments
                .iter()
                .map(|f| f["lineId"].as_str().unwrap())
                .collect();
            assert!(
                fids.contains(&"F1") && fids.contains(&"F2"),
                "{label}: missing fulfillment lines, got {fids:?}"
            );

            let grns = val["deliveryOrder"].as_array().unwrap();
            assert_eq!(grns.len(), 1, "{label}: GRN count");
            assert_eq!(
                grns[0]["netSuiteRef"],
                json!("GRN-001"),
                "{label}: should carry NetSuite ref from B"
            );
        }
        assert_eq!(engine.escalation_depth(), 0);
    }

    #[tokio::test]
    async fn conflicting_grn_quantities_escalate_without_pushing() {
        // Both sides changed receivedQty for the same GRN line to different
        // values (the "over-receive disagreement" case). Escalate blocks
        // the push; ancestor stays put; sides keep their divergent state.
        let a = TestMemoryAdapter::new("internal");
        let b = TestMemoryAdapter::new("netsuite");

        let ancestor = json!({
            "deliveryFullfillment": [
                {"lineId": "F1", "internalFulfillmentId": "IF1", "nsFulfillmentId": "NS-F1", "items": 3}
            ],
            "deliveryOrder": [
                {"grnId": "G1", "internalGrnId": "IG1", "nsGrnId": "NS-G1", "receivedQty": 3}
            ]
        });

        a.seed(
            ENTITY,
            ID,
            json!({
                "deliveryFullfillment": [
                    {"lineId": "F1", "internalFulfillmentId": "IF1", "nsFulfillmentId": "NS-F1", "items": 3}
                ],
                "deliveryOrder": [
                    {"grnId": "G1", "internalGrnId": "IG1", "nsGrnId": "NS-G1", "receivedQty": 5}
                ]
            }),
        );
        b.seed(
            ENTITY,
            ID,
            json!({
                "deliveryFullfillment": [
                    {"lineId": "F1", "internalFulfillmentId": "IF1", "nsFulfillmentId": "NS-F1", "items": 3}
                ],
                "deliveryOrder": [
                    {"grnId": "G1", "internalGrnId": "IG1", "nsGrnId": "NS-G1", "receivedQty": 4}
                ]
            }),
        );

        let engine = SyncEngine::builder(a.clone(), b.clone())
            .policy("deliveryFullfillment", Box::new(fulfillment_policy()))
            .policy("deliveryOrder", Box::new(grn_policy_escalate()))
            .seed_ancestor(ENTITY, ID, ancestor)
            .build();

        let outcome = engine.sync(ENTITY, ID).await.expect("sync ok");
        match outcome {
            SyncOutcome::Escalated { conflicts } => {
                assert_eq!(conflicts.len(), 1);
                assert_eq!(conflicts[0].path, "deliveryOrder");
            }
            other => panic!("expected Escalated, got {other:?}"),
        }

        let (a_view, _) = a
            .fetch(
                ENTITY,
                &a.find_by_canonical_id(ENTITY, ID).await.unwrap().unwrap(),
            )
            .await
            .unwrap();
        let (b_view, _) = b
            .fetch(
                ENTITY,
                &b.find_by_canonical_id(ENTITY, ID).await.unwrap().unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(a_view["deliveryOrder"][0]["receivedQty"], json!(5));
        assert_eq!(b_view["deliveryOrder"][0]["receivedQty"], json!(4));
        assert_eq!(engine.escalation_depth(), 1);
    }

    #[tokio::test]
    async fn nested_items_inside_fulfillment_and_grn_merge_independently() {
        // deliveryFullfillment[*].items[] and deliveryOrder[*].received[]
        // are nested arrays. Both sides edit line items inside the same
        // parent. The outer policy recursively delegates to a nested
        // SetByKey for each named sub-array, so per-line detail is
        // preserved rather than flattened.
        let a = TestMemoryAdapter::new("internal");
        let b = TestMemoryAdapter::new("netsuite");

        let ancestor = json!({
            "deliveryFullfillment": [
                {
                    "lineId": "F1",
                    "internalFulfillmentId": "IF1",
                    "nsFulfillmentId": "NS-F1",
                    "items": [
                        {"sku": "SKU-A", "amount": 3}
                    ]
                }
            ],
            "deliveryOrder": [
                {
                    "grnId": "G1",
                    "internalGrnId": "IG1",
                    "nsGrnId": "NS-G1",
                    "fullfillmentId": "F1",
                    "received": [
                        {"sku": "SKU-A", "amount": 3}
                    ]
                }
            ]
        });

        a.seed(
            ENTITY,
            ID,
            json!({
                "deliveryFullfillment": [
                    {
                        "lineId": "F1",
                        "internalFulfillmentId": "IF1",
                        "nsFulfillmentId": "NS-F1",
                        "items": [
                            {"sku": "SKU-A", "amount": 3},
                            {"sku": "SKU-B", "amount": 1}
                        ]
                    }
                ],
                "deliveryOrder": [
                    {
                        "grnId": "G1",
                        "internalGrnId": "IG1",
                        "nsGrnId": "NS-G1",
                        "fullfillmentId": "F1",
                        "received": [
                            {"sku": "SKU-A", "amount": 3, "cost": 50}
                        ]
                    }
                ]
            }),
        );

        b.seed(
            ENTITY,
            ID,
            json!({
                "deliveryFullfillment": [
                    {
                        "lineId": "F1",
                        "internalFulfillmentId": "IF1",
                        "nsFulfillmentId": "NS-F1",
                        "items": [
                            {"sku": "SKU-A", "amount": 3},
                            {"sku": "SKU-C", "amount": 2}
                        ]
                    }
                ],
                "deliveryOrder": [
                    {
                        "grnId": "G1",
                        "internalGrnId": "IG1",
                        "nsGrnId": "NS-G1",
                        "fullfillmentId": "F1",
                        "received": [
                            {"sku": "SKU-A", "amount": 3, "cost": 50},
                            {"sku": "SKU-B", "amount": 1}
                        ]
                    }
                ]
            }),
        );

        let items_policy = SetByKey::new(vec!["sku".into()], "sku", "sku");
        let received_policy = SetByKey::new(vec!["sku".into()], "sku", "sku");

        let mut fulfillment_with_nested = fulfillment_policy();
        fulfillment_with_nested
            .nested
            .insert("items".into(), items_policy);
        let mut grn_with_nested = grn_policy_union();
        grn_with_nested
            .nested
            .insert("received".into(), received_policy);

        let engine = SyncEngine::builder(a.clone(), b.clone())
            .policy("deliveryFullfillment", Box::new(fulfillment_with_nested))
            .policy("deliveryOrder", Box::new(grn_with_nested))
            .seed_ancestor(ENTITY, ID, ancestor)
            .build();

        let outcome = engine.sync(ENTITY, ID).await.expect("sync ok");
        assert!(
            matches!(outcome, SyncOutcome::Synced { .. }),
            "expected Synced, got {outcome:?}"
        );

        for (port, label) in [(&a, "internal"), (&b, "netsuite")] {
            let ext = port
                .find_by_canonical_id(ENTITY, ID)
                .await
                .unwrap()
                .unwrap();
            let (val, _) = port.fetch(ENTITY, &ext).await.unwrap();

            let items = val["deliveryFullfillment"][0]["items"].as_array().unwrap();
            let skus: Vec<&str> = items.iter().map(|i| i["sku"].as_str().unwrap()).collect();
            assert!(
                skus.contains(&"SKU-A") && skus.contains(&"SKU-B") && skus.contains(&"SKU-C"),
                "{label}: fulfillment items should contain A, B, C — got {skus:?}"
            );

            let received = val["deliveryOrder"][0]["received"].as_array().unwrap();
            let r_skus: Vec<&str> = received
                .iter()
                .map(|i| i["sku"].as_str().unwrap())
                .collect();
            assert!(
                r_skus.contains(&"SKU-A") && r_skus.contains(&"SKU-B"),
                "{label}: received items should contain A and B — got {r_skus:?}"
            );
            let sku_a_cost = received
                .iter()
                .find(|r| r["sku"] == "SKU-A")
                .and_then(|r| r.get("cost"))
                .cloned();
            assert_eq!(
                sku_a_cost,
                Some(json!(50)),
                "{label}: SKU-A cost should survive nested merge"
            );
        }
        assert_eq!(engine.escalation_depth(), 0);
    }
}
