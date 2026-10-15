//! The product's patch document (`*.patch.json`), re-created: where a
//! fixture's lamps land on an output's wire. A copy of the shapes in
//! `lpc-mapping`'s `patch.rs`, not an import (the fence).
//!
//! - **Format 1**: keyed entries, `{"range": {"start", "count"?}, "at":
//!   {"channel"}, "reversed"?}` — fixture lamp ranges. Read, and written
//!   only when that is all a document says.
//! - **Format 2**: rows `[from, out#, lamp, flags?, offset?]` with an
//!   `outputs` table of output names (`-1` = the default output). `from`
//!   is an object path (`/sector/2`), `[path, start, count?]`, or
//!   `[start, count?]`.
//! - **Format 3**: format 2 plus `"flow": "manual"` (lamps no row places
//!   are dark).
//!
//! A document newer than format 3 is refused, as the product refuses it.
//! Writing stamps the lowest format that holds the content, in the
//! product's canonical layout (one row per line).

use serde_json::{Map, Value};

use crate::json::u32_of;

pub const PATCH_FORMAT: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchDoc {
    pub manual: bool,
    pub entries: Vec<PatchEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchEntry {
    pub source: PatchSource,
    /// The output's name; `None` = the default output.
    pub output: Option<String>,
    /// Where the run starts on the output's flat wire, in lamps.
    pub lamp: u32,
    pub reversed: bool,
    /// Rotation within the run's window, in lamps.
    pub offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchSource {
    /// An object path, perhaps narrowed to some of its lamps.
    Path {
        path: ObjectPath,
        range: Option<(u32, Option<u32>)>,
    },
    /// Fixture lamps `start`, `count` (to the end when `None`).
    Range(u32, Option<u32>),
}

/// `/id` or `/id/k/…`: an object by its stable id, then repeat copies.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ObjectPath {
    pub id: String,
    pub instances: Vec<u32>,
}

impl ObjectPath {
    pub fn parse(text: &str) -> Option<ObjectPath> {
        let rest = text.strip_prefix('/')?;
        let mut segments = rest.split('/');
        let id = segments.next()?.to_string();
        if !valid_object_id(&id) {
            return None;
        }
        let mut instances = Vec::new();
        for s in segments {
            let k: u32 = s.parse().ok()?;
            if s != k.to_string() {
                return None;
            }
            instances.push(k);
        }
        Some(ObjectPath { id, instances })
    }

    pub fn text(&self) -> String {
        let mut s = format!("/{}", self.id);
        for k in &self.instances {
            s.push_str(&format!("/{k}"));
        }
        s
    }
}

/// The product's object id: `[a-z][a-z0-9_-]*`, at most 24 bytes.
pub fn valid_object_id(id: &str) -> bool {
    let mut chars = id.chars();
    id.len() <= 24
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

impl PatchDoc {
    pub fn parse(text: &str) -> Result<PatchDoc, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let format = v.get("format").and_then(u32_of).ok_or("no format number")?;
        if format == 0 || format > PATCH_FORMAT {
            return Err(format!(
                "patch format {format} is newer than this copy reads (up to {PATCH_FORMAT})"
            ));
        }
        let rows = match v.get("entries") {
            None => Vec::new(),
            Some(Value::Array(rows)) => rows.clone(),
            Some(_) => return Err("\"entries\" is not a list".into()),
        };
        if format == 1 {
            let entries = rows.iter().map(parse_v1).collect::<Result<_, _>>()?;
            return Ok(PatchDoc {
                manual: false,
                entries,
            });
        }
        let manual = match v.get("flow").and_then(Value::as_str) {
            None | Some("auto") => false,
            Some("manual") if format >= 3 => true,
            Some(other) => return Err(format!("flow {other:?} is not auto or manual")),
        };
        let outputs: Vec<String> = match v.get("outputs") {
            None => Vec::new(),
            Some(Value::Array(names)) => names
                .iter()
                .map(|n| {
                    n.as_str()
                        .map(str::to_string)
                        .ok_or("output names are text")
                })
                .collect::<Result<_, _>>()?,
            Some(_) => return Err("\"outputs\" is not a list".into()),
        };
        let entries = rows
            .iter()
            .enumerate()
            .map(|(i, row)| parse_row(row, &outputs).map_err(|e| format!("row {i}: {e}")))
            .collect::<Result<_, _>>()?;
        Ok(PatchDoc { manual, entries })
    }

    /// The lowest format that holds this document.
    pub fn required_format(&self) -> u32 {
        if self.manual {
            return 3;
        }
        let grain = self.entries.iter().any(|e| {
            matches!(e.source, PatchSource::Path { .. }) || e.output.is_some() || e.offset != 0
        });
        if grain { 2 } else { 1 }
    }

    /// The product's canonical text for this document (`to_json_pretty`),
    /// with a trailing newline.
    pub fn text(&self) -> String {
        let format = self.required_format();
        if format == 1 {
            let entries: Vec<Value> = self.entries.iter().map(v1_entry).collect();
            let mut doc = Map::new();
            doc.insert("format".into(), 1.into());
            doc.insert("entries".into(), Value::Array(entries));
            return crate::json::pretty(&Value::Object(doc));
        }
        let mut outputs: Vec<String> = Vec::new();
        let rows: Vec<Value> = self.entries.iter().map(|e| row(e, &mut outputs)).collect();
        let mut text = format!("{{\n  \"format\": {format},\n");
        if format >= 3 {
            let flow = if self.manual { "manual" } else { "auto" };
            text.push_str(&format!("  \"flow\": \"{flow}\",\n"));
        }
        if !outputs.is_empty() {
            let names = serde_json::to_string(&outputs).unwrap_or_default();
            text.push_str(&format!("  \"outputs\": {names},\n"));
        }
        if rows.is_empty() {
            text.push_str("  \"entries\": []\n}\n");
            return text;
        }
        text.push_str("  \"entries\": [\n");
        for (i, r) in rows.iter().enumerate() {
            text.push_str("    ");
            text.push_str(&serde_json::to_string(r).unwrap_or_default());
            text.push_str(if i + 1 < rows.len() { ",\n" } else { "\n" });
        }
        text.push_str("  ]\n}\n");
        text
    }
}

fn parse_v1(v: &Value) -> Result<PatchEntry, String> {
    let range = v.get("range").ok_or("an entry has no range")?;
    let start = range.get("start").and_then(u32_of).ok_or("range.start")?;
    let count = range.get("count").and_then(u32_of);
    let lamp = v
        .get("at")
        .and_then(|a| a.get("channel"))
        .and_then(u32_of)
        .ok_or("at.channel")?;
    Ok(PatchEntry {
        source: PatchSource::Range(start, count),
        output: None,
        lamp,
        reversed: v.get("reversed").and_then(Value::as_bool).unwrap_or(false),
        offset: 0,
    })
}

fn v1_entry(e: &PatchEntry) -> Value {
    let (start, count) = match e.source {
        PatchSource::Range(start, count) => (start, count),
        PatchSource::Path { .. } => (0, None),
    };
    let mut range = Map::new();
    range.insert("start".into(), start.into());
    if let Some(c) = count {
        range.insert("count".into(), c.into());
    }
    let mut at = Map::new();
    at.insert("channel".into(), e.lamp.into());
    let mut out = Map::new();
    out.insert("range".into(), Value::Object(range));
    out.insert("at".into(), Value::Object(at));
    if e.reversed {
        out.insert("reversed".into(), true.into());
    }
    Value::Object(out)
}

fn parse_row(row: &Value, outputs: &[String]) -> Result<PatchEntry, String> {
    let cols = row.as_array().ok_or("a row is a list")?;
    if !(3..=5).contains(&cols.len()) {
        return Err("a row has 3 to 5 columns".into());
    }
    let source = match &cols[0] {
        Value::String(t) => PatchSource::Path {
            path: ObjectPath::parse(t).ok_or("a bad object path")?,
            range: None,
        },
        Value::Array(parts) => match parts.first() {
            Some(Value::String(t)) => PatchSource::Path {
                path: ObjectPath::parse(t).ok_or("a bad object path")?,
                range: Some(range_of(&parts[1..]).ok_or("a bad sub-span")?),
            },
            Some(_) => {
                let (s, c) = range_of(parts).ok_or("a bad range")?;
                PatchSource::Range(s, c)
            }
            None => return Err("an empty source".into()),
        },
        _ => return Err("a bad source".into()),
    };
    let out = cols[1].as_i64().ok_or("out# is a number")?;
    let output = if out == -1 {
        None
    } else {
        Some(
            usize::try_from(out)
                .ok()
                .and_then(|i| outputs.get(i))
                .ok_or("out# is past the outputs table")?
                .clone(),
        )
    };
    let lamp = u32_of(&cols[2]).ok_or("lamp is a whole number")?;
    let reversed = match cols.get(3).map(|v| v.as_str()) {
        None | Some(Some("")) => false,
        Some(Some("r")) => true,
        _ => return Err("flags are \"\" or \"r\"".into()),
    };
    let offset = match cols.get(4) {
        None => 0,
        Some(v) => u32_of(v).ok_or("offset is a whole number")?,
    };
    Ok(PatchEntry {
        source,
        output,
        lamp,
        reversed,
        offset,
    })
}

fn range_of(parts: &[Value]) -> Option<(u32, Option<u32>)> {
    match parts {
        [s] => Some((u32_of(s)?, None)),
        [s, c] => Some((u32_of(s)?, Some(u32_of(c)?))),
        _ => None,
    }
}

fn row(e: &PatchEntry, outputs: &mut Vec<String>) -> Value {
    let out: i64 = match &e.output {
        None => -1,
        Some(name) => match outputs.iter().position(|n| n == name) {
            Some(i) => i as i64,
            None => {
                outputs.push(name.clone());
                outputs.len() as i64 - 1
            }
        },
    };
    let range = |start: u32, count: Option<u32>| {
        let mut v = vec![Value::from(start)];
        if let Some(c) = count {
            v.push(c.into());
        }
        v
    };
    let from = match &e.source {
        PatchSource::Path { path, range: None } => Value::String(path.text()),
        PatchSource::Path {
            path,
            range: Some((s, c)),
        } => {
            let mut v = vec![Value::String(path.text())];
            v.extend(range(*s, *c));
            Value::Array(v)
        }
        PatchSource::Range(s, c) => Value::Array(range(*s, *c)),
    };
    let mut cols = vec![from, out.into(), e.lamp.into()];
    if e.reversed || e.offset != 0 {
        cols.push(if e.reversed { "r" } else { "" }.into());
    }
    if e.offset != 0 {
        cols.push(e.offset.into());
    }
    Value::Array(cols)
}
