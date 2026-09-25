//! HOST-ONLY (feature `wire-dict-gen`): generates `wire_dictionary.rs`.
//!
//! `just wire-dict` regenerates it; `just wire-dict-check` (in `check-lint`)
//! fails on drift. No firmware crate enables the feature, and it pulls no
//! third-party crate: the name tracer is hand-rolled (plan
//! `lp2025/2026-09-23-1701-lp-json-pack`, Q3).

pub mod wire_dictionary_generator;
pub mod wire_name_tracer;

pub use wire_dictionary_generator::{
    GeneratedWireDictionary, WireDictionaryVerdict, generate_wire_dictionary, judge_wire_dictionary,
};
pub use wire_name_tracer::{WireNames, trace_wire_names};
