//! `just wire-dict` / `just wire-dict-check`: regenerate the wire's JSON Pack
//! dictionary (`lpc-wire/src/wire_dictionary.rs`), or check it for drift.
//!
//! Host-only (feature `wire-dict-gen`).

use std::process::ExitCode;

use lpc_wire::wire_dictionary_gen::{
    WireDictionaryVerdict, generate_wire_dictionary, judge_wire_dictionary,
};
use lpc_wire::{WIRE_DICTIONARY, WIRE_DICTIONARY_PROTO, WIRE_PROTO_VERSION};

const DICTIONARY_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/wire_dictionary.rs");
const SHOWN_PATH: &str = "lp-core/lpc-wire/src/wire_dictionary.rs";

fn main() -> ExitCode {
    let check = match std::env::args().nth(1).as_deref() {
        None => false,
        Some("--check") => true,
        Some(other) => {
            eprintln!("usage: wire-dict [--check]  (unknown argument {other:?})");
            return ExitCode::from(2);
        }
    };
    let generated = generate_wire_dictionary(WIRE_PROTO_VERSION);
    let committed = std::fs::read_to_string(DICTIONARY_PATH).unwrap_or_default();
    let verdict = judge_wire_dictionary(
        &generated,
        &committed,
        WIRE_DICTIONARY.fingerprint(),
        WIRE_DICTIONARY_PROTO,
    );
    let summary = format!(
        "{} keys, {} values, {} bytes of statics, fingerprint {:#010x}, proto {} \
         ({} structs, {} enums traced in {} runs; unreached: {:?})",
        generated.dictionary.keys.len(),
        generated.dictionary.values.len(),
        generated.static_bytes,
        generated.fingerprint,
        generated.proto,
        generated.names.structs,
        generated.names.enums,
        generated.names.runs,
        generated.names.unreached,
    );
    match verdict {
        WireDictionaryVerdict::NeedsProtoBump {
            committed,
            generated: fresh,
        } => {
            eprintln!(
                "error: the wire dictionary changed (fingerprint {committed:#010x} -> {fresh:#010x}) \
                 but WIRE_PROTO_VERSION is still {WIRE_PROTO_VERSION}, the version {SHOWN_PATH} \
                 was generated at.\n\
                 A dictionary change is a wire change: bump `WIRE_PROTO_VERSION` \
                 (lp-core/lpc-wire/src/server/hello.rs), then run `just wire-dict`."
            );
            ExitCode::FAILURE
        }
        WireDictionaryVerdict::Current => {
            println!("wire dictionary current: {summary}");
            ExitCode::SUCCESS
        }
        WireDictionaryVerdict::Stale if check => {
            eprintln!(
                "error: {SHOWN_PATH} is not what the generator writes: run `just wire-dict`.\n\
                 (generated: {summary})"
            );
            ExitCode::FAILURE
        }
        WireDictionaryVerdict::Stale => match std::fs::write(DICTIONARY_PATH, &generated.source) {
            Ok(()) => {
                println!("wrote {SHOWN_PATH}: {summary}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: writing {SHOWN_PATH}: {e}");
                ExitCode::FAILURE
            }
        },
    }
}
