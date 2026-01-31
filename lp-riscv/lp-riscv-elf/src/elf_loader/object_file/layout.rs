//! Layout calculation for object file loading.

extern crate alloc;

use ::object::{Object, ObjectSection};
use alloc::string::String;

/// Object file layout information.
///
/// Specifies where object file sections should be placed in memory
/// relative to the base executable.
pub struct ObjectLayout {
    /// Where to place .text section (after base executable's code_end)
    pub text_placement: u32,
    /// Where to place .data section (after base executable's ram_end, relative to RAM_START)
    pub data_placement: u32,
}

/// Align an address to 4-byte boundary.
#[inline]
fn align_4_bytes(addr: u32) -> u32 {
    (addr + 3) & !3
}

/// Calculate layout for object file sections.
///
/// Determines where to place object file sections after the base executable.
///
/// # Arguments
///
/// * `obj` - The object file to calculate layout for
/// * `base_code_end` - End address of base executable's code sections
/// * `base_ram_end` - End offset of base executable's RAM sections (relative to RAM_START)
///
/// # Returns
///
/// Layout information with placement addresses, or an error if calculation fails.
pub fn calculate_object_layout(
    obj: &::object::File,
    base_code_end: u32,
    base_ram_end: u32,
) -> Result<ObjectLayout, String> {
    debug!("=== Calculating object file layout ===");
    debug!(
        "Base code end: 0x{:x}, Base RAM end offset: 0x{:x}",
        base_code_end, base_ram_end
    );

    // Find .text and .data sections
    let mut text_size: u64 = 0;
    let mut data_size: u64 = 0;

    for section in obj.sections() {
        let section_name = match section.name() {
            Ok(name) => name,
            Err(_) => continue,
        };

        // Skip debug sections
        if section_name.starts_with(".debug_") || section_name.starts_with(".zdebug_") {
            continue;
        }

        match section_name {
            ".text" => {
                text_size = section.size();
            }
            ".data" => {
                data_size = section.size();
            }
            _ => {}
        }
    }

    // Calculate placement addresses
    // text_placement: Start after base code_end, aligned to 4 bytes
    let text_placement = align_4_bytes(base_code_end);

    // data_placement: Start after base ram_end, aligned to 4 bytes (relative to RAM_START)
    let data_placement = align_4_bytes(base_ram_end);

    debug!(
        "Object layout: .text at 0x{:x} (size={}), .data at offset 0x{:x} (size={})",
        text_placement, text_size, data_placement, data_size
    );

    Ok(ObjectLayout {
        text_placement,
        data_placement,
    })
}
