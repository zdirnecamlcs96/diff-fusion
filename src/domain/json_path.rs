//! Dotted-path walkers over `serde_json::Value`.
//!
//! The three-way diff produces changelog paths like `"pricing.amount"`
//! (dotted object keys). This module provides the inverse operation:
//! given a path and a value, set the leaf at that path, creating any
//! missing intermediate objects.
//!
//! Paths do not support array indexing — the diff primitive emits array
//! mismatches as a single change at the array's own path, so there's no
//! `"items.0.sku"` style input to handle here.

use serde_json::{Map, Value};

/// Set `new_value` at the dotted `path` inside `target`. Intermediate
/// objects are created when missing; non-object values along the path
/// are replaced with a fresh empty object so descent can continue.
///
/// An empty path is a no-op.
pub fn set_at_path(target: &mut Value, path: &str, new_value: Value) {
    if path.is_empty() {
        return;
    }
    let parts: Vec<&str> = path.split('.').collect();
    set_recursive(target, &parts, new_value);
}

fn set_recursive(target: &mut Value, parts: &[&str], new_value: Value) {
    let Some((head, rest)) = parts.split_first() else {
        return;
    };

    if !target.is_object() {
        *target = Value::Object(Map::new());
    }
    let map = target.as_object_mut().expect("is object");

    if rest.is_empty() {
        map.insert((*head).to_string(), new_value);
        return;
    }

    let child = map
        .entry((*head).to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    set_recursive(child, rest, new_value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_path_is_noop() {
        let mut v = json!({"a": 1});
        set_at_path(&mut v, "", json!(99));
        assert_eq!(v, json!({"a": 1}));
    }

    #[test]
    fn leaf_set_overwrites_existing_value() {
        let mut v = json!({"a": 1, "b": 2});
        set_at_path(&mut v, "a", json!(99));
        assert_eq!(v, json!({"a": 99, "b": 2}));
    }

    #[test]
    fn leaf_set_inserts_missing_key() {
        let mut v = json!({"a": 1});
        set_at_path(&mut v, "b", json!(2));
        assert_eq!(v, json!({"a": 1, "b": 2}));
    }

    #[test]
    fn nested_path_creates_intermediate_objects() {
        let mut v = json!({});
        set_at_path(&mut v, "outer.inner.leaf", json!("x"));
        assert_eq!(v, json!({"outer": {"inner": {"leaf": "x"}}}));
    }

    #[test]
    fn nested_path_overwrites_existing_leaf() {
        let mut v = json!({"outer": {"inner": {"leaf": "old"}}});
        set_at_path(&mut v, "outer.inner.leaf", json!("new"));
        assert_eq!(v, json!({"outer": {"inner": {"leaf": "new"}}}));
    }

    #[test]
    fn non_object_along_path_is_replaced() {
        // "outer" currently holds a scalar; descending through it should
        // replace the scalar with an object so the nested set can
        // complete. This keeps the function total.
        let mut v = json!({"outer": 42});
        set_at_path(&mut v, "outer.inner", json!("x"));
        assert_eq!(v, json!({"outer": {"inner": "x"}}));
    }

    #[test]
    fn preserves_sibling_keys() {
        let mut v = json!({
            "keep": "me",
            "outer": {"keep": "also", "inner": 1},
        });
        set_at_path(&mut v, "outer.inner", json!(99));
        assert_eq!(
            v,
            json!({
                "keep": "me",
                "outer": {"keep": "also", "inner": 99},
            })
        );
    }

    #[test]
    fn value_types_other_than_object_can_be_stored() {
        let mut v = json!({});
        set_at_path(&mut v, "arr", json!([1, 2, 3]));
        set_at_path(&mut v, "s", json!("hello"));
        set_at_path(&mut v, "n", json!(42));
        set_at_path(&mut v, "b", json!(true));
        set_at_path(&mut v, "null", Value::Null);
        assert_eq!(
            v,
            json!({
                "arr": [1, 2, 3],
                "s": "hello",
                "n": 42,
                "b": true,
                "null": null,
            })
        );
    }
}
