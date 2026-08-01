//! GOT (Global Offset Table) entry tracking and management.

use alloc::string::String;
use hashbrown::HashMap;

/// A GOT entry for a symbol.
#[derive(Debug, Clone)]
pub struct GotEntry {
    /// Symbol name
    #[allow(dead_code, reason = "Used for debugging and symbol resolution")]
    pub symbol_name: String,
    /// Address where the GOT entry is located
    pub address: u32,
    /// Whether the entry has been initialized
    pub initialized: bool,
}

/// Tracks GOT entries by symbol name.
#[derive(Debug, Default, Clone)]
pub struct GotTracker {
    entries: HashMap<String, GotEntry>,
}

impl GotTracker {
    /// Create a new GOT tracker.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Add a GOT entry.
    pub fn add_entry(&mut self, symbol_name: String, address: u32) {
        log::trace!("  GOT entry: '{symbol_name}' at 0x{address:x}");
        self.entries.insert(
            symbol_name.clone(),
            GotEntry {
                symbol_name,
                address,
                initialized: false,
            },
        );
    }

    /// Get a GOT entry by symbol name.
    pub fn get_entry(&self, symbol_name: &str) -> Option<&GotEntry> {
        self.entries.get(symbol_name)
    }

    /// Mark a GOT entry as initialized.
    pub fn mark_initialized(&mut self, symbol_name: &str) {
        if let Some(entry) = self.entries.get_mut(symbol_name) {
            entry.initialized = true;
        }
    }

    /// Check if a symbol has a GOT entry.
    pub fn has_entry(&self, symbol_name: &str) -> bool {
        self.entries.contains_key(symbol_name)
    }

    /// Get all GOT entries.
    pub fn entries(&self) -> &HashMap<String, GotEntry> {
        &self.entries
    }
}

/// True for the sections that hold global offset table slots.
///
/// `.got` and `.got.plt` are the psABI names; `-fdata-sections`-style
/// splitting yields `.got.<symbol>` subsections, which are still GOT.
fn is_got_section(section_name: &str) -> bool {
    section_name == ".got" || section_name.starts_with(".got.")
}

/// Identify GOT entries from R_RISCV_32 relocations.
///
/// A GOT entry is a **slot in a GOT section** initialized to a symbol's
/// address, so that is what this asks: the relocation's *location*, not its
/// symbol's spelling.
///
/// This used to key off the symbol name instead — any `R_RISCV_32` against a
/// `__lp_`- or `_ZN`-prefixed symbol was declared a GOT entry. That is a guess
/// about what a relocation means made from what it is called, and it is wrong
/// for the most ordinary use of `R_RISCV_32` there is: a `.rodata` table of
/// pointers into another object. rustc lowers a large `match` returning
/// `&'static str` to exactly that (one relocation per arm against one merged
/// string constant), so a big enough enum turned every arm into a bogus GOT
/// entry — all sharing one key, since the tracker is keyed by symbol name, so
/// they also overwrote each other down to a single surviving slot address.
/// Nothing about those relocations was GOT-shaped; only their symbols' names
/// were suggestive.
pub fn identify_got_entries(relocations: &[super::phase1::RelocationInfo]) -> GotTracker {
    log::debug!("=== Identifying GOT entries ===");

    let mut tracker = GotTracker::new();

    for reloc in relocations {
        // R_RISCV_32 slots living in a GOT section initialize GOT entries.
        if reloc.r_type == 1 && is_got_section(&reloc.section_name) {
            tracker.add_entry(reloc.symbol_name.clone(), reloc.address);
        }
    }

    log::debug!("Identified {} GOT entries", tracker.entries().len());
    tracker
}
