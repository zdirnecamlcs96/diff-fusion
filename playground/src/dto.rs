use diff_fusion::FacadeConflict;
use diff_fusion::SyncOutcome;
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

#[derive(Debug, Serialize)]
pub struct SyncResponse {
    pub stages: StagesDto,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Default)]
pub struct StagesDto {
    pub transform_a: Option<StageData>,
    pub transform_b: Option<StageData>,
    pub diff: Option<DiffStage>,
    pub policy: Option<PolicyStage>,
    pub outcome: Option<OutcomeDto>,
}

#[derive(Debug, Serialize)]
pub struct StageData {
    pub cif: Value,
}

#[derive(Debug, Serialize)]
pub struct DiffStage {
    pub a_vs_ancestor: Vec<FieldDiff>,
    pub b_vs_ancestor: Vec<FieldDiff>,
    pub a_vs_b: Vec<FieldDiff>,
    pub ancestor_used: Value,
}

#[derive(Debug, Serialize)]
pub struct FieldDiff {
    pub path: String,
    pub left: Value,
    pub right: Value,
}

#[derive(Debug, Serialize)]
pub struct PolicyStage {
    pub would_write: Option<Value>,
    pub conflicts: Vec<ConflictDto>,
}

#[derive(Debug, Serialize)]
pub struct OutcomeDto {
    pub kind: String,
    pub pushed_to: Vec<String>,
    pub conflicts: Vec<ConflictDto>,
}

#[derive(Debug, Serialize)]
pub struct ConflictDto {
    pub path: String,
    pub reason: String,
    pub class: String,
}

impl From<&FacadeConflict> for ConflictDto {
    fn from(c: &FacadeConflict) -> Self {
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

impl From<&SyncOutcome> for OutcomeDto {
    fn from(o: &SyncOutcome) -> Self {
        match o {
            SyncOutcome::NoOp => Self {
                kind: "NoOp".into(),
                pushed_to: Vec::new(),
                conflicts: Vec::new(),
            },
            SyncOutcome::Synced { pushed_to } => Self {
                kind: "Synced".into(),
                pushed_to: pushed_to.clone(),
                conflicts: Vec::new(),
            },
            SyncOutcome::Escalated { conflicts } => Self {
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
