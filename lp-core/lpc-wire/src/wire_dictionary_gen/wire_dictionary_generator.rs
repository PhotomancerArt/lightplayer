//! Ranks the traced wire names by the committed traffic sample, renders
//! `wire_dictionary.rs`, and judges a committed copy against it.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::string::String;
use std::vec::Vec;

use lp_json_pack::{DictionaryBuilder, OwnedDictionary};

use super::wire_name_tracer::{WireNames, trace_wire_names};

/// The committed traffic sample the ranking counts (provenance in the
/// fixture's README).
const SAMPLE: &str =
    include_str!("../../../../lp-base/lp-json-pack/tests/fixtures/choker-lens-sample.txt");

/// The codec that writes slot data, the one piece of the wire that is
/// pre-serialized JSON text (`WireSlotData`, a `RawValue`) rather than a
/// serde type. Its vocabulary is the `.prop("…")` keys and `.string("…")`
/// tags these files write, read straight out of their source, so it cannot
/// drift from them.
const SLOT_TEXT_CODEC: [(&str, &str); 3] = [
    (
        "lp-core/lpc-model/src/slot_sync_codec/snapshot_writer.rs",
        include_str!("../../../lpc-model/src/slot_sync_codec/snapshot_writer.rs"),
    ),
    (
        "lp-core/lpc-model/src/slot_codec/slot_value_codec.rs",
        include_str!("../../../lpc-model/src/slot_codec/slot_value_codec.rs"),
    ),
    (
        "lp-core/lpc-model/src/slot_codec/mod.rs",
        include_str!("../../../lpc-model/src/slot_codec/mod.rs"),
    ),
];

/// The sample's path, as the generated file names it.
const SAMPLE_PATH: &str = "lp-base/lp-json-pack/tests/fixtures/choker-lens-sample.txt";

/// A freshly generated wire dictionary.
#[derive(Debug, Clone)]
pub struct GeneratedWireDictionary {
    /// The tables, in code order.
    pub dictionary: OwnedDictionary,
    /// Their `Dictionary::fingerprint()`.
    pub fingerprint: u32,
    /// The `WIRE_PROTO_VERSION` it was generated at.
    pub proto: u32,
    /// Bytes the tables take as statics: text, offsets and hash tables.
    pub static_bytes: usize,
    /// How many keys and values the sample ranked (the rest follow by name).
    pub ranked_keys: usize,
    /// See [`GeneratedWireDictionary::ranked_keys`].
    pub ranked_values: usize,
    /// What the trace found.
    pub names: WireNames,
    /// Keys and values the slot text codec adds that the trace did not find.
    pub slot_text_names: usize,
    /// The Rust source of `wire_dictionary.rs`.
    pub source: String,
}

/// Trace the wire types, rank by the sample, and render.
pub fn generate_wire_dictionary(proto: u32) -> GeneratedWireDictionary {
    let names = trace_wire_names();
    let (slot_keys, slot_values) = slot_text_vocabulary();
    let mut keys_found = names.keys.clone();
    keys_found.extend(slot_keys.iter().copied());
    let mut values_found = names.values.clone();
    values_found.extend(slot_values.iter().copied());
    let mut builder = DictionaryBuilder::new();
    for line in SAMPLE.lines() {
        if let Some(json) = line.get(2..).and_then(|l| l.strip_prefix("M!")) {
            builder.observe_json(json.as_bytes());
        }
    }
    let seen = builder.build(1);
    let (keys, ranked_keys) = rank(&seen.keys, &keys_found);
    let (values, ranked_values) = rank(&seen.values, &values_found);
    let dictionary = OwnedDictionary::from_entries(&keys, &values);
    let fingerprint = dictionary.leak().fingerprint();
    let k = dictionary.key_layout();
    let v = dictionary.value_layout();
    let static_bytes = k.text.len()
        + v.text.len()
        + 2 * (k.offsets.len() + v.offsets.len() + k.hash.len() + v.hash.len());
    let mut generated = GeneratedWireDictionary {
        dictionary,
        fingerprint,
        proto,
        static_bytes,
        ranked_keys,
        ranked_values,
        slot_text_names: keys_found.len() + values_found.len()
            - names.keys.len()
            - names.values.len(),
        names,
        source: String::new(),
    };
    generated.source = render(&generated);
    generated
}

/// What a committed `wire_dictionary.rs` is, against a fresh generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireDictionaryVerdict {
    /// Byte for byte what the generator writes.
    Current,
    /// Different text; `just wire-dict` rewrites it.
    Stale,
    /// The dictionary itself changed but `WIRE_PROTO_VERSION` is still the
    /// one the committed file was generated at.
    NeedsProtoBump {
        /// The committed dictionary's fingerprint.
        committed: u32,
        /// The fresh one's.
        generated: u32,
    },
}

