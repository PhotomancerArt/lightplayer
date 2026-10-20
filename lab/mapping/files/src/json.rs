//! Small JSON helpers: numbers the way the product writes them, equality
//! the way the product reads it, and the product's pretty layout.
//!
//! The product parses every coordinate as f32, so two documents mean the
//! same thing when their numbers agree **as f32** — `132.41` and
//! `132.410000` are one value. Writing goes the other way: an f32 is
//! written in its shortest form (`132.41`, not `132.41000366210938`), and a
//! number that did not change keeps the exact JSON it was read from, so a
//! file the lab did not change is never rewritten at all.

use serde_json::{Map, Number, Value};

/// A number for the file: the old one when it still means `new` (as f32),
/// else `new` in its shortest f32 form.
pub fn num_f32(old: Option<&Value>, new: f64) -> Value {
    if let Some(v) = old
        && let Some(x) = v.as_f64()
        && x as f32 == new as f32
    {
        return v.clone();
    }
    f32_value(new as f32)
}

/// An f32 as the product's serde writes it: shortest round-trip digits.
pub fn f32_value(x: f32) -> Value {
    let text = format!("{x}");
    match text.parse::<f64>().ok().and_then(Number::from_f64) {
        Some(n) => Value::Number(n),
        None => Value::Null,
    }
}

/// An `[x, y]` point, keeping the old entries that still mean the same.
pub fn point_f32(old: Option<&Value>, x: f64, y: f64) -> Value {
    let at = |i: usize| old.and_then(|v| v.get(i));
    Value::Array(vec![num_f32(at(0), x), num_f32(at(1), y)])
}

/// A list of points, keeping old points where they still mean the same.
pub fn points_f32(old: Option<&Value>, points: impl Iterator<Item = (f64, f64)>) -> Value {
    Value::Array(
        points
            .enumerate()
            .map(|(i, (x, y))| point_f32(old.and_then(|v| v.get(i)), x, y))
            .collect(),
    )
}

/// An `[x, y]` from the file, as the product reads it (f32, widened).
pub fn read_point(v: &Value) -> Option<(f64, f64)> {
    let a = v.as_array()?;
    match a.as_slice() {
        [x, y] => Some((f32_of(x)?, f32_of(y)?)),
        _ => None,
    }
}

/// A number from the file, rounded to f32 the way the product reads it.
pub fn f32_of(v: &Value) -> Option<f64> {
    Some(v.as_f64()? as f32 as f64)
}

pub fn u32_of(v: &Value) -> Option<u32> {
    v.as_u64().and_then(|n| u32::try_from(n).ok())
}

/// Do two documents mean the same? Numbers compare as f32; everything
/// else exactly. Key order does not matter (the product reads into maps).
pub fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(x), Some(y)) => x as f32 == y as f32,
            _ => x == y,
        },
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

/// The product's pretty layout: two-space indent, one trailing newline
/// (what `NodeDef::write_json` writes).
///
/// The layout is serde_json's pretty one, but the key order is this
/// crate's: every object's `kind` comes first (the product's node reader
/// reads its tag before anything else), then the rest in sorted order —
/// serde_json's own map order, which this crate never changes (its
/// `preserve_order` feature would flip it for every crate in Studio's
/// build).
pub fn pretty(v: &Value) -> String {
    let mut text = String::new();
    write_pretty(v, 0, &mut text);
    text.push('\n');
    text
}

fn write_pretty(v: &Value, depth: usize, out: &mut String) {
    let indent = |out: &mut String, depth: usize| out.push_str(&"  ".repeat(depth));
    match v {
        Value::Object(map) if map.is_empty() => out.push_str("{}"),
        Value::Object(map) => {
            out.push_str("{\n");
            let kind = map.get_key_value("kind");
            let rest = map.iter().filter(|(k, _)| k.as_str() != "kind");
            for (i, (k, v)) in kind.into_iter().chain(rest).enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                indent(out, depth + 1);
                out.push_str(&serde_json::to_string(k).unwrap_or_default());
                out.push_str(": ");
                write_pretty(v, depth + 1, out);
            }
            out.push('\n');
            indent(out, depth);
            out.push('}');
        }
        Value::Array(items) if items.is_empty() => out.push_str("[]"),
        Value::Array(items) => {
            out.push_str("[\n");
            for (i, v) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                indent(out, depth + 1);
                write_pretty(v, depth + 1, out);
            }
            out.push('\n');
            indent(out, depth);
            out.push(']');
        }
        scalar => out.push_str(&serde_json::to_string(scalar).unwrap_or_default()),
    }
}

/// Set `key` on an object, or remove it for `None`.
pub fn set(obj: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    match value {
        Some(v) => {
            obj.insert(key.to_string(), v);
        }
        None => {
            obj.remove(key);
        }
    }
}

/// Insert `key` keeping a map's keys sorted (the product writes its node
/// and port maps sorted). An existing key keeps its place.
pub fn insert_sorted(obj: &mut Map<String, Value>, key: &str, value: Value, numeric: bool) {
    if obj.contains_key(key) {
        obj.insert(key.to_string(), value);
        return;
    }
    let mut entries: Vec<(String, Value)> = std::mem::take(obj).into_iter().collect();
    entries.push((key.to_string(), value));
    if numeric {
        entries.sort_by_key(|(k, _)| k.parse::<u64>().unwrap_or(u64::MAX));
    } else {
        entries.sort_by(|a, b| a.0.cmp(&b.0));
    }
    obj.extend(entries);
}
