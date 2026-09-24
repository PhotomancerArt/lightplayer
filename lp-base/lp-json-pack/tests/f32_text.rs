//! f32 exactness: a value printed the way `ser-write-json` prints it
//! (`ryu-js`, JS layout) packs and decodes to identical text, and that text
//! parses back to identical bits.
//!
//! The committed subset takes every 7,919th bit pattern (~542 k values); the
//! full every-7th sweep (~614 M values) is `#[ignore]`d:
//! `cargo test -p lp-json-pack --features lex --release --test f32_text -- --ignored`.

use lp_json_pack::{Dictionary, PackEncoder, SliceJsonOut, decode, pack_tags};

#[test]
fn every_7919th_f32_round_trips_text_and_bits() {
    let counts = sweep(7919);
    // Most values take the decimal form; the escape stays rare.
    assert!(counts.decimal > counts.text * 100, "{counts:?}");
}

#[test]
#[ignore = "614 M values; run in release"]
fn every_7th_f32_round_trips_text_and_bits() {
    sweep(7);
}

#[derive(Debug, Default)]
struct Counts {
    checked: u64,
    decimal: u64,
    int: u64,
    text: u64,
}

fn sweep(stride: u64) -> Counts {
    let mut counts = Counts::default();
    let mut packed = [0u8; 64];
    let mut json = [0u8; 64];
    let mut bits: u64 = 0;
    while bits <= u64::from(u32::MAX) {
        let v = f32::from_bits(bits as u32);
        bits += stride;
        if !v.is_finite() {
            continue; // ser-write-json writes null
        }
        let mut rb = ryu_js::Buffer::new();
        let text = rb.format_finite(v);
        let mut enc = PackEncoder::new(&mut packed, &Dictionary::EMPTY);
        enc.decimal_text(text).unwrap();
        let n = enc.finish().unwrap();
        match packed[0] {
            pack_tags::DECIMAL_POS | pack_tags::DECIMAL_NEG => counts.decimal += 1,
            pack_tags::NUMBER_TEXT => counts.text += 1,
            _ => counts.int += 1,
        }
        let mut out = SliceJsonOut::new(&mut json);
        decode(&Dictionary::EMPTY, &packed[..n], &mut out).unwrap();
        assert_eq!(
            out.as_bytes(),
            text.as_bytes(),
            "bits {:#010x}",
            v.to_bits()
        );
        let parsed: f32 = text.parse().unwrap();
        assert_eq!(parsed.to_bits(), v.to_bits(), "{text}");

        // The lexer path agrees with the event path.
        let mut again = [0u8; 64];
        let mut enc = PackEncoder::new(&mut again, &Dictionary::EMPTY);
        enc.json_value(text.as_bytes()).unwrap();
        assert_eq!(enc.finish(), Ok(n));
        assert_eq!(again[..n], packed[..n]);
        counts.checked += 1;
    }
    counts
}