/// Judge the committed file (its text, its compiled dictionary's fingerprint
/// and its recorded proto) against a fresh generation.
pub fn judge_wire_dictionary(
    generated: &GeneratedWireDictionary,
    committed_source: &str,
    committed_fingerprint: u32,
    committed_proto: u32,
) -> WireDictionaryVerdict {
    if generated.fingerprint != committed_fingerprint && committed_proto == generated.proto {
        return WireDictionaryVerdict::NeedsProtoBump {
            committed: committed_fingerprint,
            generated: generated.fingerprint,
        };
    }
    if committed_source == generated.source {
        WireDictionaryVerdict::Current
    } else {
        WireDictionaryVerdict::Stale
    }
}

/// The slot text codec's keys and string tags ([`SLOT_TEXT_CODEC`]).
fn slot_text_vocabulary() -> (BTreeSet<&'static str>, BTreeSet<&'static str>) {
    let mut keys = BTreeSet::new();
    let mut values = BTreeSet::new();
    for (_, source) in SLOT_TEXT_CODEC {
        keys.extend(string_arguments(source, ".prop(\""));
        values.extend(string_arguments(source, ".string(\""));
    }
    (keys, values)
}

/// Every plain string literal passed right after `call` (which ends in the
/// literal's opening quote) in `source`.
fn string_arguments(source: &'static str, call: &str) -> impl Iterator<Item = &'static str> {
    source.match_indices(call).filter_map(move |(at, _)| {
        let rest = &source[at + call.len()..];
        let end = rest.find('"')?;
        let name = &rest[..end];
        let plain = !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.');
        (plain && rest[end..].starts_with("\")")).then_some(name)
    })
}

/// The traced names in the sample's frequency order, then the rest by name.
/// Returns the order and how many came from the sample.
fn rank(sample_order: &[String], traced: &BTreeSet<&'static str>) -> (Vec<String>, usize) {
    let mut out: Vec<String> = sample_order
        .iter()
        .filter(|s| traced.contains(s.as_str()))
        .cloned()
        .collect();
    let ranked = out.len();
    let placed: BTreeSet<String> = out.iter().cloned().collect();
    out.extend(
        traced
            .iter()
            .filter(|t| !placed.contains(**t))
            .map(|t| String::from(*t)),
    );
    (out, ranked)
}

fn render(g: &GeneratedWireDictionary) -> String {
    let d = &g.dictionary;
    let mut s = String::new();
    let w = &mut s;
    let _ = writeln!(
        w,
        "//! The wire's JSON Pack dictionary: what a packed frame is coded against.
//!
//! GENERATED by `just wire-dict`
//! (`cargo run -p lpc-wire --features wire-dict-gen --bin wire-dict`). Do not
//! edit: `just wire-dict-check` (in `check-lint`) fails when this file is not
//! what the generator writes, and when the dictionary changed without a
//! `WIRE_PROTO_VERSION` bump.
//!
//! - Keys: every struct field name and every data-carrying enum variant name
//!   reachable from `WireServerMessage` and `ClientMessage`, found by tracing
//!   their `Deserialize` impls (`wire_dictionary_gen`).
//! - Values: every unit variant name.
//! - Both: the keys and string tags of the slot text codec
//!   (`lpc-model`'s `slot_sync_codec` / `slot_codec` writers), which writes
//!   `WireSlotData`'s pre-serialized JSON.
//! - Order: by frequency in the committed traffic sample
//!   (`{SAMPLE_PATH}`),
//!   then by name. The commonest get the one-byte codes.
//!
//! {nk} keys ({rk} ranked by the sample), {nv} values ({rv} ranked); {bytes} bytes
//! of statics.

use lp_json_pack::{{Dictionary, PackStrings}};

/// The `WIRE_PROTO_VERSION` this dictionary was generated at.
pub const WIRE_DICTIONARY_PROTO: u32 = {proto};

/// [`WIRE_DICTIONARY`]'s `fingerprint()`, as generated.
pub const WIRE_DICTIONARY_FINGERPRINT: u32 = {fp:#010x};

/// The wire's dictionary.
pub static WIRE_DICTIONARY: Dictionary = Dictionary {{
    keys: KEYS,
    values: VALUES,
    key_hash: KEY_HASH,
    value_hash: VALUE_HASH,
}};",
        nk = d.keys.len(),
        rk = g.ranked_keys,
        nv = d.values.len(),
        rv = g.ranked_values,
        bytes = g.static_bytes,
        proto = g.proto,
        fp = g.fingerprint,
    );
    let k = d.key_layout();
    let v = d.value_layout();
    render_strings(w, "KEYS", &d.keys, &k.offsets);
    render_strings(w, "VALUES", &d.values, &v.offsets);
    render_u16s(w, "KEY_HASH", &k.hash);
    render_u16s(w, "VALUE_HASH", &v.hash);
    let _ = write!(
        w,
        "
#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn tables_are_well_formed_and_match_their_fingerprint() {{
        assert_eq!(WIRE_DICTIONARY.check(), Ok(()));
        assert_eq!(WIRE_DICTIONARY.fingerprint(), WIRE_DICTIONARY_FINGERPRINT);
    }}
}}
"
    );
    s
}

