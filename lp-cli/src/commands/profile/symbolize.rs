//! Map guest PCs to ELF symbol names (interval lookup).

use std::borrow::Cow;

use lp_riscv_emu::profile::{PcSymbolizer, TraceSymbol};

pub struct Symbolizer<'a> {
    sorted: Vec<(u32, u32, &'a str)>,
}

impl<'a> Symbolizer<'a> {
    pub fn new(symbols: &'a [TraceSymbol]) -> Self {
        let mut sorted: Vec<_> = symbols
            .iter()
            .filter(|s| s.size > 0)
            .map(|s| (s.addr, s.addr.saturating_add(s.size), s.name.as_str()))
            .collect();
        sorted.sort_unstable_by_key(|&(lo, _, _)| lo);
        Self { sorted }
    }

    pub fn lookup(&self, pc: u32) -> Cow<'a, str> {
        if pc == 0 {
            return Cow::Borrowed("<root>");
        }
        let idx = self.sorted.partition_point(|t| t.0 <= pc).saturating_sub(1);
        if let Some((lo, hi, name)) = self.sorted.get(idx) {
            if pc >= *lo && pc < *hi {
                return Cow::Borrowed(name);
            }
        }
        if pc >= 0x8000_0000 {
            Cow::Owned(format!("<jit:{pc:#010x}>"))
        } else {
            Cow::Owned(format!("<unknown:{pc:#010x}>"))
        }
    }

    /// Lowest address of the ELF symbol interval containing `pc`, or `pc` itself if none.
    pub fn entry_lo_for_pc(&self, pc: u32) -> u32 {
        if pc == 0 {
            return 0;
        }
        let idx = self.sorted.partition_point(|t| t.0 <= pc).saturating_sub(1);
        if let Some((lo, hi, _)) = self.sorted.get(idx) {
            if pc >= *lo && pc < *hi {
                return *lo;
            }
        }
        pc
    }
}

impl<'a> PcSymbolizer for Symbolizer<'a> {
    fn symbolize(&self, pc: u32) -> Cow<'_, str> {
        self.lookup(pc)
    }

    fn entry_lo_for_pc(&self, pc: u32) -> u32 {
        Symbolizer::entry_lo_for_pc(self, pc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<TraceSymbol> {
        vec![
            TraceSymbol {
                addr: 0x1000,
                size: 0x40,
                name: "alpha".into(),
            },
            TraceSymbol {
                addr: 0x1100,
                size: 0x80,
                name: "beta".into(),
            },
            TraceSymbol {
                addr: 0x2000,
                size: 0x10,
                name: "gamma".into(),
            },
        ]
    }

    #[test]
    fn pc_zero_is_root() {
        let f = fixture();
        let s = Symbolizer::new(&f);
        assert_eq!(s.lookup(0), "<root>");
    }

    #[test]
    fn exact_addr_hit() {
        let f = fixture();
        let s = Symbolizer::new(&f);
        assert_eq!(s.lookup(0x1000), "alpha");
    }

    #[test]
    fn last_byte_hit() {
        let f = fixture();
        let s = Symbolizer::new(&f);
        assert_eq!(s.lookup(0x103f), "alpha");
    }

    #[test]
    fn one_past_end_misses() {
        let f = fixture();
        let s = Symbolizer::new(&f);
        assert_eq!(s.lookup(0x1040), "<unknown:0x00001040>");
    }

    #[test]
    fn between_symbols_misses() {
        let f = fixture();
        let s = Symbolizer::new(&f);
        assert_eq!(s.lookup(0x1080), "<unknown:0x00001080>");
    }

    #[test]
    fn ram_pc_is_jit() {
        let f = fixture();
        let s = Symbolizer::new(&f);
        assert_eq!(s.lookup(0x8000_0000 + 0x1234), "<jit:0x80001234>");
    }

    #[test]
    fn rom_pc_is_unknown() {
        let f = fixture();
        let s = Symbolizer::new(&f);
        assert_eq!(s.lookup(0x500), "<unknown:0x00000500>");
    }

    #[test]
    fn boundary_addr_plus_size_minus_one() {
        let f = fixture();
        let s = Symbolizer::new(&f);
        assert_eq!(s.lookup(0x2000 + 0x10 - 1), "gamma");
    }
}
