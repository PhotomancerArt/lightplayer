//! The byte codecs under arbitrary input: COBS-FF round-trips and keeps
//! `0x00` and `0xFF` off the wire, its decoder never panics on garbage, and
//! the CRC-32C detects every single-bit flip.

use lp_link::cobs;
use lp_link::crc::crc32c;
use proptest::prelude::*;

proptest! {
    #[test]
    fn cobs_ff_round_trips(src in prop::collection::vec(any::<u8>(), 0..2_000)) {
        let mut enc = vec![1, 2];
        cobs::encode_no_ff_into(&src, &mut enc);
        let body = &enc[2..];
        prop_assert!(!body.contains(&0) && !body.contains(&0xFF));
        prop_assert!(body.len() <= cobs::max_encoded_no_ff_len(src.len()));
        let mut dec = vec![9];
        cobs::decode_no_ff_into(body, &mut dec).unwrap();
        prop_assert_eq!(&dec[1..], &src[..]);
    }

    /// Runs of the bytes the encoder treats specially, where block and escape
    /// boundaries meet.
    #[test]
    fn cobs_ff_round_trips_specials(
        src in prop::collection::vec(prop_oneof![Just(0u8), Just(0xFE), Just(0xFF), Just(0x41)], 0..1_200),
    ) {
        let mut enc = Vec::new();
        cobs::encode_no_ff_into(&src, &mut enc);
        prop_assert!(!enc.contains(&0) && !enc.contains(&0xFF));
        let mut dec = Vec::new();
        cobs::decode_no_ff_into(&enc, &mut dec).unwrap();
        prop_assert_eq!(dec, src);
    }

    #[test]
    fn cobs_ff_decoder_takes_any_garbage(src in prop::collection::vec(any::<u8>(), 0..1_000)) {
        let mut dec = Vec::new();
        if cobs::decode_no_ff_into(&src, &mut dec).is_ok() {
            // Whatever decodes re-encodes to something that decodes the same.
            let mut enc = Vec::new();
            cobs::encode_no_ff_into(&dec, &mut enc);
            let mut again = Vec::new();
            cobs::decode_no_ff_into(&enc, &mut again).unwrap();
            prop_assert_eq!(again, dec);
        }
    }

    #[test]
    fn crc32c_catches_every_single_bit_flip(
        src in prop::collection::vec(any::<u8>(), 1..600),
        bit in any::<usize>(),
        key in any::<u32>(),
    ) {
        let mut damaged = src.clone();
        let bit = bit % (src.len() * 8);
        damaged[bit / 8] ^= 1 << (bit % 8);
        prop_assert_ne!(crc32c(key, &src), crc32c(key, &damaged));
    }
}
