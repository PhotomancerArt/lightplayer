//! Ordered JSON, printed the way the product's writers print it.
//!
//! The product writes node files and map2d documents with serde_json's
//! pretty printer (two-space indent, one array element per line) over
//! structs whose field order is fixed. This crate may not depend on the
//! product's types, and `serde_json::Value` sorts object keys, so it builds
//! an ordered tree and prints it with the same layout.

/// A JSON value whose objects keep their insertion order.
pub enum Json {
    Int(i64),
    /// Printed through serde_json, so `60.0` stays `60.0`. Callers round
    /// first; a value with at most six significant digits prints the same
    /// as the product's `f32` writer would print it.
    Num(f64),
    Bool(bool),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn str(text: &str) -> Self {
        Self::Str(text.to_string())
    }

    pub fn obj<const N: usize>(fields: [(&str, Json); N]) -> Self {
        Self::Obj(
            fields
                .into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect(),
        )
    }

    /// serde_json's pretty form, with no trailing newline.
    pub fn pretty(&self) -> String {
        let mut out = String::new();
        write_pretty(self, 0, &mut out);
        out
    }
}

/// A JSON string literal.
pub fn quoted(text: &str) -> String {
    serde_json::to_string(text).expect("a string serializes")
}

/// A float as serde_json prints it.
pub fn number(value: f64) -> String {
    // `+ 0.0` turns a rounded `-0.0` into `0.0`.
    serde_json::to_string(&(value + 0.0)).expect("a finite float serializes")
}

fn write_pretty(value: &Json, depth: usize, out: &mut String) {
    match value {
        Json::Int(n) => out.push_str(&n.to_string()),
        Json::Num(x) => out.push_str(&number(*x)),
        Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Json::Str(s) => out.push_str(&quoted(s)),
        Json::Arr(items) if items.is_empty() => out.push_str("[]"),
        Json::Obj(fields) if fields.is_empty() => out.push_str("{}"),
        Json::Arr(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                out.push_str(if index == 0 { "\n" } else { ",\n" });
                indent(depth + 1, out);
                write_pretty(item, depth + 1, out);
            }
            out.push('\n');
            indent(depth, out);
            out.push(']');
        }
        Json::Obj(fields) => {
            out.push('{');
            for (index, (key, item)) in fields.iter().enumerate() {
                out.push_str(if index == 0 { "\n" } else { ",\n" });
                indent(depth + 1, out);
                out.push_str(&quoted(key));
                out.push_str(": ");
                write_pretty(item, depth + 1, out);
            }
            out.push('\n');
            indent(depth, out);
            out.push('}');
        }
    }
}

fn indent(depth: usize, out: &mut String) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prints_like_serde_json_pretty() {
        // Keys in alphabetical order, so serde_json's sorted map agrees.
        let value = Json::obj([
            ("at", Json::Arr(vec![Json::Num(60.0), Json::Num(107.7)])),
            ("count", Json::Int(80)),
            ("empty", Json::Arr(vec![])),
            ("kind", Json::str("Output")),
            ("on", Json::Bool(false)),
            ("ports", Json::obj([("0", Json::obj([]))])),
        ]);
        let expected = serde_json::to_string_pretty(&serde_json::json!({
            "at": [60.0, 107.7],
            "count": 80,
            "empty": [],
            "kind": "Output",
            "on": false,
            "ports": { "0": {} },
        }))
        .unwrap();
        assert_eq!(value.pretty(), expected);
        assert_eq!(number(-0.0), "0.0");
    }
}
