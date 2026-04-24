use crate::dto::{
    ConflictDto, DiffStage, FieldDiff, OutcomeDto, PolicyStage, StageData, StagesDto,
    SyncRequest, SyncResponse,
};
use diff_fusion::adapters::test_memory::TestMemoryAdapter;
use diff_fusion::application::policy::{MergePolicy, MergePolicyRef, OnBothChanged, SetByKey};
use diff_fusion::compare_json;
use diff_fusion::transform_to_cif;
use diff_fusion::{SyncEngine, SyncOutcome};
use serde_json::Value;

const ENTITY: &str = "playground";
const ID: &str = "item";

pub async fn run(req: SyncRequest) -> SyncResponse {
    let mut stages = StagesDto::default();

    // Stages 1 & 2: transform raw system JSON to CIF — independent, run in parallel.
    // `transform_to_cif` is synchronous and its error type is not `Send`, so each
    // call goes on a blocking worker and the `Result` is mapped to `String` inside
    // the closure before it crosses the task boundary.
    let (a_job, b_job) = (
        {
            let source = req.system_a.clone();
            let schema = req.schema.clone();
            let name = req.system_a_name.clone();
            tokio::task::spawn_blocking(move || {
                transform_to_cif(&source, &schema, &name).map_err(|e| e.to_string())
            })
        },
        {
            let source = req.system_b.clone();
            let schema = req.schema.clone();
            let name = req.system_b_name.clone();
            tokio::task::spawn_blocking(move || {
                transform_to_cif(&source, &schema, &name).map_err(|e| e.to_string())
            })
        },
    );

    let (a_join, b_join) = tokio::join!(a_job, b_job);

    let cif_a = match a_join.expect("transform A task panicked") {
        Ok(v) => v,
        Err(e) => {
            return SyncResponse::with_error(format!("Transform System A failed: {e}"), stages);
        }
    };
    stages.transform_a = Some(StageData { cif: cif_a.clone() });

    let cif_b = match b_join.expect("transform B task panicked") {
        Ok(v) => v,
        Err(e) => {
            return SyncResponse::with_error(format!("Transform System B failed: {e}"), stages);
        }
    };
    stages.transform_b = Some(StageData { cif: cif_b.clone() });

    // Stage 3: 3-way diff. Missing ancestor → empty object (first-sync semantics).
    let ancestor = req.ancestor.clone().unwrap_or_else(|| Value::Object(Default::default()));
    stages.diff = Some(DiffStage {
        a_vs_ancestor: diffs(&ancestor, &cif_a),
        b_vs_ancestor: diffs(&ancestor, &cif_b),
        a_vs_b: diffs(&cif_a, &cif_b),
        ancestor_used: ancestor.clone(),
    });

    // Parse per-field policy declarations. Supports every JSON-declarable
    // `MergePolicyRef` plus the playground-only `set_by_key` kind (the library
    // has `SetByKey` but it's not in the serde-serializable enum yet; we
    // construct it directly from the public type instead of modifying the lib).
    let mut decls: Vec<(String, Box<dyn MergePolicy>)> = Vec::new();
    for (path, decl_value) in &req.policy.per_field {
        match build_policy(decl_value) {
            Ok(policy) => decls.push((path.clone(), policy)),
            Err(e) => {
                return SyncResponse::with_error(
                    format!("Invalid policy for `{path}`: {e}"),
                    stages,
                );
            }
        }
    }

    // Build adapters with CIF values seeded.
    let adapter_a = TestMemoryAdapter::new(&req.system_a_name);
    adapter_a.seed(ENTITY, ID, cif_a.clone());
    let adapter_b = TestMemoryAdapter::new(&req.system_b_name);
    adapter_b.seed(ENTITY, ID, cif_b.clone());

    // Assemble engine with declared policies + seeded ancestor.
    let mut builder = SyncEngine::builder(adapter_a.clone(), adapter_b.clone());
    for (path, policy) in decls {
        builder = builder.policy(path, policy);
    }
    if req.ancestor.is_some() {
        builder = builder.seed_ancestor(ENTITY, ID, ancestor.clone());
    }

    // Fail-fast schema check: for every policy (e.g. SetByKey) that
    // cares about the CIF element shape, verify its anchor / identity
    // fields are declared at `cif_schema.<path>.element`. Catches typos
    // and missing `anchor` role markers before we waste a sync cycle.
    let builder = match builder.validate_against_schema(&req.schema) {
        Ok(b) => b,
        Err(errs) => {
            return SyncResponse::with_error(
                format!("Schema validation failed:\n  • {}", errs.join("\n  • ")),
                stages,
            );
        }
    };

    let engine = builder.build();

    // Stage 4: policy preview (dry run).
    let preview = match engine.preview(ENTITY, ID).await {
        Ok(p) => p,
        Err(e) => {
            return SyncResponse::with_error(format!("Preview failed: {e}"), stages);
        }
    };
    stages.policy = Some(PolicyStage {
        would_write: preview.would_write.clone(),
        conflicts: preview.conflicts.iter().map(ConflictDto::from).collect(),
    });

    // Stage 5: real sync — note adapters are shared state, so this runs
    // against the same seeded views as preview did.
    let outcome: SyncOutcome = match engine.sync(ENTITY, ID).await {
        Ok(o) => o,
        Err(e) => {
            return SyncResponse::with_error(format!("Sync failed: {e}"), stages);
        }
    };
    stages.outcome = Some(OutcomeDto::from(&outcome));

    SyncResponse {
        stages,
        error: None,
    }
}

