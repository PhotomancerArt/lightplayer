//! f32 exactness: for a stride through all 2^32 bit patterns, print the value the
//! way `ser_write_json` does (`ryu-js`), wrap it as a JSON value, encode, decode,
//! and require (a) identical text and (b) the text parses back to identical bits.
//!
//! usage: lpbj-f32-sweep [stride=97]

fn main() {
    let stride: u64 = std::env::args().nth(1).map_or(97, |s| s.parse().unwrap());
    let mut out = [0u8; 256];
    let (mut checked, mut decimals, mut texts, mut ints) = (0u64, 0u64, 0u64, 0u64);
    let mut bits: u64 = 0;
    while bits <= u64::from(u32::MAX) {
        let v = f32::from_bits(bits as u32);
        bits += stride;
        if !v.is_finite() {
            continue; // ser_write_json writes null
        }
        let mut rb = ryu_js::Buffer::new();
        let text = rb.format(v);
        let mut enc = ion_wire_spike::Encoder::new(&mut out);
        enc.push(text.as_bytes()).unwrap();
        let n = enc.finish().unwrap();
        match out[0] {
            0xA8 | 0xA9 => decimals += 1,
            0xAE => texts += 1,
            _ => ints += 1,
        }
        let mut back = Vec::new();
        ion_wire_spike::decode_to_json(&out[..n], &mut |s: &[u8]| back.extend_from_slice(s)).unwrap();
        assert_eq!(back, text.as_bytes(), "text mismatch for bits {:#010x}", bits - stride);
        let parsed: f32 = std::str::from_utf8(&back).unwrap().parse().unwrap();
        assert_eq!(parsed.to_bits(), v.to_bits(), "bits mismatch for {text}");
        checked += 1;
    }
    println!("{checked} finite f32 values (stride {stride}): text and bits identical; \
        {decimals} as decimal, {ints} as int, {texts} via text escape");
}
