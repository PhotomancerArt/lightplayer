//! SPIKE (plan `lp2025/2026-09-25-0006-learned-wire-dictionary`): replay a
//! recorded session's board→host messages through each dictionary option and
//! report bytes per message class, the learned table's growth, and a
//! round-trip check of every learned frame.
//!
//! usage: learned-replay <session.jsonl>...   (lines `kind<TAB>json`, from
//! `spikes/learned-wire-dict/extract.py`)

use std::collections::BTreeMap;

use lp_json_pack::{
    Dictionary, LearnMark, LearnStore, LearnedTable, OwnedDictionary, SliceJsonOut, decode_learned,
};
use lpc_wire::{WIRE_DICTIONARY, WireEncoding, WireServerMessage, ser_learned_to, ser_wire_to};

use lp_json_pack::pack_learned::{VALUES_NONE, VALUES_SECOND_SIGHTING};

/// Big enough never to fill on these sessions: shows what a session needs.
type Unbounded = LearnedTable<32768, 2048, 4096, 2048, 4096>;
/// A device-sized candidate (see the plan's RAM section).
type Device = LearnedTable<3072, 320, 512, 128, 256>;
type DeviceSmall = LearnedTable<2048, 256, 512, 64, 128>;
/// Keys only: every value string goes inline (or back-referenced).
type KeysOnly = LearnedTable<2304, 288, 512, 1, 2, VALUES_NONE>;
/// Keys, plus values learned on their second inline sighting.
type Second = LearnedTable<3584, 288, 512, 192, 256, VALUES_SECOND_SIGHTING, 512>;
type SecondSmall = LearnedTable<3072, 288, 512, 128, 256, VALUES_SECOND_SIGHTING, 512>;
type SecondLog1k = LearnedTable<3072, 288, 512, 128, 256, VALUES_SECOND_SIGHTING, 1024>;
fn second_log1k() -> Box<dyn LearnStore> {
    Box::new(SecondLog1k::NEW)
}

type Make = fn() -> Box<dyn LearnStore>;
fn unbounded() -> Box<dyn LearnStore> {
    Box::new(Unbounded::NEW)
}
fn device() -> Box<dyn LearnStore> {
    Box::new(Device::NEW)
}
fn device_small() -> Box<dyn LearnStore> {
    Box::new(DeviceSmall::NEW)
}
fn keys_only() -> Box<dyn LearnStore> {
    Box::new(KeysOnly::NEW)
}
fn second() -> Box<dyn LearnStore> {
    Box::new(Second::NEW)
}
fn second_small() -> Box<dyn LearnStore> {
    Box::new(SecondSmall::NEW)
}

struct Msg {
    kind: String,
    id: u64,
    json: String,
    msg: WireServerMessage,
}

#[derive(Default)]
struct Tally {
    by_class: BTreeMap<String, Vec<usize>>,
    total: usize,
    first_lens_read: usize,
    /// Every byte from connect through the first lens reply.
    connect: usize,
    fallbacks: usize,
    peak: LearnMark,
    /// Message index at which the table first reached 90 % of its final entries.
    settle_at: Option<usize>,
}

fn main() {
    for path in std::env::args().skip(1) {
        let msgs = load(&path);
        println!("\n=== {path}: {} board→host messages ===", msgs.len());
        let json: Vec<usize> = msgs.iter().map(|m| m.json.len()).collect();
        report("JSON", &msgs, &tally_sizes(&msgs, &json), None);

        let static_sizes: Vec<usize> = msgs
            .iter()
            .map(|m| {
                let mut buf = vec![0u8; 1 << 17];
                ser_wire_to(&mut buf, WireEncoding::Packed, &m.msg).unwrap()
            })
            .collect();
        report(
            "static (today, 419/186 entries)",
            &msgs,
            &tally_sizes(&msgs, &static_sizes),
            None,
        );

        let e = &Dictionary::EMPTY;
        run_learned("(a) all values, unbounded", &msgs, e, unbounded);
        run_learned("(a) all values, Device", &msgs, e, device);
        run_learned("(a) all values, DeviceSmall", &msgs, e, device_small);
        run_learned("(a) keys only", &msgs, e, keys_only);
        run_learned("(a) 2nd-sighting values, Second", &msgs, e, second);
        run_learned(
            "(a) 2nd-sighting values, SecondSmall",
            &msgs,
            e,
            second_small,
        );
        run_learned(
            "(a) 2nd-sighting values, SecondLog1k",
            &msgs,
            e,
            second_log1k,
        );
        for (k, v) in [(32, 8), (64, 16), (128, 32), (240, 64)] {
            let seed = seed_of(k, v);
            run_learned(
                &format!("(c) seed {k}k/{v}v, all values, Device"),
                &msgs,
                seed,
                device,
            );
            run_learned(
                &format!("(c) seed {k}k/{v}v, 2nd-sighting, Second"),
                &msgs,
                seed,
                second,
            );
        }
        let values_only = seed_of_values();
        run_learned(
            "(c) static VALUES + learned keys only",
            &msgs,
            values_only,
            keys_only,
        );
        run_learned("static + learned, Device", &msgs, &WIRE_DICTIONARY, device);
        run_losses(&msgs, second_small);
        run_losses(&msgs, second_log1k);
    }
    in_band_cost();
}

