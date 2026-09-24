//! Every line of the committed traffic sample survives JSON → packed → JSON
//! byte for byte, on the lexer path and on the event path, and a too-small
//! buffer is always `Full`, never a panic.

#[path = "support/event_driver.rs"]
mod event_driver;
#[path = "support/fixture_dictionary.rs"]
mod fixture_dictionary;

use fixture_dictionary::{fixture_dictionary, fixture_lines};
use lp_json_pack::{Dictionary, PackEncoder, PackError, PackLexer, decode};

#[test]
fn lexer_round_trips_every_line_in_uneven_slices() {
    let dict = fixture_dictionary();
    let mut packed = vec![0u8; 1 << 16];
    let (mut json_total, mut packed_total) = (0, 0);
    let mut step = 1;
    let lines = fixture_lines();
    assert!(lines.iter().any(|l| l.dir == b'<') && lines.iter().any(|l| l.dir == b'>'));
    for (i, line) in lines.iter().enumerate() {
        let mut enc = PackEncoder::new(&mut packed, dict);
        let mut lexer = PackLexer::new();
        let mut at = 0;
        while at < line.json.len() {
            let end = (at + step).min(line.json.len());
            lexer
                .push(&mut enc, &line.json[at..end])
                .unwrap_or_else(|e| panic!("line {i}: {e:?}"));
            at = end;
            step = step % 23 + 1;
        }
        lexer.finish(&mut enc).unwrap();
        let n = enc.finish().unwrap();
        assert_round_trip(i, dict, &packed[..n], line.json);
        json_total += line.json.len();
        packed_total += n;
    }
    // Real traffic packs to under 40 % of its JSON even with the base64 left
    // as text (the lexer path; blobs are the event path's). The post-lean-wire
    // sample (2026-09-23): 135,131 → 51,902 B, 38.4 %. Lean replies resend
    // less structure, so there is less for the dictionary to fold than in the
    // pre-lean-wire sample's 4.0×.
    assert!(
        packed_total * 5 < json_total * 2,
        "{packed_total} vs {json_total}"
    );
}

#[test]
fn event_path_packs_exactly_what_the_lexer_packs() {
    let dict = fixture_dictionary();
    let mut by_lexer = vec![0u8; 1 << 16];
    let mut by_events = vec![0u8; 1 << 16];
    for (i, line) in fixture_lines().iter().enumerate() {
        let mut enc = PackEncoder::new(&mut by_lexer, dict);
        enc.json_value(line.json).unwrap();
        let a = enc.finish().unwrap();
        let mut enc = PackEncoder::new(&mut by_events, dict);
        event_driver::drive(&mut enc, line.json, false).unwrap();
        let b = enc.finish().unwrap();
        assert_eq!(&by_lexer[..a], &by_events[..b], "line {i}");
    }
}

#[test]
fn blobs_decode_back_to_their_base64_text() {
    let dict = fixture_dictionary();
    let mut packed = vec![0u8; 1 << 16];
    let mut blobs = 0;
    let (mut json_total, mut with_blobs, mut without) = (0, 0, 0);
    for (i, line) in fixture_lines().iter().enumerate() {
        let mut enc = PackEncoder::new(&mut packed, dict);
        blobs += event_driver::drive(&mut enc, line.json, true).unwrap();
        let n = enc.finish().unwrap();
        assert_round_trip(i, dict, &packed[..n], line.json);
        with_blobs += n;
        json_total += line.json.len();
        let mut enc = PackEncoder::new(&mut packed, dict);
        enc.json_value(line.json).unwrap();
        without += enc.finish().unwrap();
    }
    assert!(
        blobs > 10,
        "the sample carries pixel and file blobs ({blobs})"
    );
    // Raw blobs beat their base64 text, repeats included (`AF`).
    assert!(with_blobs < without, "{with_blobs} vs {without}");
    // 135,131 → 46,719 B (34.6 %) on the post-lean-wire sample.
    assert!(
        with_blobs * 25 < json_total * 9,
        "{with_blobs} vs {json_total}"
    );
}

#[test]
fn a_mixed_frame_shares_back_references() {
    // Events around lexed text (a RawValue inside a typed message), with a
    // string repeated across the seam.
    let dict: &'static Dictionary = &Dictionary::EMPTY;
    let mut packed = [0u8; 256];
    let mut enc = PackEncoder::new(&mut packed, dict);
    enc.begin_map().unwrap();
    enc.key("slot").unwrap();
    enc.json_value(br#"{"name":"a long repeated value","v":[1.5,-2]}"#)
        .unwrap();
    enc.key("again").unwrap();
    enc.str("a long repeated value").unwrap();
    enc.end_map().unwrap();
    let n = enc.finish().unwrap();
    let text = br#"{"slot":{"name":"a long repeated value","v":[1.5,-2]},"again":"a long repeated value"}"#;
    assert_round_trip(0, dict, &packed[..n], text);
    assert!(n < text.len() - 20, "the repeat became a back-reference");
}

#[test]
fn full_is_reported_for_every_short_buffer() {
    let dict = fixture_dictionary();
    let lines = fixture_lines();
    // Every small line, and the largest line, at every size.
    let big = (0..lines.len())
        .max_by_key(|&i| lines[i].json.len())
        .unwrap();
    let chosen = lines
        .iter()
        .enumerate()
        .filter(|&(i, l)| l.json.len() < 1200 || i == big);
    let mut exact = vec![0u8; 1 << 16];
    let mut short = vec![0u8; 1 << 16];
    for (i, line) in chosen {
        let mut enc = PackEncoder::new(&mut exact, dict);
        enc.json_value(line.json).unwrap();
        let n = enc.finish().unwrap();
        let mut enc = PackEncoder::new(&mut exact, dict);
        event_driver::drive(&mut enc, line.json, true).unwrap();
        let m = enc.finish().unwrap();
        for size in 0..n.max(m) {
            let mut enc = PackEncoder::new(&mut short[..size], dict);
            let r = enc.json_value(line.json);
            if size < n {
                assert_eq!(r, Err(PackError::Full), "line {i}, size {size}");
                assert_eq!(enc.finish(), Err(PackError::Full));
            }
            let mut enc = PackEncoder::new(&mut short[..size], dict);
            let r = event_driver::drive(&mut enc, line.json, true);
            if size < m {
                assert_eq!(r, Err(PackError::Full), "line {i}, size {size}");
                assert_eq!(enc.finish(), Err(PackError::Full));
            }
        }
        // The event path needs no slack: the exact length fits. (The lexer
        // writes a string's text before it knows its code, so it may need a
        // few bytes more than the final frame.)
        let mut enc = PackEncoder::new(&mut short[..m], dict);
        event_driver::drive(&mut enc, line.json, true).unwrap();
        assert_eq!(enc.finish(), Ok(m));
    }
}

fn assert_round_trip(i: usize, dict: &Dictionary, packed: &[u8], json: &[u8]) {
    let mut back = Vec::with_capacity(json.len());
    decode(dict, packed, &mut back).unwrap_or_else(|e| panic!("line {i}: decode {e:?}"));
    if back != json {
        let at = back
            .iter()
            .zip(json)
            .position(|(a, b)| a != b)
            .unwrap_or(back.len().min(json.len()));
        let show = |s: &[u8]| {
            String::from_utf8_lossy(&s[at.saturating_sub(40)..(at + 40).min(s.len())]).into_owned()
        };
        panic!(
            "line {i}: differs at byte {at}\n want {}\n got  {}",
            show(json),
            show(&back)
        );
    }
}
