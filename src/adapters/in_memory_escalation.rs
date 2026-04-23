//! In-memory [`EscalationQueue`] — reference impl used by tests and the
//! default [`crate::drivers::sync_engine::SyncEngine`] configuration.
//!
//! Not durable. Production deployments implement the trait against a
//! durable store (Postgres, SQS, etc.) so items survive a restart.

use crate::ports::escalation::{EscalationError, EscalationItem, EscalationQueue};
use std::sync::Mutex;

#[derive(Debug, Default)]
pub struct InMemoryEscalationQueue {
    items: Mutex<Vec<EscalationItem>>,
}

impl InMemoryEscalationQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the queue — tests use this to assert what was escalated.
    pub fn snapshot(&self) -> Vec<EscalationItem> {
        self.items.lock().expect("poisoned").clone()
    }
}

impl EscalationQueue for InMemoryEscalationQueue {
    fn push(&self, item: EscalationItem) -> Result<(), EscalationError> {
        self.items
            .lock()
            .map_err(|e| EscalationError::Backend(e.to_string()))?
            .push(item);
        Ok(())
    }

    fn len(&self) -> usize {
        self.items.lock().expect("poisoned").len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::policy::{ConflictClass, UnresolvedConflict};
    use crate::domain::diff::{ChangeSource, FieldChange};
    use serde_json::json;

    fn item(path: &str) -> EscalationItem {
        EscalationItem {
            entity_type: "purchase_order".into(),
            canonical_id: "PO-1".into(),
            conflicts: vec![UnresolvedConflict {
                path: path.into(),
                reason: "divergent".into(),
                class: ConflictClass::PolicyConflict,
                change: FieldChange {
                    path: path.into(),
                    old_value: json!(0),
                    new_from_a: Some(json!(1)),
                    new_from_b: Some(json!(2)),
                    source: ChangeSource::Both,
                },
            }],
            created_at_ms: 1,
        }
    }

    #[test]
    fn push_appends_items() {
        let q = InMemoryEscalationQueue::new();
        assert_eq!(q.len(), 0);

        q.push(item("price")).unwrap();
        q.push(item("qty")).unwrap();

        assert_eq!(q.len(), 2);
        let snap = q.snapshot();
        assert_eq!(snap[0].conflicts[0].path, "price");
        assert_eq!(snap[1].conflicts[0].path, "qty");
    }
}