fn load(path: &str) -> Vec<Msg> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| {
            let (kind, json) = l.split_once('\t').unwrap();
            let v: serde_json::Value = serde_json::from_str(json).unwrap();
            Msg {
                kind: kind.to_string(),
                id: v.get("id").and_then(|i| i.as_u64()).unwrap_or(0),
                json: json.to_string(),
                msg: lpc_wire::json::from_str(json).unwrap(),
            }
        })
        .collect()
}

fn seed_of(keys: usize, values: usize) -> &'static Dictionary {
    let k: Vec<String> = (0..keys.min(WIRE_DICTIONARY.keys.len()))
        .map(|i| String::from_utf8(WIRE_DICTIONARY.key(i).unwrap().to_vec()).unwrap())
        .collect();
    let v: Vec<String> = (0..values.min(WIRE_DICTIONARY.values.len()))
        .map(|i| String::from_utf8(WIRE_DICTIONARY.value(i).unwrap().to_vec()).unwrap())
        .collect();
    let owned = OwnedDictionary::from_entries(&k, &v);
    let text: usize = k.iter().chain(v.iter()).map(|s| s.len()).sum();
    eprintln!("seed {keys}/{values}: {text} B of text");
    owned.leak()
}

fn seed_of_values() -> &'static Dictionary {
    let v: Vec<String> = (0..WIRE_DICTIONARY.values.len())
        .map(|i| String::from_utf8(WIRE_DICTIONARY.value(i).unwrap().to_vec()).unwrap())
        .collect();
    OwnedDictionary::from_entries::<String, String>(&[], &v).leak()
}

fn run_learned(name: &str, msgs: &[Msg], seed: &'static Dictionary, make: Make) {
    let mut board = make();
    // The host's table is a twin of the board's (same capacities, same rule).
    let mut host = make();
    let mut sizes = Vec::with_capacity(msgs.len());
    let mut fallbacks = 0;
    let mut marks = Vec::new();
    for m in msgs {
        let mut buf = vec![0u8; m.json.len() + 4];
        match ser_learned_to(&mut buf, seed, &mut *board, &m.msg) {
            Ok(n) => {
                let mut out = vec![0u8; m.json.len() + 64];
                let mut sink = SliceJsonOut::new(&mut out);
                decode_learned(seed, &mut *host, &buf[..n], &mut sink)
                    .unwrap_or_else(|e| panic!("{name}: id {} decode: {e:?}", m.id));
                assert_eq!(
                    sink.as_bytes(),
                    m.json.as_bytes(),
                    "{name}: id {} round trip",
                    m.id
                );
                sizes.push(n);
            }
            Err(e) => {
                // The device would send JSON; the table was truncated.
                eprintln!("{name}: id {} {} not packed ({e})", m.id, m.kind);
                fallbacks += 1;
                sizes.push(m.json.len());
            }
        }
        marks.push(board.mark());
    }
    assert_eq!(board.mark(), host.mark(), "{name}: tables diverged");
    let mut t = tally_sizes(msgs, &sizes);
    t.fallbacks = fallbacks;
    t.peak = board.mark();
    let fin = t.peak.keys + t.peak.values;
    t.settle_at = marks
        .iter()
        .position(|m| (m.keys + m.values) * 10 >= fin * 9);
    report(name, msgs, &t, Some(board_bytes(&*board)));
}

