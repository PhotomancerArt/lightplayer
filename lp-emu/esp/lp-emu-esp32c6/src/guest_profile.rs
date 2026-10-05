//! Guest instructions by symbol, from the `blockprof` census (emulator seams
//! M0 part C, spike quality).
//!
//! `blockprof` counts instructions retired per block start. This folds those
//! counts into the app's (or the ROM's) symbol that contains each block
//! start, demangled, so a census can say how much of the guest's work was
//! the USB-Serial-JTAG driver. A block that runs across a symbol boundary is
//! credited to the symbol it starts in — at spike precision that is fine.

use std::collections::BTreeMap;

impl crate::machine::Esp32C6Machine {
    /// `(symbol, instructions retired)` for every symbol that retired any,
    /// largest first. `None` unless the run asked for `blockprof`.
    pub fn blockprof_symbols(&self) -> Option<Vec<(String, u64)>> {
        let prof = self.harts[0].blockprof()?;
        let mut by: BTreeMap<String, u64> = BTreeMap::new();
        for (pc, _, retired) in prof.iter() {
            let name = self
                .app()
                .and_then(|a| a.symbol_at(pc))
                .or_else(|| self.rom().symbol_at(pc))
                .map(|s| format!("{:#}", rustc_demangle::demangle(&s.name)))
                .unwrap_or_else(|| format!("?{pc:#010x}"));
            *by.entry(name).or_default() += retired;
        }
        let mut out: Vec<(String, u64)> = by.into_iter().collect();
        out.sort_unstable_by(|a, b| b.1.cmp(&a.1));
        Some(out)
    }

    /// Every block start: `(pc, entries, retired, symbol+offset)`, by pc.
    pub fn blockprof_rows(&self) -> Option<Vec<(u32, u64, u64, String)>> {
        let prof = self.harts[0].blockprof()?;
        let mut rows: Vec<(u32, u64, u64, String)> = prof
            .iter()
            .map(|(pc, entries, retired)| {
                let sym = self
                    .app()
                    .and_then(|a| a.symbol_at(pc))
                    .or_else(|| self.rom().symbol_at(pc))
                    .map(|s| format!("{:#}", rustc_demangle::demangle(&s.name)))
                    .unwrap_or_default();
                (pc, entries, retired, sym)
            })
            .collect();
        rows.sort_unstable_by_key(|r| r.0);
        Some(rows)
    }
}
