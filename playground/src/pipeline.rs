use crate::dto::{
    ConflictDto, DiffStage, FieldDiff, OutcomeDto, PolicyStage, StageData, StagesDto,
    SyncRequest, SyncResponse,
};
use diff_fusion::adapters::test_memory::TestMemoryAdapter;
use diff_fusion::application::policy::MergePolicyRef;
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

    // Parse per-field policy declarations.
    let mut decls: Vec<(String, MergePolicyRef)> = Vec::new();
    for (path, decl_value) in &req.policy.per_field {
        match serde_json::from_value::<MergePolicyRef>(decl_value.clone()) {
            Ok(ref_) => decls.push((path.clone(), ref_)),
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
    for (path, decl) in &decls {
        builder = builder.policy(path.clone(), decl.build());
    }
    if req.ancestor.is_some() {
        builder = builder.seed_ancestor(ENTITY, ID, ancestor.clone());
    }
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
