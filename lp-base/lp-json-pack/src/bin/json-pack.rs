//! `json-pack`: the crate's own oracle on the command line (feature `std`).
//!
//! ```text
//! json-pack roundtrip <lines>                 every M! line: JSON → packed → JSON, byte for byte
//! json-pack encode [--dict-from <lines>]      stdin M! lines → framed packed frames on stdout
//! json-pack decode [--dict-from <lines>]      stdin bytes → frames back to M!{json} lines
//! ```
//!
//! `<lines>` is a file of `M!{json}` lines, optionally prefixed with a
//! direction (`< ` board→host, `> ` host→board) as in the committed fixture.
//! Other lines pass through `encode` untouched. The wire's own dictionary lives
//! in `lpc-wire`, so this tool harvests one from `--dict-from` (its
//! even-numbered lines, as the crate's fixture tests do); without it every
//! string goes inline.
//! `lp-cli wire unpack` is the user-facing reader; this is the codec's.

use std::io::{Read, Write};
use std::process::ExitCode;

use lp_json_pack::{
    Dictionary, DictionaryBuilder, FRAME_KIND_PACK, PackEncoder, ScanEvent, VecFrameScanner,
    decode, frame,
};

/// Room for one frame (the wire's frames are at most 16 KiB of JSON).
const MAX_FRAME: usize = 1 << 20;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("roundtrip") if args.len() == 2 => roundtrip(&args[1]),
        Some("encode") => dict_arg(&args[1..]).and_then(|d| encode(d)),
        Some("decode") => dict_arg(&args[1..]).and_then(|d| decode_stream(d)),
        _ => Err(String::from(
            "usage: json-pack roundtrip <lines> | encode [--dict-from <lines>] | decode [--dict-from <lines>]",
        )),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("json-pack: {e}");
            ExitCode::FAILURE
        }
    }
}

fn dict_arg(args: &[String]) -> Result<&'static Dictionary, String> {
    match args {
        [] => Ok(&Dictionary::EMPTY),
        [flag, path] if flag == "--dict-from" => {
            let data = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
            Ok(harvest(&data))
        }
        _ => Err(String::from("expected --dict-from <lines>")),
    }
}

/// The JSON of each `M!` line, with its direction if it had one.
fn m_lines(data: &[u8]) -> Vec<&[u8]> {
    data.split(|&b| b == b'\n')
        .filter_map(|line| {
            let line = match line {
                [b'<' | b'>', b' ', rest @ ..] => rest,
                _ => line,
            };
            line.strip_prefix(b"M!")
        })
        .collect()
}

/// A dictionary trained on the even-numbered `M!` lines of a file.
fn harvest(data: &[u8]) -> &'static Dictionary {
    let lines = m_lines(data);
    let mut b = DictionaryBuilder::new();
    for json in lines.iter().step_by(2) {
        b.observe_json(json);
    }
    b.build(2).leak()
}

fn pack(dict: &'static Dictionary, json: &[u8], out: &mut [u8]) -> Result<usize, String> {
    let mut e = PackEncoder::new(out, dict);
    e.json_value(json)
        .map_err(|err| format!("encode: {err:?}"))?;
    e.finish().map_err(|err| format!("encode: {err:?}"))
}

fn roundtrip(path: &str) -> Result<(), String> {
    let data = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let dict = harvest(&data);
    let mut packed = vec![0u8; MAX_FRAME];
    let (mut json_total, mut packed_total, mut n) = (0usize, 0usize, 0usize);
    for (i, json) in m_lines(&data).into_iter().enumerate() {
        let len = pack(dict, json, &mut packed).map_err(|e| format!("line {i}: {e}"))?;
        let mut back = Vec::with_capacity(json.len());
        decode(dict, &packed[..len], &mut back).map_err(|e| format!("line {i}: decode {e:?}"))?;
        if back != json {
            let at = back
                .iter()
                .zip(json)
                .position(|(a, b)| a != b)
                .unwrap_or(back.len().min(json.len()));
            return Err(format!("line {i}: differs at byte {at}"));
        }
        json_total += json.len();
        packed_total += len;
        n += 1;
    }
    println!(
        "{n} lines round-tripped byte for byte; JSON {json_total} B -> packed {packed_total} B ({:.2}x); dictionary {} keys, {} values, fingerprint {:#010x}",
        json_total as f64 / packed_total.max(1) as f64,
        dict.keys.len(),
        dict.values.len(),
        dict.fingerprint()
    );
    Ok(())
}

fn encode(dict: &'static Dictionary) -> Result<(), String> {
    let mut input = Vec::new();
    std::io::stdin()
        .read_to_end(&mut input)
        .map_err(|e| e.to_string())?;
    let mut stdout = std::io::stdout().lock();
    let mut packed = vec![0u8; MAX_FRAME];
    let mut framed = vec![0u8; lp_json_pack::max_framed_len(MAX_FRAME)];
    for line in input.split_inclusive(|&b| b == b'\n') {
        let body = line.strip_suffix(b"\n").unwrap_or(line);
        let packed_len = body
            .strip_prefix(b"M!")
            .and_then(|json| pack(dict, json, &mut packed).ok());
        let w = match packed_len {
            Some(n) => {
                let f = frame(FRAME_KIND_PACK, &packed[..n], &mut framed)
                    .ok_or("frame buffer too small")?;
                stdout.write_all(&framed[..f])
            }
            None => stdout.write_all(line),
        };
        w.map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn decode_stream(dict: &'static Dictionary) -> Result<(), String> {
    let mut stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    let mut scanner = VecFrameScanner::with_max_body(lp_json_pack::max_framed_len(MAX_FRAME));
    let mut buf = vec![0u8; 64 * 1024];
    let mut failure = None;
    loop {
        let n = stdin.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        scanner.push(&buf[..n], |event| {
            let r = match event {
                ScanEvent::Text(t) => stdout.write_all(t),
                ScanEvent::Frame { kind, payload } if kind == FRAME_KIND_PACK => {
                    let mut json = b"M!".to_vec();
                    match decode(dict, payload, &mut json) {
                        Ok(()) => {
                            json.push(b'\n');
                            stdout.write_all(&json)
                        }
                        Err(e) => {
                            eprintln!("json-pack: frame did not decode: {e:?}");
                            Ok(())
                        }
                    }
                }
                ScanEvent::Frame { kind, .. } => {
                    eprintln!("json-pack: skipped a frame of kind {kind:#04x}");
                    Ok(())
                }
                ScanEvent::Dropped(why) => {
                    eprintln!("json-pack: dropped a frame: {why:?}");
                    Ok(())
                }
            };
            if let Err(e) = r {
                failure.get_or_insert(e.to_string());
            }
        });
        if let Some(e) = failure.take() {
            return Err(e);
        }
    }
    Ok(())
}
