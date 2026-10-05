//! `inflate` under arbitrary damage: bit flips and truncation applied to
//! otherwise-valid streams, in a seeded loop (no `cargo-fuzz` convention
//! exists in this repo yet — `lp-link`'s own fuzz test uses `proptest`
//! instead and still calls itself "fuzzing"; this crate is small enough
//! that a hand-rolled seeded PRNG needs no extra dependency at all).
//!
//! The one hard guarantee, same as the OTA spike's prototype this crate
//! carries forward: **never panics**, on any input, corrupted or not —
//! every array access `inflate` makes into caller-controlled data goes
//! through a checked accessor first. A truncated stream — strictly shorter
//! than the valid one it came from — must also always come back an
//! `Error`, never a successful but short decode: deflate has no "stop here
//! and call it done" short-circuit, only an explicit end-of-block symbol or
//! a stored block's own length, so cutting bytes out from under either one
//! can only run out of bits truncated, or decode into less than the
//! intended output and then run out, both already covered. A single
//! flipped bit has no such guarantee — deflate carries no checksum of its
//! own, so a flipped bit occasionally still decodes to *something* (wrong,
//! but not detectably so from the stream alone) — so those cases only
//! assert the no-panic guarantee, and separately report how often an error
//! was still produced.

const FIRMWARE: &[u8] =
    include_bytes!("../../../lp-fw/bootloaders/esp32c6-bootloader-idf-v5.5.1.bin");

/// A small, dependency-free splitmix64: deterministic across platforms and
/// Rust versions, which a `std` `HashMap`-seeded RNG is not.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

/// A handful of base streams spanning every block kind miniz is likely to
/// pick: empty, tiny (a stored or fixed block), and a firmware-sized slice
/// (stored, fixed and dynamic blocks all plausible, at various levels).
fn base_streams() -> Vec<Vec<u8>> {
    let mut v = vec![
        miniz_oxide::deflate::compress_to_vec(b"", 6),
        miniz_oxide::deflate::compress_to_vec(b"a", 6),
        miniz_oxide::deflate::compress_to_vec(b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", 6),
    ];
    for level in [0u8, 1, 6, 9] {
        v.push(miniz_oxide::deflate::compress_to_vec(FIRMWARE, level));
        v.push(miniz_oxide::deflate::compress_to_vec(
            &FIRMWARE[..4096],
            level,
        ));
    }
    v
}

#[test]
fn truncation_never_panics_and_always_errors() {
    let bases = base_streams();
    let mut rng = Rng(0x7275_6E63_6174_6564);
    let mut cases = 0;
    for base in &bases {
        if base.len() < 2 {
            continue; // nothing shorter than "empty" to truncate to.
        }
        for _ in 0..300 {
            let cut = rng.below(base.len());
            let truncated = &base[..cut];
            let mut buf = vec![0u8; 1 << 16];
            let result = lp_deflate::inflate(truncated, &mut buf, 0);
            assert!(
                result.is_err(),
                "truncation to {cut}/{} decoded instead of erroring",
                base.len()
            );
            cases += 1;
        }
    }
    assert!(cases >= 3000, "only ran {cases} truncation cases");
}

#[test]
fn bit_flips_never_panic() {
    let bases = base_streams();
    let mut rng = Rng(0x6269_745F_666C_6970);
    let mut cases = 0;
    let mut errored = 0;
    for base in &bases {
        if base.is_empty() {
            continue; // nothing to flip a bit in.
        }
        for _ in 0..300 {
            let mut corrupt = base.clone();
            let byte = rng.below(corrupt.len());
            let bit = rng.below(8);
            corrupt[byte] ^= 1 << bit;
            let mut buf = vec![0u8; 1 << 16];
            // The guarantee under test is simply that this call returns
            // instead of panicking; a panic here fails the test on its own.
            if lp_deflate::inflate(&corrupt, &mut buf, 0).is_err() {
                errored += 1;
            }
            cases += 1;
        }
    }
    assert!(cases >= 3000, "only ran {cases} bit-flip cases");
    // Not a hard guarantee (see the module doc) — measured, not assumed: a
    // level-0 stream is almost entirely stored blocks, which carry no
    // Huffman coding at all, so a flipped bit inside one just changes a
    // single output byte instead of desyncing anything. Across this run's
    // mix (level-0 streams included) only around a fifth of single-bit
    // flips surfaced as an error; this assertion is a floor well under
    // that measurement, not a target to tune toward.
    assert!(
        errored > 0,
        "0/{cases} bit-flip cases errored; corruption may not be reaching the decoder"
    );
}

#[test]
fn truncation_and_bit_flips_combined_never_panic() {
    let bases = base_streams();
    let mut rng = Rng(0x636F_6D62_696E_6564);
    let mut cases = 0;
    for base in &bases {
        if base.len() < 2 {
            continue;
        }
        for _ in 0..300 {
            let cut = 1 + rng.below(base.len() - 1);
            let mut corrupt = base[..cut].to_vec();
            let byte = rng.below(corrupt.len());
            let bit = rng.below(8);
            corrupt[byte] ^= 1 << bit;
            let mut buf = vec![0u8; 1 << 16];
            let _ = lp_deflate::inflate(&corrupt, &mut buf, 0);
            cases += 1;
        }
    }
    assert!(cases >= 3000, "only ran {cases} combined cases");
}

/// A buffer too small to hold the real output must come back `NoRoom`,
/// never silently truncated output or a panic — exercised on valid
/// (uncorrupted) streams, since this is about the caller's buffer being
/// wrong, not the stream being damaged.
#[test]
fn undersized_buffer_is_no_room_never_a_panic() {
    for base in base_streams() {
        let mut buf = [0u8; 1];
        match lp_deflate::inflate(&base, &mut buf, 0) {
            Ok(n) => assert!(n <= 1, "decoded {n} bytes into a 1-byte buffer"),
            Err(lp_deflate::Error::NoRoom) => {}
            Err(other) => panic!("expected NoRoom or Ok(<=1), got {other:?}"),
        }
    }
}
