//! JSON number text ⇄ (sign, coefficient, exponent), text-exact.
//!
//! Ion has both a 4/8-byte binary float and an arbitrary-precision decimal. The
//! wire's floats are f32 printed shortest-round-trip by `ryu-js` (JS
//! `Number::toString` layout), so the *text* is already the exact identity of the
//! f32. We keep the text: digits become an integer coefficient and the point
//! becomes a base-10 exponent (Ion's decimal, minus Ion's big-endian Int).
//! Decoding re-lays the digits out with the JS rules, and the encoder only
//! takes this path when that re-layout reproduces the input byte for byte.
//! Anything else (another printer's `1e21`, a >19-digit coefficient) goes
//! through the `NUMBER_TEXT` escape hatch verbatim.
//!
//! Why not Ion's binary f32: it needs a decimal→f32 parser on the device (core's
//! `dec2flt`) plus an f32→decimal printer on every decoder, both far larger than
//! this file, for ~1 byte per float saved (measured in the findings).

/// A classified number token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberToken {
    Int { neg: bool, mag: u64 },
    Decimal { neg: bool, coeff: u64, exp: i32 },
    Text,
}

/// Classify a JSON number token (no surrounding bytes).
pub fn classify(text: &[u8]) -> NumberToken {
    let (neg, body) = match text.first() {
        Some(b'-') => (true, &text[1..]),
        _ => (false, text),
    };
    if body.is_empty() {
        return NumberToken::Text;
    }
    // Plain integer: 0 | [1-9][0-9]*, never "-0".
    if body.iter().all(u8::is_ascii_digit) {
        if (body.len() == 1 || body[0] != b'0')
            && let Some(mag) = parse_u64(body)
            && !(neg && mag == 0)
        {
            return NumberToken::Int { neg, mag };
        }
        // falls through: "-0" and >u64 go to the decimal path
    }
    let Some((coeff, exp)) = parse_decimal(body) else {
        return NumberToken::Text;
    };
    let mut buf = [0u8; 48];
    let n = layout(neg, coeff, exp, &mut buf);
    if n == Some(text.len()) && buf[..text.len()] == *text {
        NumberToken::Decimal { neg, coeff, exp }
    } else {
        NumberToken::Text
    }
}

/// Write the JS `Number::toString` layout of `(-1)^neg × coeff × 10^exp` into
/// `out`, returning the length, or `None` if it does not fit.
pub fn layout(neg: bool, coeff: u64, exp: i32, out: &mut [u8]) -> Option<usize> {
    let mut digits = [0u8; 20];
    let k = write_digits(coeff, &mut digits);
    let s = &digits[..k];
    let n = k as i32 + exp; // position of the decimal point
    let mut w = Writer { out, len: 0 };
    if neg {
        w.push(b'-')?;
    }
    let k = k as i32;
    if k <= n && n <= 21 {
        w.extend(s)?;
        for _ in 0..(n - k) {
            w.push(b'0')?;
        }
    } else if 0 < n && n <= 21 {
        w.extend(&s[..n as usize])?;
        w.push(b'.')?;
        w.extend(&s[n as usize..])?;
    } else if -6 < n && n <= 0 {
        w.extend(b"0.")?;
        for _ in 0..(-n) {
            w.push(b'0')?;
        }
        w.extend(s)?;
    } else {
        w.push(s[0])?;
        if k > 1 {
            w.push(b'.')?;
            w.extend(&s[1..])?;
        }
        w.push(b'e')?;
        let e = n - 1;
        w.push(if e >= 0 { b'+' } else { b'-' })?;
        let mut ed = [0u8; 20];
        let el = write_digits(u64::from(e.unsigned_abs()), &mut ed);
        w.extend(&ed[..el])?;
    }
    Some(w.len)
}

