//! Backtrace symbolication using ELF symbol map.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use hashbrown::HashMap;
use rustc_demangle::demangle;

use super::memory::RAM_START;

/// Resolve an address to its containing symbol.
///
/// Returns the symbol with the largest address <= `addr` (i.e. the function containing this address).
/// Only considers code symbols (address < RAM_START).
///
/// # Returns
/// `Some((symbol_name, offset_from_symbol_start))` if a containing symbol is found
pub fn resolve_address(
    symbol_map: &HashMap<String, u32>,
    addr: u32,
    code_end: u32,
) -> Option<(String, u32)> {
    if addr >= RAM_START {
        return None;
    }
    if addr >= code_end {
        return None;
    }

    // Build sorted list of (addr, name) for code symbols
    let mut sorted: Vec<(u32, String)> = symbol_map
        .iter()
        .filter(|(_, a)| **a < RAM_START)
        .map(|(n, a)| (*a, n.clone()))
        .collect();
    sorted.sort_by_key(|(a, _)| *a);

    if sorted.is_empty() {
        return None;
    }

    // Binary search for largest addr' <= addr
    let idx = match sorted.binary_search_by_key(&addr, |(a, _)| *a) {
        Ok(i) => i,
        Err(0) => return None,
        Err(i) => i - 1,
    };

    let (sym_addr, sym_name) = &sorted[idx];
    let offset = addr.wrapping_sub(*sym_addr);
    Some((sym_name.clone(), offset))
}

/// Format a symbol name, demangling Rust symbols (e.g. _ZN4core... -> core::...).
fn format_symbol_name(name: &str) -> String {
    if name.starts_with("_Z") {
        format!("{}", demangle(name))
    } else {
        name.to_string()
    }
}

/// Format a list of addresses as a backtrace string.
///
/// Each line shows frame index, address, and symbol (if resolved).
/// Rust mangled symbols are demangled for readability.
pub fn format_backtrace(
    addresses: &[u32],
    symbol_map: &HashMap<String, u32>,
    code_end: u32,
) -> String {
    let mut result = String::new();
    for (i, &addr) in addresses.iter().enumerate() {
        let sym_info = match resolve_address(symbol_map, addr, code_end) {
            Some((name, offset)) => {
                let display_name = format_symbol_name(&name);
                if offset == 0 {
                    format!(" in {display_name}")
                } else {
                    format!(" in {display_name} (+0x{offset:x})")
                }
            }
            None => {
                if addr >= RAM_START || addr >= code_end {
                    " (invalid address)".to_string()
                } else {
                    " in ???".to_string()
                }
            }
        };
        result.push_str(&format!("  #{i} 0x{addr:08x}{sym_info}\n"));
    }
    result
}