fn render_strings(w: &mut String, name: &str, entries: &[String], offsets: &[u16]) {
    let _ = writeln!(w, "\nconst {name}: PackStrings = PackStrings {{");
    let _ = writeln!(w, "    text: concat!(");
    for (i, e) in entries.iter().enumerate() {
        let _ = writeln!(w, "        {e:?}, // {i}");
    }
    let _ = writeln!(w, "    ),");
    let _ = writeln!(w, "    offsets: &[");
    for chunk in offsets.chunks(16) {
        let row: Vec<String> = chunk.iter().map(|o| std::format!("{o}")).collect();
        let _ = writeln!(w, "        {},", row.join(", "));
    }
    let _ = writeln!(w, "    ],");
    let _ = writeln!(w, "}};");
}

fn render_u16s(w: &mut String, name: &str, table: &[u16]) {
    let _ = writeln!(w, "\nconst {name}: &[u16] = &[");
    for chunk in table.chunks(16) {
        let row: Vec<String> = chunk.iter().map(|o| std::format!("{o:#06x}")).collect();
        let _ = writeln!(w, "    {},", row.join(", "));
    }
    let _ = writeln!(w, "];");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_puts_the_sample_first_then_names() {
        let traced: BTreeSet<&'static str> = ["a", "b", "c", "z"].into_iter().collect();
        let sample = [String::from("z"), String::from("q"), String::from("b")];
        let (order, ranked) = rank(&sample, &traced);
        assert_eq!(order, ["z", "b", "a", "c"]);
        assert_eq!(ranked, 2);
    }

    #[test]
    fn reads_the_slot_text_codec_vocabulary() {
        let (keys, values) = slot_text_vocabulary();
        for key in ["kind", "fields_revision", "changed_at", "value"] {
            assert!(keys.contains(key), "{key}");
        }
        for value in ["record", "value", "unit"] {
            assert!(values.contains(value), "{value}");
        }
        let src = r#"a.prop("x")?; b.prop(name)?; c.string("y-1")?; d.prop("a b")"#;
        assert_eq!(string_arguments(src, ".prop(\"").collect::<Vec<_>>(), ["x"]);
        assert_eq!(
            string_arguments(src, ".string(\"").collect::<Vec<_>>(),
            ["y-1"]
        );
    }

    #[test]
    fn a_changed_dictionary_needs_a_proto_bump() {
        let g = generate_wire_dictionary(crate::WIRE_PROTO_VERSION);
        let same = judge_wire_dictionary(&g, &g.source, g.fingerprint, g.proto);
        assert_eq!(same, WireDictionaryVerdict::Current);
        let edited = judge_wire_dictionary(&g, "// edited", g.fingerprint, g.proto);
        assert_eq!(edited, WireDictionaryVerdict::Stale);
        let unbumped = judge_wire_dictionary(&g, &g.source, g.fingerprint ^ 1, g.proto);
        assert!(matches!(
            unbumped,
            WireDictionaryVerdict::NeedsProtoBump { .. }
        ));
        let bumped = judge_wire_dictionary(&g, &g.source, g.fingerprint ^ 1, g.proto - 1);
        assert_eq!(bumped, WireDictionaryVerdict::Current);
    }

    #[test]
    fn the_committed_dictionary_is_current() {
        let g = generate_wire_dictionary(crate::WIRE_PROTO_VERSION);
        let committed = include_str!("../wire_dictionary.rs");
        assert_eq!(
            judge_wire_dictionary(
                &g,
                committed,
                crate::WIRE_DICTIONARY.fingerprint(),
                crate::WIRE_DICTIONARY_PROTO
            ),
            WireDictionaryVerdict::Current,
            "run `just wire-dict`"
        );
    }
}
