//! Dotted-path walkers over `serde_json::Value`.
//!
//! The three-way diff produces changelog paths like `"pricing.amount"`
//! (dotted object keys). Object keys that themselves contain a literal `.`
//! or `\` are escaped per [`escape_segment`] before joining, so a key like
//! `"a.b"` becomes the path segment `"a\.b"` — indistinguishable from a
//! *real* path separator only if you don't run it back through
//! [`split_path`], which undoes the escaping. This module provides the
//! inverse operation: given a path and a value, set the leaf at that path,
//! creating any missing intermediate objects.
//!
//! Paths do not support array indexing — the diff primitive emits array
//! mismatches as a single change at the array's own path, so there's no
//! `"items.0.sku"` style input to handle here.

use serde_json::{Map, Value};

/// Escape a single path segment so a literal `.` or `\` inside it survives
/// [`split_path`] as part of the segment instead of being read as a
/// separator or escape character. Order matters: backslashes are escaped
/// first, then dots — otherwise a dot-escape's own backslash would itself
/// get re-escaped.
pub(crate) fn escape_segment(s: &str) -> String {
    s.replace('\\', "\\\\").replace('.', "\\.")
}

/// Split a path produced by joining [`escape_segment`]-escaped segments
/// with `.` back into its original segments. Scans left to right: `\`
/// consumes the next character literally (whatever it is) into the current
/// segment, an unescaped `.` ends the current segment, and a trailing lone
/// `\` is kept as a literal backslash. Never errors — every input has a
/// well-defined split.
pub(crate) fn split_path(path: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = path.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => current.push(chars.next().unwrap_or('\\')),
            '.' => parts.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    parts.push(current);
    parts
}

/// Set `new_value` at the dotted `path` inside `target`. Intermediate
/// objects are created when missing; non-object values along the path
/// are replaced with a fresh empty object so descent can continue.
///
/// An empty path is a no-op.
pub fn set_at_path(target: &mut Value, path: &str, new_value: Value) {
    if path.is_empty() {
        return;
    }
    let parts = split_path(path);
    let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
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
    fn plain_segments_round_trip_like_split_on_dot() {
        let joined = format!("{}.{}", escape_segment("a"), escape_segment("b"));
        assert_eq!(joined, "a.b");
        assert_eq!(split_path(&joined), vec!["a", "b"]);
    }

    #[test]
    fn dotted_key_round_trips_through_escape_and_split() {
        let key = "key_2025-01-01T00:00:00.000Z_demo";
        let joined = format!("items.{}.name", escape_segment(key));
        assert_eq!(joined, "items.key_2025-01-01T00:00:00\\.000Z_demo.name");
        assert_eq!(split_path(&joined), vec!["items", key, "name"]);
    }

    #[test]
    fn backslash_key_round_trips() {
        let key = r"weird\key";
        let joined = escape_segment(key);
        assert_eq!(joined, r"weird\\key");
        assert_eq!(split_path(&joined), vec![key]);
    }

    #[test]
    fn trailing_lone_backslash_is_kept_literal_deterministically() {
        assert_eq!(split_path(r"a\"), vec!["a\\"]);
    }

    #[test]
    fn set_at_path_lands_in_real_dotted_key_not_phantom_branch() {
        let mut v = json!({"items": {"key_2025-01-01T00:00:00.000Z_demo": {"name": "old"}}});
        let path = format!(
            "items.{}.name",
            escape_segment("key_2025-01-01T00:00:00.000Z_demo")
        );
        set_at_path(&mut v, &path, json!("new"));
        assert_eq!(
            v,
            json!({"items": {"key_2025-01-01T00:00:00.000Z_demo": {"name": "new"}}})
        );
    }

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
