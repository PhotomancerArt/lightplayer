//! JSON number text ⇄ (sign, coefficient, exponent), text-exact.
//!
//! The wire's floats are f32s printed shortest-round-trip by `ryu-js` (the JS
//! `Number.prototype.toString` layout), so the *text* already is the exact
//! identity of the value. JSON Pack keeps the text: the digits become an
//! integer coefficient and the decimal point a base-10 exponent (Ion's decimal,
//! minus Ion's big-endian Int). Decoding lays the digits out again with the JS
//! rules, and the encoder only takes the decimal form when that layout
//! reproduces its input byte for byte. Anything else (another printer's `1e21`,
//! a coefficient over 19 digits) takes the [`NUMBER_TEXT`] escape verbatim.
//!
//! Why not a binary f32: it needs a decimal→f32 parser on the device and an
//! f32→decimal printer in every decoder, both far larger than this file, for
//! about one byte per float.
//!
//! [`NUMBER_TEXT`]: crate::pack_tags::NUMBER_TEXT

/// Longest text [`layout_decimal`] can produce for a 20-digit coefficient.
pub const DECIMAL_TEXT_MAX: usize = 48;

/// How a JSON number token packs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackNumber {
    /// A plain integer (`0`, `42`, `-7`; never `-0`).
    Int { neg: bool, mag: u64 },
    /// A decimal whose JS layout reproduces the text exactly.
    Decimal { neg: bool, coeff: u64, exp: i32 },
    /// Anything else: carried verbatim.
    Text,
}

/// Classify a JSON number token (just the token, no surrounding bytes).
pub fn classify_number(text: &[u8]) -> PackNumber {
    let (neg, body) = match text.first() {
        Some(b'-') => (true, &text[1..]),
        _ => (false, text),
    };
    if body.is_empty() {
        return PackNumber::Text;
    }
    // Plain integer: 0 | [1-9][0-9]*, and never "-0" (that is a decimal).
    if body.iter().all(u8::is_ascii_digit)
        && (body.len() == 1 || body[0] != b'0')
        && let Some(mag) = parse_u64(body)
        && !(neg && mag == 0)
    {
        return PackNumber::Int { neg, mag };
    }
    let Some((coeff, exp)) = parse_decimal(body) else {
        return PackNumber::Text;
    };
    let mut buf = [0u8; DECIMAL_TEXT_MAX];
    match layout_decimal(neg, coeff, exp, &mut buf) {
        Some(n) if buf[..n] == *text => PackNumber::Decimal { neg, coeff, exp },
        _ => PackNumber::Text,
    }
}

/// Write the JS `Number.prototype.toString` layout of
/// `(-1)^neg × coeff × 10^exp` into `out`, returning its length, or `None` if
/// it does not fit ([`DECIMAL_TEXT_MAX`] always does).
pub fn layout_decimal(neg: bool, coeff: u64, exp: i32, out: &mut [u8]) -> Option<usize> {
    let mut digits = [0u8; 20];
    let k = write_digits(coeff, &mut digits);
    let s = &digits[..k];
    let k = k as i32;
    // Position of the decimal point, counted from the first digit.
    let n = k.checked_add(exp)?;
    let mut w = TextWriter { out, len: 0 };
    if neg {
        w.push(b'-')?;
    }
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

/// `int[.frac][(e|E)[+-]exp]` → (coefficient, exponent). Leading zeros are
/// dropped; trailing zeros are kept, because they are part of the text.
fn parse_decimal(body: &[u8]) -> Option<(u64, i32)> {
    let mut i = 0;
    let mut coeff: u64 = 0;
    let mut sig = 0usize; // significant digits so far
    let mut frac_digits: i32 = 0;
    let mut saw_digit = false;
    let mut in_frac = false;
    // Integer-part zeros after the significant digits, held back: if nothing
    // non-zero follows they become exponent, so a 21-digit
    // `123456790000000000000` still fits a u64 coefficient.
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
        i += 1; // the 'e'
        let mut exp_neg = false;
        match body.get(i) {
            Some(b'+') => i += 1,
            Some(b'-') => {
                exp_neg = true;
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
        if exp_neg {
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

/// Decimal digits of `v`, most significant first; returns the count.
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

struct TextWriter<'a> {
    out: &'a mut [u8],
    len: usize,
}

impl TextWriter<'_> {
    fn push(&mut self, b: u8) -> Option<()> {
        *self.out.get_mut(self.len)? = b;
        self.len += 1;
        Some(())
    }

    fn extend(&mut self, s: &[u8]) -> Option<()> {
        self.out
            .get_mut(self.len..self.len + s.len())?
            .copy_from_slice(s);
        self.len += s.len();
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ryu_js_shapes_are_decimals() {
        for t in [
            "58.082996",
            "0.008998871",
            "-1000.5",
            "1e+21",
            "1.5e-7",
            "0.000001",
            "-0",
            "123456790000000000000",
            "1500.25",
            "100.5",
        ] {
            assert!(
                matches!(classify_number(t.as_bytes()), PackNumber::Decimal { .. }),
                "{t}"
            );
        }
    }

    #[test]
    fn serde_json_shapes_round_trip_too() {
        for t in ["1.0", "0.0", "-0.0", "1.50"] {
            assert!(
                matches!(classify_number(t.as_bytes()), PackNumber::Decimal { .. }),
                "{t}"
            );
        }
    }

    #[test]
    fn foreign_layouts_escape() {
        for t in [
            "1e21",
            "1E+21",
            "15e2",
            "01",
            "-",
            "1.2.3",
            "1e",
            "99999999999999999999.5",
        ] {
            assert_eq!(classify_number(t.as_bytes()), PackNumber::Text, "{t}");
        }
    }

    #[test]
    fn ints() {
        assert_eq!(
            classify_number(b"4311744755"),
            PackNumber::Int {
                neg: false,
                mag: 4_311_744_755
            }
        );
        assert_eq!(
            classify_number(b"-1000"),
            PackNumber::Int {
                neg: true,
                mag: 1000
            }
        );
    }

    #[test]
    fn layout_reproduces_every_decimal() {
        for t in ["58.082996", "1e+21", "1.5e-7", "-0", "0.0", "1.50"] {
            let PackNumber::Decimal { neg, coeff, exp } = classify_number(t.as_bytes()) else {
                panic!("{t}");
            };
            let mut buf = [0u8; DECIMAL_TEXT_MAX];
            let n = layout_decimal(neg, coeff, exp, &mut buf).unwrap();
            assert_eq!(&buf[..n], t.as_bytes());
        }
    }

    #[test]
    fn layout_never_overflows_its_bound() {
        let mut buf = [0u8; DECIMAL_TEXT_MAX];
        for exp in [i32::MIN, -1000, -26, -7, 0, 1, 21, 22, i32::MAX - 30] {
            assert!(
                layout_decimal(true, u64::MAX, exp, &mut buf).is_some(),
                "{exp}"
            );
        }
        assert!(layout_decimal(false, 1, i32::MAX, &mut buf).is_none());
    }
}
