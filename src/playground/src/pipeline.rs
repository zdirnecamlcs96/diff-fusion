use crate::dto::{
    ConflictDto, DiffStage, FieldDiff, OutcomeDto, PolicyStage, ProgressEvent, StageData,
    StagesDto, SyncRequest, SyncResponse,
};
use crate::runs::SyncRegistry;
use diff_fusion::adapters::test_memory::TestMemoryAdapter;
use diff_fusion::application::policy::{MergePolicy, MergePolicyRef};
use diff_fusion::compare_json;
use diff_fusion::transform_to_cif;
use diff_fusion::{CycleOutcome, SyncEngine};
use serde_json::Value;
use std::time::Instant;

const ENTITY: &str = "playground";
const ID: &str = "item";

/// Optional fan-out target for stage-by-stage progress. When provided,
/// `pipeline::run` pushes a `ProgressEvent` to the registry under
/// `run_id` as each stage completes; an SSE-attached browser dialog
/// renders them live.
pub struct Progress<'a> {
    pub registry: &'a SyncRegistry,
    pub run_id: &'a str,
}

impl<'a> Progress<'a> {
    fn emit(&self, ev: ProgressEvent) {
        self.registry.push(self.run_id, ev);
    }
}

pub async fn run(req: SyncRequest, progress: Option<Progress<'_>>) -> SyncResponse {
    let mut stages = StagesDto::default();
    let started = Instant::now();
    let emit = |ev: ProgressEvent| {
        if let Some(p) = &progress {
            p.emit(ev);
        }
    };
    let emit_error = |msg: String, partial: &StagesDto| {
        if let Some(p) = &progress {
            p.emit(ProgressEvent::Error {
                message: msg,
                partial: partial.clone(),
            });
        }
    };

    // Stages 1 & 2: transform raw system JSON to CIF — independent, run in parallel.
    // `transform_to_cif` is synchronous and its error type is not `Send`, so each
    // call goes on a blocking worker and the `Result` is mapped to `String` inside
    // the closure before it crosses the task boundary.
    // Each task records its own elapsed time inside the closure so we
    // measure pure work, not scheduler latency.
    let (a_job, b_job) = (
        {
            let source = req.system_a.clone();
            let schema = req.schema.clone();
            let name = req.system_a_name.clone();
            tokio::task::spawn_blocking(move || {
                let t = Instant::now();
                let r = transform_to_cif(&source, &schema, &name).map_err(|e| e.to_string());
                (r, t.elapsed().as_millis() as u64)
            })
        },
        {
            let source = req.system_b.clone();
            let schema = req.schema.clone();
            let name = req.system_b_name.clone();
            tokio::task::spawn_blocking(move || {
                let t = Instant::now();
                let r = transform_to_cif(&source, &schema, &name).map_err(|e| e.to_string());
                (r, t.elapsed().as_millis() as u64)
            })
        },
    );

    let (a_join, b_join) = tokio::join!(a_job, b_job);

    let (cif_a, transform_a_ms) = match a_join.expect("transform A task panicked") {
        (Ok(v), ms) => (v, ms),
        (Err(e), _) => {
            let msg = format!("Transform System A failed: {e}");
            emit_error(msg.clone(), &stages);
            return SyncResponse::with_error(msg, stages);
        }
    };
    let stage_a = StageData { cif: cif_a.clone() };
    stages.transform_a = Some(stage_a.clone());
    stages.timings.transform_a_ms = Some(transform_a_ms);
    emit(ProgressEvent::TransformA {
        data: stage_a,
        duration_ms: transform_a_ms,
    });

    let (cif_b, transform_b_ms) = match b_join.expect("transform B task panicked") {
        (Ok(v), ms) => (v, ms),
        (Err(e), _) => {
            let msg = format!("Transform System B failed: {e}");
            emit_error(msg.clone(), &stages);
            return SyncResponse::with_error(msg, stages);
        }
    };
    let stage_b = StageData { cif: cif_b.clone() };
    stages.transform_b = Some(stage_b.clone());
    stages.timings.transform_b_ms = Some(transform_b_ms);
    emit(ProgressEvent::TransformB {
        data: stage_b,
        duration_ms: transform_b_ms,
    });

    // Stage 3: 3-way diff. Missing ancestor → empty object (first-sync semantics).
    let ancestor = req.ancestor.clone().unwrap_or_else(|| Value::Object(Default::default()));
    let diff_t0 = Instant::now();
    let diff_stage = DiffStage {
        a_vs_ancestor: diffs(&ancestor, &cif_a),
        b_vs_ancestor: diffs(&ancestor, &cif_b),
        a_vs_b: diffs(&cif_a, &cif_b),
        ancestor_used: ancestor.clone(),
    };
    let diff_ms = diff_t0.elapsed().as_millis() as u64;
    stages.diff = Some(diff_stage.clone());
    stages.timings.diff_ms = Some(diff_ms);
    emit(ProgressEvent::Diff {
        data: diff_stage,
        duration_ms: diff_ms,
    });

    // Parse per-field policy declarations — every JSON-declarable `MergePolicyRef`,
    // including `set_by_key`.
    let mut decls: Vec<(String, Box<dyn MergePolicy>)> = Vec::new();
    for (path, decl_value) in &req.policy.per_field {
        match build_policy(decl_value) {
            Ok(policy) => decls.push((path.clone(), policy)),
            Err(e) => {
                let msg = format!("Invalid policy for `{path}`: {e}");
                emit_error(msg.clone(), &stages);
                return SyncResponse::with_error(msg, stages);
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
            let msg = format!("Schema validation failed:\n  • {}", errs.join("\n  • "));
            emit_error(msg.clone(), &stages);
            return SyncResponse::with_error(msg, stages);
        }
    };

    let engine = builder.build();

    // Stage 4: policy preview (dry run).
    let policy_t0 = Instant::now();
    let preview = match engine.preview(ENTITY, ID).await {
        Ok(p) => p,
        Err(e) => {
            let msg = format!("Preview failed: {e}");
            emit_error(msg.clone(), &stages);
            return SyncResponse::with_error(msg, stages);
        }
    };
    let policy_ms = policy_t0.elapsed().as_millis() as u64;
    let policy_stage = PolicyStage {
        would_write: preview.would_write.clone(),
        conflicts: preview.resolution.conflicts.iter().map(ConflictDto::from).collect(),
    };
    stages.policy = Some(policy_stage.clone());
    stages.timings.policy_ms = Some(policy_ms);
    emit(ProgressEvent::Policy {
        data: policy_stage,
        duration_ms: policy_ms,
    });

    // Stage 5: real sync — note adapters are shared state, so this runs
    // against the same seeded views as preview did.
    let outcome_t0 = Instant::now();
    let outcome: CycleOutcome = match engine.sync(ENTITY, ID).await {
        Ok(o) => o,
        Err(e) => {
            let msg = format!("Sync failed: {e}");
            emit_error(msg.clone(), &stages);
            return SyncResponse::with_error(msg, stages);
        }
    };
    let outcome_ms = outcome_t0.elapsed().as_millis() as u64;
    let outcome_dto = OutcomeDto::from(&outcome);
    stages.outcome = Some(outcome_dto.clone());
    stages.timings.outcome_ms = Some(outcome_ms);
    stages.timings.total_ms = started.elapsed().as_millis() as u64;
    emit(ProgressEvent::Outcome {
        data: outcome_dto,
        duration_ms: outcome_ms,
    });

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

/// Build a `MergePolicy` from a raw JSON declaration by deserializing it
/// straight into the library's wire type, `MergePolicyRef` — covers
/// `owned_by`, `additive`, `append`, `state_machine`, `last_write_wins`, and
/// `set_by_key` (with its full `identity`/anchors/`nested` shape).
fn build_policy(decl: &Value) -> Result<Box<dyn MergePolicy>, String> {
    serde_json::from_value::<MergePolicyRef>(decl.clone())
        .map(|r| r.build())
        .map_err(|e| e.to_string())
}
