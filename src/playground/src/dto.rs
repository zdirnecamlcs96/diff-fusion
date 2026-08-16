use diff_fusion::CycleOutcome;
use diff_fusion::UnresolvedConflict;
use diff_fusion::application::policy::ConflictClass;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct SyncRequest {
    pub system_a: Value,
    pub system_b: Value,
    pub schema: Value,
    pub policy: PolicyConfig,
    #[serde(default)]
    pub ancestor: Option<Value>,
    #[serde(default = "default_a_name")]
    pub system_a_name: String,
    #[serde(default = "default_b_name")]
    pub system_b_name: String,
    /// Optional. When provided, `pipeline::run` pushes a `ProgressEvent`
    /// to the `SyncRegistry` under this key as each pipeline stage
    /// completes. Surfaced over SSE at `/api/sync/:sync_id/stream` so a
    /// browser dialog can watch the cycle land stage by stage instead of
    /// waiting for the full HTTP response.
    #[serde(default)]
    pub run_id: Option<String>,
}

/// One-stage-at-a-time progress for the demo form's dialog. Each variant
/// carries the fully-formed `StagesDto` chunk plus the wall-clock
/// duration of *that stage's work*.
///
/// `duration_ms` for parallel stages (`transform_a` / `transform_b`)
/// reports per-task time, not total elapsed since the request started —
/// the wall-clock for the whole parallel group is `max(a, b)`.
///
/// Wire format mirrors `StagesDto` field names so JS code reads
/// `event.stage === "transform_a"` etc.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum ProgressEvent {
    TransformA { data: StageData, duration_ms: u64 },
    TransformB { data: StageData, duration_ms: u64 },
    Diff { data: DiffStage, duration_ms: u64 },
    Policy { data: PolicyStage, duration_ms: u64 },
    Outcome { data: OutcomeDto, duration_ms: u64 },
    /// Pipeline aborted partway through. `partial` carries whatever
    /// stages had completed so the dialog can still render context.
    Error { message: String, partial: StagesDto },
}

fn default_a_name() -> String {
    "system_a".into()
}
fn default_b_name() -> String {
    "system_b".into()
}

/// User-supplied policy config.
///
/// Shape:
/// ```json
/// {
///   "per_field": {
///     "price":    {"kind": "owned_by", "system": "system_a"},
///     "qty_recv": {"kind": "additive"}
///   }
/// }
/// ```
#[derive(Debug, Deserialize)]
pub struct PolicyConfig {
    #[serde(default)]
    pub per_field: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncResponse {
    pub stages: StagesDto,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct StagesDto {
    pub transform_a: Option<StageData>,
    pub transform_b: Option<StageData>,
    pub diff: Option<DiffStage>,
    pub policy: Option<PolicyStage>,
    pub outcome: Option<OutcomeDto>,
    /// Per-stage wall-clock timing. Filled in as stages complete; a
    /// `None` means the stage didn't run (e.g. pipeline aborted earlier).
    pub timings: Timings,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Timings {
    pub transform_a_ms: Option<u64>,
    pub transform_b_ms: Option<u64>,
    pub diff_ms: Option<u64>,
    pub policy_ms: Option<u64>,
    pub outcome_ms: Option<u64>,
    pub total_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StageData {
    pub cif: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffStage {
    pub a_vs_ancestor: Vec<FieldDiff>,
    pub b_vs_ancestor: Vec<FieldDiff>,
    pub a_vs_b: Vec<FieldDiff>,
    pub ancestor_used: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldDiff {
    pub path: String,
    pub left: Value,
    pub right: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct PolicyStage {
    pub would_write: Option<Value>,
    pub conflicts: Vec<ConflictDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OutcomeDto {
    pub kind: String,
    pub pushed_to: Vec<String>,
    pub conflicts: Vec<ConflictDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConflictDto {
    pub path: String,
    pub reason: String,
    pub class: String,
}

impl From<&UnresolvedConflict> for ConflictDto {
    fn from(c: &UnresolvedConflict) -> Self {
        Self {
            path: c.path.clone(),
            reason: c.reason.clone(),
            class: conflict_class_label(&c.class).to_string(),
        }
    }
}

fn conflict_class_label(c: &ConflictClass) -> &'static str {
    match c {
        ConflictClass::NoPolicy => "NoPolicy",
        ConflictClass::PolicyConflict => "PolicyConflict",
        ConflictClass::InvariantViolation => "InvariantViolation",
    }
}

impl From<&CycleOutcome> for OutcomeDto {
    fn from(o: &CycleOutcome) -> Self {
        match o {
            CycleOutcome::NoOp => Self {
                kind: "NoOp".into(),
                pushed_to: Vec::new(),
                conflicts: Vec::new(),
            },
            CycleOutcome::Synced { pushed_to } => Self {
                kind: "Synced".into(),
                pushed_to: pushed_to.clone(),
                conflicts: Vec::new(),
            },
            CycleOutcome::Escalated { conflicts } => Self {
                kind: "Escalated".into(),
                pushed_to: Vec::new(),
                conflicts: conflicts.iter().map(ConflictDto::from).collect(),
            },
        }
    }
}

impl SyncResponse {
    pub fn with_error(msg: impl Into<String>, stages: StagesDto) -> Self {
        Self {
            stages,
            error: Some(msg.into()),
        }
    }
}
