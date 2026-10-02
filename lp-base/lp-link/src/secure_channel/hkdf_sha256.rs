//! Noise's `HKDF(chaining_key, input_key_material, num_outputs)` over
//! HMAC-SHA256, written from the Noise spec (rev 34) §4.3:
//!
//! ```text
//! temp_key = HMAC-HASH(chaining_key, input_key_material)
//! output1  = HMAC-HASH(temp_key, byte(0x01))
//! output2  = HMAC-HASH(temp_key, output1 || byte(0x02))
//! output3  = HMAC-HASH(temp_key, output2 || byte(0x03))
//! ```
//!
//! This is RFC 5869's extract-then-expand with an empty `info`, which is what
//! the `hkdf` oracle checks below.

use zeroize::Zeroize;

use crate::secure_channel::hmac_sha256::hmac_sha256;

/// Two outputs (`MixKey`, `Split`).
pub fn hkdf2(chaining_key: &[u8; 32], ikm: &[u8]) -> ([u8; 32], [u8; 32]) {
    let mut temp = hmac_sha256(chaining_key, &[ikm]);
    let out1 = hmac_sha256(&temp, &[&[0x01]]);
    let out2 = hmac_sha256(&temp, &[&out1, &[0x02]]);
    temp.zeroize();
    (out1, out2)
}

/// Three outputs (`MixKeyAndHash`, the psk token).
pub fn hkdf3(chaining_key: &[u8; 32], ikm: &[u8]) -> ([u8; 32], [u8; 32], [u8; 32]) {
    let mut temp = hmac_sha256(chaining_key, &[ikm]);
    let out1 = hmac_sha256(&temp, &[&[0x01]]);
    let out2 = hmac_sha256(&temp, &[&out1, &[0x02]]);
    let out3 = hmac_sha256(&temp, &[&out2, &[0x03]]);
    temp.zeroize();
    (out1, out2, out3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hkdf::Hkdf;
    use sha2::Sha256;

    #[test]
    fn agrees_with_the_hkdf_oracle() {
        for len in [0usize, 1, 32, 64, 100] {
            let ck: [u8; 32] = core::array::from_fn(|i| (i * 3 + len) as u8);
            let ikm: alloc::vec::Vec<u8> = (0..len).map(|i| (i * 11 + 2) as u8).collect();
            let mut okm = [0u8; 96];
            Hkdf::<Sha256>::new(Some(&ck), &ikm)
                .expand(&[], &mut okm)
                .unwrap();
            let (a, b) = hkdf2(&ck, &ikm);
            assert_eq!((&a[..], &b[..]), (&okm[..32], &okm[32..64]), "ikm {len}");
            let (a, b, c) = hkdf3(&ck, &ikm);
            assert_eq!(
                (&a[..], &b[..], &c[..]),
                (&okm[..32], &okm[32..64], &okm[64..]),
                "ikm {len}"
            );
        }
    }
}
