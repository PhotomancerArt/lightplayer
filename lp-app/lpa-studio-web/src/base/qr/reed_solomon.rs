//! Reed–Solomon error-correction codewords over GF(2⁸).
//!
//! The field is the one the QR standard names: polynomial arithmetic modulo
//! x⁸ + x⁴ + x³ + x² + 1 (0x11D), with α = 2. A block's error-correction
//! codewords are the remainder of its data polynomial, shifted up by the
//! codeword count, divided by the generator ∏ (x − αⁱ) for i in 0..n.

/// The field's reducing polynomial, less its x⁸ term.
const REDUCER: u8 = 0x1D;

/// Multiply two field elements (shift-and-add, reducing as it goes).
pub fn gf_mul(mut a: u8, mut b: u8) -> u8 {
    let mut product = 0u8;
    while b != 0 {
        if b & 1 != 0 {
            product ^= a;
        }
        let carry = a & 0x80 != 0;
        a <<= 1;
        if carry {
            a ^= REDUCER;
        }
        b >>= 1;
    }
    product
}

/// The generator polynomial of degree `degree`, highest-order coefficient
/// first, the leading 1 left implicit (so `degree` coefficients).
pub fn generator(degree: usize) -> Vec<u8> {
    // Start from the constant polynomial 1 and multiply in (x − αⁱ) — in
    // GF(2⁸) subtraction is addition, so each factor is (x + αⁱ).
    let mut poly = vec![1u8];
    let mut alpha_i = 1u8;
    for _ in 0..degree {
        let mut next = vec![0u8; poly.len() + 1];
        for (index, &coefficient) in poly.iter().enumerate() {
            next[index] ^= coefficient;
            next[index + 1] ^= gf_mul(coefficient, alpha_i);
        }
        poly = next;
        alpha_i = gf_mul(alpha_i, 2);
    }
    poly.remove(0);
    poly
}

/// The error-correction codewords for one block of `data`.
pub fn remainder(data: &[u8], generator: &[u8]) -> Vec<u8> {
    let mut rest = vec![0u8; generator.len()];
    for &byte in data {
        let factor = byte ^ rest[0];
        rest.rotate_left(1);
        if let Some(last) = rest.last_mut() {
            *last = 0;
        }
        for (slot, &coefficient) in rest.iter_mut().zip(generator) {
            *slot ^= gf_mul(coefficient, factor);
        }
    }
    rest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_to_the_eighth_reduces_by_the_field_polynomial() {
        // α⁸ = α⁴ + α³ + α² + 1 = 0x1D.
        let mut value = 1u8;
        for _ in 0..8 {
            value = gf_mul(value, 2);
        }
        assert_eq!(value, 0x1D);
        // α has order 255.
        let mut value = 1u8;
        for step in 1..=255 {
            value = gf_mul(value, 2);
            if step < 255 {
                assert_ne!(value, 1, "α^{step} = 1");
            }
        }
        assert_eq!(value, 1);
    }

    /// The degree-7 generator from the standard's worked example (Annex A):
    /// coefficients α^87, α^229, α^146, α^149, α^238, α^102, α^21.
    #[test]
    fn the_seven_codeword_generator_matches_the_standard() {
        let alpha = |exponent: u32| {
            let mut value = 1u8;
            for _ in 0..exponent {
                value = gf_mul(value, 2);
            }
            value
        };
        let want: Vec<u8> = [87, 229, 146, 149, 238, 102, 21]
            .into_iter()
            .map(alpha)
            .collect();
        assert_eq!(generator(7), want);
    }

    #[test]
    fn a_codeword_with_its_remainder_is_divisible() {
        let g = generator(10);
        let data = b"lightplayer";
        let ec = remainder(data, &g);
        let mut whole = data.to_vec();
        whole.extend(&ec);
        assert!(remainder(&whole, &g).iter().all(|&byte| byte == 0));
    }
}