fn diffs(a: &Value, b: &Value) -> Vec<FieldDiff> {
    compare_json(a, b)
        .into_iter()
        .map(|(path, (left, right))| FieldDiff { path, left, right })
        .collect()
}

/// Build a `MergePolicy` from a raw JSON declaration.
///
/// Recognizes every variant of the library's `MergePolicyRef` (`owned_by`,
/// `additive`, `append`, `state_machine`) plus `set_by_key` for
/// array-of-objects merging. A `set_by_key` declaration requires three
/// keys: `identity` (the composite business identity), `a_anchor`, and
/// `b_anchor` (each side's stable local row ID). Anchors are mandatory —
/// any realistic cross-system merge involves one system handing out
/// immutable IDs while the business key mutates, and the library doesn't
/// ship a way to opt out of that concern.
fn build_policy(decl: &Value) -> Result<Box<dyn MergePolicy>, String> {
    let kind = decl
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| "policy missing `kind`".to_string())?;

    if kind == "set_by_key" {
        return Ok(Box::new(build_set_by_key(decl)?));
    }

    serde_json::from_value::<MergePolicyRef>(decl.clone())
        .map(|r| r.build())
        .map_err(|e| e.to_string())
}

/// Parse a `set_by_key` declaration. Split out so nested declarations
/// (`nested: {field: {kind: set_by_key, ...}}`) can recurse without going
/// through the `Box<dyn MergePolicy>` wrapper.
fn build_set_by_key(decl: &Value) -> Result<SetByKey, String> {
    let identity: Vec<String> = match decl.get("identity") {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(arr)) => arr
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| "identity array entries must be strings".to_string())
            })
            .collect::<Result<_, _>>()?,
        Some(other) => {
            return Err(format!("identity must be string or array, got {other}"));
        }
        None => return Err("set_by_key requires `identity`".to_string()),
    };
    if identity.is_empty() {
        return Err("identity cannot be empty".to_string());
    }

    let a_anchor = decl
        .get("a_anchor")
        .and_then(Value::as_str)
        .ok_or_else(|| "set_by_key requires `a_anchor`".to_string())?;
    let b_anchor = decl
        .get("b_anchor")
        .and_then(Value::as_str)
        .ok_or_else(|| "set_by_key requires `b_anchor`".to_string())?;

    let mut policy = SetByKey::new(identity, a_anchor, b_anchor);
    if let Some(s) = decl.get("on_both_changed").and_then(Value::as_str) {
        policy.on_both_changed = match s {
            "union" => OnBothChanged::Union,
            "prefer_a" => OnBothChanged::PreferA,
            "prefer_b" => OnBothChanged::PreferB,
            "escalate" => OnBothChanged::Escalate,
            other => return Err(format!("unknown on_both_changed: `{other}`")),
        };
    }
    // Recursive nested declarations: each value under `nested` must itself
    // be a `set_by_key` declaration. Lets the same mechanism reconcile
    // nested line items (e.g. fulfillment.items[], GRN.received[]) within
    // a matched parent element rather than shallow-overwriting them.
    if let Some(nested_decl) = decl.get("nested") {
        let obj = nested_decl
            .as_object()
            .ok_or_else(|| "nested must be an object mapping field name to policy".to_string())?;
        for (field, sub_decl) in obj {
            let sub = build_set_by_key(sub_decl)
                .map_err(|e| format!("nested.{field}: {e}"))?;
            policy.nested.insert(field.clone(), sub);
        }
    }
    Ok(policy)
}
