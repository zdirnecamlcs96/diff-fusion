use serde_json::Value;
use std::collections::BTreeSet;

/// Compare two JSONs recursively
pub fn compare_json(a: &Value, b: &Value) -> Vec<(String, (Value, Value))> {
    let mut diffs = Vec::new();
    recurse_compare("", a, b, &mut diffs);
    diffs
}

fn recurse_compare(path: &str, a: &Value, b: &Value, diffs: &mut Vec<(String, (Value, Value))>) {
    // Check for exact equality first
    if a == b {
        return;
    }

    // Numeric equality across int/float reps (5 == 5.0).
    if let (Some(num_a), Some(num_b)) = (a.as_f64(), b.as_f64())
        && (num_a - num_b).abs() < f64::EPSILON
    {
        return;
    }

    match (a, b) {
        (Value::Object(map_a), Value::Object(map_b)) => {
            let keys: BTreeSet<_> = map_a.keys().chain(map_b.keys()).collect();
            for key in keys {
                let new_path = if path.is_empty() {
                    key.to_string()
                } else {
                    format!("{path}.{key}")
                };
                recurse_compare(
                    &new_path,
                    map_a.get(key).unwrap_or(&Value::Null),
                    map_b.get(key).unwrap_or(&Value::Null),
                    diffs,
                );
            }
        }
        _ => {
            diffs.push((path.to_string(), (a.clone(), b.clone())));
        }
    }
}
