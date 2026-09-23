//! Host timing of the encoder on one reply, pushed the way the serializer
//! pushes it (short slices), to find where the device's instructions go.
//! usage: lpbj-bench <reply.json> [iterations]
fn main() {
    let json = std::fs::read(std::env::args().nth(1).unwrap()).unwrap();
    let iters: usize = std::env::args().nth(2).map_or(20000, |s| s.parse().unwrap());
    // Split like ser_write_json: punctuation, key text, value text separately.
    let mut pieces = Vec::new();
    let mut start = 0;
    for (i, &b) in json.iter().enumerate() {
        if matches!(b, b'"' | b'{' | b'}' | b'[' | b']' | b',' | b':') {
            if start < i { pieces.push(&json[start..i]); }
            pieces.push(&json[i..i + 1]);
            start = i + 1;
        }
    }
    if start < json.len() { pieces.push(&json[start..]); }
    let mut out = vec![0u8; 1 << 16];
    let t = std::time::Instant::now();
    let mut n = 0;
    for _ in 0..iters {
        let mut e = ion_wire_spike::Encoder::new(&mut out);
        for p in &pieces { e.push(p).unwrap(); }
        n = e.finish().unwrap();
    }
    let ns = t.elapsed().as_nanos() as f64 / iters as f64;
    lookups_only(&json, iters);
    println!("{} B json in {} pieces -> {} B; {:.1} us/frame ({:.1} ns/B)", json.len(), pieces.len(), n, ns / 1e3, ns / json.len() as f64);
}

#[allow(dead_code)]
fn lookups_only(json: &[u8], iters: usize) {
    // every quoted string in the reply, looked up once in each table
    let mut strs = Vec::new();
    let mut in_s = None;
    for (i, &b) in json.iter().enumerate() {
        if b == b'"' {
            match in_s { None => in_s = Some(i + 1), Some(s) => { strs.push(&json[s..i]); in_s = None } }
        }
    }
    let t = std::time::Instant::now();
    let mut hits = 0;
    for _ in 0..iters {
        for s in &strs {
            if ion_wire_spike::wire_dictionary::find_key(std::hint::black_box(s)).is_some() { hits += 1 }
            else if ion_wire_spike::wire_dictionary::find_value(s).is_some() { hits += 1 }
        }
    }
    let ns = t.elapsed().as_nanos() as f64 / iters as f64;
    println!("lookups only: {} strings, {:.1} us/frame ({})", strs.len(), ns / 1e3, hits / iters);
}
