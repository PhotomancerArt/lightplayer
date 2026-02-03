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

/// Identify GOT entries from R_RISCV_32 relocations.
/// GOT entries are initialized with R_RISCV_32 relocations that write symbol addresses.
pub fn identify_got_entries(relocations: &[super::phase1::RelocationInfo]) -> GotTracker {
    log::debug!("=== Identifying GOT entries ===");

    let mut tracker = GotTracker::new();

    for reloc in relocations {
        // R_RISCV_32 relocations that initialize GOT entries
        if reloc.r_type == 1 {
            // R_RISCV_32
            // Check if this looks like a GOT entry initialization
            // GOT entries are typically at zero-initialized locations
            // and target external symbols (starting with __lp_)
            let is_got_entry =
                reloc.symbol_name.starts_with("__lp_") || reloc.symbol_name.starts_with("_ZN");

            if is_got_entry {
                tracker.add_entry(reloc.symbol_name.clone(), reloc.address);
            }
        }
    }

    log::debug!("Identified {} GOT entries", tracker.entries().len());
    tracker
}