/// The resync design under loss: every 23rd frame is abandoned by the board
/// (it knows, and truncates), every 29th (from the 8th) arrives torn (the board thinks it was
/// sent). The host checks each learned frame's header, drops frames on a
/// mismatch, and "sends SetEncoding"; the board resets its table (new epoch)
/// before the frame after next (one round trip). Every frame the host does
/// decode must equal the recorded JSON.
fn run_losses(msgs: &[Msg], make: Make) {
    let seed = &Dictionary::EMPTY;
    let mut board = make();
    let mut host = make();
    let (mut abandoned, mut torn, mut dropped, mut decoded, mut resets) = (0, 0, 0, 0, 0);
    let mut reset_due: Option<usize> = None;
    for (i, m) in msgs.iter().enumerate() {
        if reset_due == Some(i) {
            let e = board.epoch().wrapping_add(1);
            board.reset(e);
            reset_due = None;
            resets += 1;
        }
        let mut buf = vec![0u8; m.json.len() + 4];
        let mark = board.mark();
        let Ok(n) = ser_learned_to(&mut buf, seed, &mut *board, &m.msg) else {
            continue;
        };
        if i % 23 == 22 {
            board.truncate(mark); // the io_task reported the write abandoned
            abandoned += 1;
            continue;
        }
        if i % 29 == 7 {
            torn += 1; // bytes lost in flight: the host's COBS walk fails
            continue;
        }
        let mut out = vec![0u8; m.json.len() + 64];
        let mut sink = SliceJsonOut::new(&mut out);
        match decode_learned(seed, &mut *host, &buf[..n], &mut sink) {
            Ok(()) => {
                assert_eq!(sink.as_bytes(), m.json.as_bytes(), "loss run: id {}", m.id);
                decoded += 1;
            }
            Err(lp_json_pack::DecodeError::Learned(_)) => {
                dropped += 1;
                if reset_due.is_none() {
                    reset_due = Some(i + 2);
                }
            }
            Err(e) => panic!("loss run: id {} {e:?}", m.id),
        }
    }
    println!(
        "loss run: {} messages, {abandoned} abandoned by the board (no desync), {torn} torn in flight, \
         {dropped} more dropped by the host while desynced, {resets} resets, {decoded} decoded byte-exact",
        msgs.len()
    );
}

fn board_bytes(board: &dyn LearnStore) -> usize {
    core::mem::size_of_val(board)
}

fn tally_sizes(msgs: &[Msg], sizes: &[usize]) -> Tally {
    let mut t = Tally::default();
    let first_lens = msgs
        .iter()
        .find(|m| m.kind.contains("[lens]"))
        .map(|m| m.id);
    let last_first_lens = msgs
        .iter()
        .rposition(|m| Some(m.id) == first_lens)
        .unwrap_or(0);
    t.connect = sizes[..=last_first_lens].iter().sum();
    for (m, &n) in msgs.iter().zip(sizes) {
        t.by_class.entry(m.kind.clone()).or_default().push(n);
        t.total += n;
        if Some(m.id) == first_lens {
            t.first_lens_read += n;
        }
    }
    t
}

fn report(name: &str, _msgs: &[Msg], t: &Tally, table_bytes: Option<usize>) {
    let steady = |class: &str| -> String {
        match t.by_class.get(class) {
            Some(v) if v.len() >= 4 => {
                let mut tail = v[v.len() / 2..].to_vec();
                tail.sort();
                format!("{}", tail[tail.len() / 2])
            }
            Some(v) => format!("{:?}", v),
            None => "-".into(),
        }
    };
    let first = |class: &str| -> String {
        t.by_class
            .get(class)
            .and_then(|v| v.first())
            .map_or("-".into(), |n| n.to_string())
    };
    print!(
        "{name:44} total {:>7} connect {:>6} sync1 {:>6} first-lens {:>5} lens steady {:>5}  card first/steady {:>4}/{:>4}  heartbeat {:>4}  hello {:>4}",
        t.total,
        t.connect,
        first("projectRead.events [sync]"),
        t.first_lens_read,
        steady("projectRead.events [lens]"),
        first("projectRead.events [card]"),
        steady("projectRead.events [card]"),
        steady("heartbeat.fps"),
        first("hello.proto"),
    );
    if let Some(b) = table_bytes {
        print!(
            "  | {}k/{}v {} B text, RAM {} B, fb {}",
            t.peak.keys, t.peak.values, t.peak.text, b, t.fallbacks
        );
    }
    println!();
}

/// Option (b): what sending today's dictionary once costs, packed as a JSON
/// object `{"k":[...],"v":[...]}` with no dictionary.
fn in_band_cost() {
    let keys: Vec<String> = (0..WIRE_DICTIONARY.keys.len())
        .map(|i| String::from_utf8(WIRE_DICTIONARY.key(i).unwrap().to_vec()).unwrap())
        .collect();
    let values: Vec<String> = (0..WIRE_DICTIONARY.values.len())
        .map(|i| String::from_utf8(WIRE_DICTIONARY.value(i).unwrap().to_vec()).unwrap())
        .collect();
    let v = serde_json::json!({ "k": keys, "v": values });
    let json = serde_json::to_string(&v).unwrap();
    let mut buf = vec![0u8; 1 << 16];
    let mut enc = lp_json_pack::PackEncoder::new(&mut buf, &Dictionary::EMPTY);
    let mut lexer = lp_json_pack::PackLexer::new();
    lexer.push(&mut enc, json.as_bytes()).unwrap();
    lexer.finish(&mut enc).unwrap();
    let n = enc.finish().unwrap();
    println!(
        "\n(b) in-band dictionary: {} keys + {} values, JSON {} B, packed {} B (once per connection when the host lacks the fingerprint)",
        keys.len(),
        values.len(),
        json.len(),
        n
    );
}