/// `int[.frac][(e|E)[+-]exp]` → (coefficient, exponent). Leading zeros of the
/// digit string are dropped; trailing zeros are kept (they are the text).
fn parse_decimal(body: &[u8]) -> Option<(u64, i32)> {
    let mut i = 0;
    let mut coeff: u64 = 0;
    let mut sig = 0usize; // significant digits accumulated (after leading zeros)
    let mut frac_digits: i32 = 0;
    let mut saw_digit = false;
    let mut in_frac = false;
    let mut int_zeros: i32 = 0;
    while i < body.len() {
        let c = body[i];
        match c {
            b'0'..=b'9' => {
                saw_digit = true;
                if in_frac {
                    frac_digits += 1;
                }
                if !in_frac && c == b'0' && coeff != 0 {
                    // Integer-part zeros after the significant digits are held
                    // back: if no non-zero digit follows they become exponent,
                    // so `123456790000000000000` (21 digits) still fits u64.
                    int_zeros += 1;
                } else if coeff != 0 || c != b'0' {
                    for _ in 0..core::mem::take(&mut int_zeros) {
                        sig += 1;
                        coeff = coeff.checked_mul(10)?;
                    }
                    sig += 1;
                    if sig > 19 {
                        return None;
                    }
                    coeff = coeff * 10 + u64::from(c - b'0');
                }
            }
            b'.' if !in_frac => {
                for _ in 0..core::mem::take(&mut int_zeros) {
                    sig += 1;
                    coeff = coeff.checked_mul(10)?;
                }
                in_frac = true;
            }
            b'e' | b'E' => break,
            _ => return None,
        }
        i += 1;
    }
    if !saw_digit {
        return None;
    }
    let mut exp: i32 = 0;
    if i < body.len() {
        i += 1; // 'e'
        let mut eneg = false;
        match body.get(i) {
            Some(b'+') => i += 1,
            Some(b'-') => {
                eneg = true;
                i += 1;
            }
            _ => {}
        }
        if i >= body.len() {
            return None;
        }
        for &c in &body[i..] {
            if !c.is_ascii_digit() || exp > 100_000 {
                return None;
            }
            exp = exp * 10 + i32::from(c - b'0');
        }
        if eneg {
            exp = -exp;
        }
    }
    Some((coeff, exp - frac_digits + int_zeros))
}

fn parse_u64(digits: &[u8]) -> Option<u64> {
    let mut v: u64 = 0;
    for &c in digits {
        v = v.checked_mul(10)?.checked_add(u64::from(c - b'0'))?;
    }
    Some(v)
}

fn write_digits(mut v: u64, out: &mut [u8; 20]) -> usize {
    let mut tmp = [0u8; 20];
    let mut n = 0;
    loop {
        tmp[n] = b'0' + (v % 10) as u8;
        v /= 10;
        n += 1;
        if v == 0 {
            break;
        }
    }
    for i in 0..n {
        out[i] = tmp[n - 1 - i];
    }
    n
}

struct Writer<'a> {
    out: &'a mut [u8],
    len: usize,
}

impl Writer<'_> {
    fn push(&mut self, b: u8) -> Option<()> {
        *self.out.get_mut(self.len)? = b;
        self.len += 1;
        Some(())
    }
    fn extend(&mut self, s: &[u8]) -> Option<()> {
        self.out.get_mut(self.len..self.len + s.len())?.copy_from_slice(s);
        self.len += s.len();
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ryu_js_shapes_are_decimals() {
        for t in ["58.082996", "0.008998871", "-1000.5", "1e+21", "1.5e-7", "0.000001", "-0", "123456790000000000000", "1500.25", "100.5"] {
            assert!(matches!(classify(t.as_bytes()), NumberToken::Decimal { .. }), "{t}");
        }
    }

    #[test]
    fn serde_json_shapes_round_trip_too() {
        for t in ["1.0", "0.0", "-0.0", "1.50"] {
            assert!(matches!(classify(t.as_bytes()), NumberToken::Decimal { .. }), "{t}");
        }
    }

    #[test]
    fn foreign_layouts_escape() {
        for t in ["1e21", "1E+21", "15e2", "01"] {
            assert_eq!(classify(t.as_bytes()), NumberToken::Text, "{t}");
        }
    }

    #[test]
    fn ints() {
        assert_eq!(classify(b"4311744755"), NumberToken::Int { neg: false, mag: 4_311_744_755 });
        assert_eq!(classify(b"-1000"), NumberToken::Int { neg: true, mag: 1000 });
    }
}
