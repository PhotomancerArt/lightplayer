//! Split a raw link capture into console text and `00 'B' COBS 00` frames, and
//! decode each frame back to JSON — the host half of the proposed framing.
//!
//! usage: lpbj-stream <capture> — prints `TEXT <line>` and `FRAME <framed> <lpbj> <json>`.

use std::io::Write;

fn main() {
    let data = std::fs::read(std::env::args().nth(1).expect("capture")).unwrap();
    let mut out = std::io::stdout().lock();
    let mut text = Vec::new();
    let mut i = 0;
    while i < data.len() {
        if data[i] == 0 && data.get(i + 1) == Some(&b'B') {
            let Some(end) = data[i + 2..].iter().position(|&b| b == 0) else { break };
            let body = &data[i + 2..i + 2 + end];
            let mut raw = vec![0u8; body.len()];
            let n = ion_wire_spike::cobs_frame::decode(body, &mut raw).expect("cobs");
            let mut json = Vec::new();
            ion_wire_spike::decode_to_json(&raw[..n], &mut |s: &[u8]| json.extend_from_slice(s)).expect("lpbj");
            writeln!(out, "FRAME {} {} {}", end + 3, n, String::from_utf8_lossy(&json)).unwrap();
            i += 2 + end + 1;
            continue;
        }
        if data[i] == b'\n' {
            if !text.is_empty() {
                writeln!(out, "TEXT {}", String::from_utf8_lossy(&text)).unwrap();
            }
            text.clear();
        } else {
            text.push(data[i]);
        }
        i += 1;
    }
}
