//! F1 corpus check for the token path, on the REAL wire types.
//!
//! Every recorded `M!` line is decoded into `lpc_wire`'s own message type
//! (`ServerMessage` board→host, `ClientMessage` host→board), then serialized
//! twice through the one erased `ser_write_json` path:
//!   1. into a byte sink → the JSON text (must equal the recorded line), and
//!   2. into `TokenEncoder` → LPBJ, decoded back → must equal (1) byte for byte.
//! Also reports the token frame size against the text-lexing `Encoder`'s.
//!
//! usage: lpbj-tokens-corpus <lines.txt>

use ion_wire_spike::{Encoder, TokenEncoder, decode_to_json};

struct VecSink(Vec<u8>);

impl ser_write::SerWrite for VecSink {
    type Error = core::convert::Infallible;
    fn write(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        self.0.extend_from_slice(buf);
        Ok(())
    }
}

fn main() {
    let data = std::fs::read(std::env::args().nth(1).expect("lines.txt")).unwrap();
    let mut out = vec![0u8; 1 << 20];
    let mut lex_out = vec![0u8; 1 << 20];
    let (mut n, mut text_diff, mut tok_total, mut lex_total, mut json_total) = (0, 0, 0usize, 0usize, 0usize);
    let mut floats_lens = (0usize, 0usize);
    for (i, line) in data.split(|&b| b == b'\n').filter(|l| !l.is_empty()).enumerate() {
        let mut parts = line.splitn(3, |&b| b == b' ');
        let _t = parts.next().unwrap();
        let dir = parts.next().unwrap()[0];
        let json = parts.next().unwrap();
        let text = std::str::from_utf8(json).unwrap();
        let (reference, tok_len) = if dir == b'<' {
            let msg: lpc_wire::WireServerMessage =
                lpc_wire::json::from_str(text).unwrap_or_else(|e| panic!("line {i}: parse {e:?}"));
            run(&msg, &mut out)
        } else {
            let msg: lpc_wire::ClientMessage =
                lpc_wire::json::from_str(text).unwrap_or_else(|e| panic!("line {i}: parse {e:?}"));
            run(&msg, &mut out)
        };
        let tok_len = tok_len.unwrap_or_else(|e| panic!("line {i}: token encode {e:?}"));
        if reference != json {
            text_diff += 1; // the JSON path itself does not reproduce the recording
        }
        let mut back = Vec::new();
        decode_to_json(&out[..tok_len], &mut |s: &[u8]| back.extend_from_slice(s)).unwrap();
        if back != reference {
            let p = back.iter().zip(&reference).position(|(a, b)| a != b).unwrap_or(0);
            panic!(
                "line {i}: token round trip differs at {p}\n want {}\n got  {}",
                String::from_utf8_lossy(&reference[p.saturating_sub(40)..(p + 40).min(reference.len())]),
                String::from_utf8_lossy(&back[p.saturating_sub(40)..(p + 40).min(back.len())])
            );
        }
        let mut lex = Encoder::new(&mut lex_out);
        lex.push(&reference).unwrap();
        let lex_len = lex.finish().unwrap();
        if dir == b'<' && line.windows(13).any(|w| w == b"\"projectRead\"") {
            floats_lens.0 += tok_len;
            floats_lens.1 += lex_len;
        }
        n += 1;
        tok_total += tok_len;
        lex_total += lex_len;
        json_total += reference.len();
    }
    println!(
        "{n} lines: token path round-trips byte-identically to the JSON path on all of them \
         ({text_diff} where the JSON path differs from the recording)"
    );
    println!("JSON {json_total} B, lexing encoder {lex_total} B, token encoder {tok_total} B");
    println!("project-read replies: token {} B vs lexing {} B", floats_lens.0, floats_lens.1);
}

fn run<T: serde::Serialize>(msg: &T, out: &mut [u8]) -> (Vec<u8>, Result<usize, lpc_wire::ErasedWriteError>) {
    let mut sink = VecSink(Vec::new());
    lpc_wire::ser_write_json_to(&mut sink, msg).unwrap();
    let mut enc = TokenEncoder::new(out);
    let r = lpc_wire::ser_write_json_to(&mut enc, msg).map(|()| enc.len());
    (sink.0, r)
}
