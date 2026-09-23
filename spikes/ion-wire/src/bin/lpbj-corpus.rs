//! Round-trip and measure every `M!` line of a corpus.
//!
//! usage: lpbj-corpus <lines.txt> <out-prefix>
//!   lines.txt: `<t_us> <dir> <json>` per line (tap_split.py / sendless_model.py)
//!   writes <out-prefix>.csv (index,dir,json_len,lpbj_len) and <out-prefix>.bin
//!   (u32-LE length + LPBJ frame, per line) for the LZ analysis.
//! Exits non-zero on the first line that does not round-trip byte-identically.
//! Input is pushed in uneven slices (1..=23 bytes) to exercise the streaming path.

use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&args[1]).expect("read corpus");
    let mut csv = std::fs::File::create(format!("{}.csv", args[2])).unwrap();
    let mut bin = std::fs::File::create(format!("{}.bin", args[2])).unwrap();
    writeln!(csv, "index,dir,json_len,lpbj_len").unwrap();
    let mut out = vec![0u8; 1 << 20];
    let (mut n, mut json_total, mut bin_total) = (0usize, 0usize, 0usize);
    for (i, line) in data.split(|&b| b == b'\n').filter(|l| !l.is_empty()).enumerate() {
        let mut parts = line.splitn(3, |&b| b == b' ');
        let _t = parts.next().unwrap();
        let dir = parts.next().unwrap()[0] as char;
        let json = parts.next().unwrap();
        let mut enc = ion_wire_spike::Encoder::new(&mut out);
        let mut at = 0;
        let mut step = 1;
        while at < json.len() {
            let end = (at + step).min(json.len());
            enc.push(&json[at..end]).unwrap_or_else(|e| panic!("line {i}: encode {e:?}"));
            at = end;
            step = step % 23 + 1;
        }
        let len = enc.finish().unwrap_or_else(|e| panic!("line {i}: finish {e:?}"));
        let mut back = Vec::with_capacity(json.len());
        let used = ion_wire_spike::decode_to_json(&out[..len], &mut |s: &[u8]| back.extend_from_slice(s))
            .unwrap_or_else(|e| panic!("line {i}: decode {e:?}"));
        assert_eq!(used, len, "line {i}: trailing bytes");
        if back != json {
            let p = back.iter().zip(json).position(|(a, b)| a != b).unwrap_or(back.len().min(json.len()));
            panic!(
                "line {i}: mismatch at {p}\n want {}\n got  {}",
                String::from_utf8_lossy(&json[p.saturating_sub(40)..(p + 40).min(json.len())]),
                String::from_utf8_lossy(&back[p.saturating_sub(40)..(p + 40).min(back.len())])
            );
        }
        writeln!(csv, "{i},{dir},{},{len}", json.len()).unwrap();
        bin.write_all(&(len as u32).to_le_bytes()).unwrap();
        bin.write_all(&out[..len]).unwrap();
        n += 1;
        json_total += json.len();
        bin_total += len;
    }
    println!("{n} lines round-tripped byte-identically; JSON {json_total} B -> LPBJ {bin_total} B ({:.2}x)",
        json_total as f64 / bin_total as f64);
}
