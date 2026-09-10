//! The MAC16 option: the 40-bit accumulator, `m0..m3`, and the multiply /
//! multiply-accumulate / load family (ruling R4: implemented per the RM
//! rather than stubbed — it is a page of arithmetic).
//!
//! Semantics: ISA RM §4.3.7 (Tables 4-37/4-38, the ACC layout: `ACCLO` is
//! bits 31..0, `ACCHI` bits 39..32, read sign-extended — Tables 5-132/5-133)
//! and the `MUL.*`, `MULA.*`, `MULS.*`, `UMUL.AA.*`, `LDINC`, `LDDEC` and
//! `MULA.*.LDINC/LDDEC` instruction pages: a 16x16 multiply of the selected
//! halves, sign-extended (zero-extended for `UMUL`) to 40 bits, then written
//! to, added to, or subtracted from the accumulator. The load forms use the
//! MR operands **before** the load lands.

use lp_xt_inst::{MacHalf, MacOp, MacSrc, MacY};

/// The accumulator is 40 bits; everything above is sign.
const ACC_BITS: u32 = 40;

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Mac16 {
    /// `ACC`, kept sign-extended from bit 39.
    pub acc: i64,
    /// `MR[0..3]`.
    pub mr: [u32; 4],
}

impl Mac16 {
    #[must_use]
    pub const fn new() -> Self {
        Self { acc: 0, mr: [0; 4] }
    }

    #[inline]
    fn wrap(v: i64) -> i64 {
        (v << (64 - ACC_BITS)) >> (64 - ACC_BITS)
    }

    /// `rsr.acclo`.
    #[inline]
    #[must_use]
    pub const fn acclo(&self) -> u32 {
        self.acc as u32
    }

    /// `rsr.acchi`: bits 39..32, sign-extended (Table 5-133).
    #[inline]
    #[must_use]
    pub const fn acchi(&self) -> u32 {
        (self.acc >> 32) as i8 as i32 as u32
    }

    #[inline]
    pub fn set_acclo(&mut self, v: u32) {
        let hi = self.acc & !0xFFFF_FFFF;
        self.acc = Self::wrap(hi | i64::from(v));
    }

    #[inline]
    pub fn set_acchi(&mut self, v: u32) {
        let lo = self.acc & 0xFFFF_FFFF;
        self.acc = Self::wrap((i64::from(v as u8 as i8) << 32) | lo);
    }

    /// The selected halves: `.ll/.hl/.lh/.hh` name the x half first.
    #[inline]
    fn halves(half: MacHalf, x: u32, y: u32) -> (u32, u32) {
        let (xh, yh) = match half {
            MacHalf::Ll => (false, false),
            MacHalf::Hl => (true, false),
            MacHalf::Lh => (false, true),
            MacHalf::Hh => (true, true),
        };
        let pick = |v: u32, high: bool| if high { v >> 16 } else { v & 0xFFFF };
        (pick(x, xh), pick(y, yh))
    }

    /// Apply one multiply to the accumulator. `x`/`y` are the full 32-bit
    /// operand values; `half` selects the 16-bit halves.
    pub fn multiply(&mut self, op: MacOp, half: MacHalf, x: u32, y: u32) {
        let (m1, m2) = Self::halves(half, x, y);
        match op {
            MacOp::Umul => {
                self.acc = Self::wrap(i64::from(m1) * i64::from(m2));
            }
            MacOp::Mul => {
                self.acc = Self::wrap(i64::from(m1 as u16 as i16) * i64::from(m2 as u16 as i16));
            }
            MacOp::Mula => {
                let p = i64::from(m1 as u16 as i16) * i64::from(m2 as u16 as i16);
                self.acc = Self::wrap(self.acc + p);
            }
            MacOp::Muls => {
                let p = i64::from(m1 as u16 as i16) * i64::from(m2 as u16 as i16);
                self.acc = Self::wrap(self.acc - p);
            }
        }
    }

    /// Resolve a [`MacSrc`] to its two operand values, given a reader for
    /// address registers.
    #[inline]
    pub fn operands(&self, src: MacSrc, ar: impl Fn(u8) -> u32) -> (u32, u32) {
        match src {
            MacSrc::Aa(s, t) => (ar(s.num()), ar(t.num())),
            MacSrc::Ad(s, my) => (ar(s.num()), self.mr[my.num() as usize]),
            MacSrc::Da(mx, t) => (self.mr[mx.num() as usize], ar(t.num())),
            MacSrc::Dd(mx, my) => (self.mr[mx.num() as usize], self.mr[my.num() as usize]),
        }
    }

    /// The `y` operand of a multiply-and-load form.
    #[inline]
    pub fn y_operand(&self, y: MacY, ar: impl Fn(u8) -> u32) -> u32 {
        match y {
            MacY::Ar(t) => ar(t.num()),
            MacY::Mr(my) => self.mr[my.num() as usize],
        }
    }
}
