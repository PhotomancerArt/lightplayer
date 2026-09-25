//! One chip's record file: a flat JSON object, figure key → value.
//!
//! The layout is fixed by [`Record::render`] and nothing else, so the file is
//! **machine-writable**: a bless, a CI job emitting a patch, or a script all
//! produce byte-identical files for identical figures. Keys are sorted
//! (byte order), two-space indent, one figure per line — except a text
//! figure, which is one line per line of text, so a moved boot line is a
//! one-line diff. Keys beginning with `_` are prose: kept, never checked.

use std::collections::BTreeMap;
use std::fmt;

/// A figure's recorded value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// A count, a size, an address, a signed gap.
    Int(i64),
    /// A digest or a one-line string.
    Str(String),
    /// A byte stream that is text, split on `\n` (so `"a\n"` is `["a", ""]`).
    Text(Vec<String>),
}

impl Value {
    /// A text figure from its whole string.
    pub fn text(s: &str) -> Self {
        Value::Text(s.split('\n').map(str::to_owned).collect())
    }

    fn render(&self, out: &mut String) {
        match self {
            Value::Int(n) => out.push_str(&n.to_string()),
            Value::Str(s) => out.push_str(&quote(s)),
            Value::Text(lines) => {
                out.push_str("[\n");
                for (i, line) in lines.iter().enumerate() {
                    out.push_str("    ");
                    out.push_str(&quote(line));
                    if i + 1 < lines.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                out.push_str("  ]");
            }
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Str(s) => write!(f, "{}", quote(s)),
            Value::Text(lines) => write!(f, "<{} lines of text>", lines.len()),
        }
    }
}

/// A parsed record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
    /// Every key, prose (`_…`) included, in the order they are written.
    pub entries: BTreeMap<String, Value>,
}

impl Record {
    /// Parse a record file's text. An empty or whitespace-only file is an
    /// empty record.
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        let json: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
        let serde_json::Value::Object(map) = json else {
            return Err("the record is not a JSON object".into());
        };
        let mut entries = BTreeMap::new();
        for (key, value) in map {
            let value = match value {
                serde_json::Value::Number(n) => Value::Int(
                    n.as_i64()
                        .ok_or_else(|| format!("`{key}`: {n} is not an integer figure"))?,
                ),
                serde_json::Value::String(s) => Value::Str(s),
                serde_json::Value::Array(items) => Value::Text(
                    items
                        .into_iter()
                        .map(|item| match item {
                            serde_json::Value::String(s) => Ok(s),
                            other => Err(format!("`{key}`: a text line is not a string: {other}")),
                        })
                        .collect::<Result<_, _>>()?,
                ),
                other => return Err(format!("`{key}`: not a figure value: {other}")),
            };
            entries.insert(key, value);
        }
        Ok(Self { entries })
    }

    /// The one layout the file is ever written in.
    pub fn render(&self) -> String {
        let mut out = String::from("{\n");
        let n = self.entries.len();
        for (i, (key, value)) in self.entries.iter().enumerate() {
            out.push_str("  ");
            out.push_str(&quote(key));
            out.push_str(": ");
            value.render(&mut out);
            if i + 1 < n {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("}\n");
        out
    }

    /// A figure's recorded value.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entries.get(key)
    }
}

fn quote(s: &str) -> String {
    serde_json::to_string(s).expect("a string always serializes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_is_sorted_one_figure_per_line_and_round_trips() {
        let mut r = Record::default();
        r.entries.insert("z.cycles".into(), Value::Int(3_251_009));
        r.entries
            .insert("_about".into(), Value::Str("prose — kept".into()));
        r.entries
            .insert("a.chain".into(), Value::text("one\ntwo \"q\"\n"));
        r.entries.insert("m.gap".into(), Value::Int(-96));
        let text = r.render();
        assert_eq!(
            text,
            "{\n  \"_about\": \"prose — kept\",\n  \"a.chain\": [\n    \"one\",\n    \
             \"two \\\"q\\\"\",\n    \"\"\n  ],\n  \"m.gap\": -96,\n  \"z.cycles\": 3251009\n}\n"
        );
        assert_eq!(Record::parse(&text).unwrap(), r);
        assert_eq!(Record::parse(&text).unwrap().render(), text);
    }

    #[test]
    fn an_empty_file_is_an_empty_record() {
        assert_eq!(Record::parse("").unwrap(), Record::default());
        assert_eq!(Record::default().render(), "{\n}\n");
    }

    #[test]
    fn a_non_integer_number_is_refused() {
        assert!(Record::parse("{\"x\": 1.5}").is_err());
        assert!(Record::parse("{\"x\": [1]}").is_err());
        assert!(Record::parse("[]").is_err());
    }
}
