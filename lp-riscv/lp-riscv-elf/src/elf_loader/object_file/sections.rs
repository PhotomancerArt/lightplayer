//! Section loading for object files.

extern crate alloc;

use ::object::{Object, ObjectSection};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use super::layout::ObjectLayout;

/// Information about where object file sections were placed.
pub struct ObjectSectionPlacement {
    /// Start address where .text section was placed
    pub text_start: u32,
    /// Size of .text section
    pub text_size: usize,
    /// Start address where .data section was placed (relative to RAM_START)
    pub data_start: u32,
    /// Size of .data section
    pub data_size: usize,
}

/// Load object file sections into memory buffers.
///
/// Copies object file sections into the base executable's code/ram buffers
/// at the specified placement addresses.
///
/// # Arguments
///
/// * `obj` - The object file to load sections from
/// * `code` - Mutable reference to code buffer (will be extended if needed)
/// * `ram` - Mutable reference to RAM buffer (will be extended if needed)
/// * `layout` - Layout information specifying where to place sections
///
/// # Returns
///
/// Information about where sections were placed, or an error if loading fails.
pub fn load_object_sections(
    obj: &::object::File,
    code: &mut Vec<u8>,
    ram: &mut Vec<u8>,
    layout: &ObjectLayout,
) -> Result<ObjectSectionPlacement, String> {
    debug!("=== Loading object file sections ===");

    let text_start = layout.text_placement;
    let mut text_size = 0usize;
    let data_start = layout.data_placement;
    let mut data_size = 0usize;

    // First pass: collect all sections, separating main sections from subsections
    let mut text_sections: Vec<(String, Vec<u8>)> = Vec::new();
    let mut data_sections: Vec<(String, Vec<u8>)> = Vec::new();
    let mut rodata_sections: Vec<(String, Vec<u8>)> = Vec::new();
    let mut bss_sections: Vec<(String, usize)> = Vec::new();

    for section in obj.sections() {
        let section_name = match section.name() {
            Ok(name) => name,
            Err(_) => continue,
        };

        // Skip debug sections
        if section_name.starts_with(".debug_") || section_name.starts_with(".zdebug_") {
            continue;
        }

        let section_kind = section.kind();
        let section_size = section.size() as usize;

        // Skip sections with no data (except .bss which we'll handle separately)
        if section_size == 0 && section_kind != ::object::SectionKind::UninitializedData {
            continue;
        }

        if section_name == ".text" || section_name.starts_with(".text.") {
            if section_kind == ::object::SectionKind::Text {
                let data = match section.data() {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                if !data.is_empty() {
                    text_sections.push((section_name.to_string(), data.to_vec()));
                }
            }
        } else if section_name == ".data" || section_name.starts_with(".data.") {
            if section_kind == ::object::SectionKind::Data {
                let data = match section.data() {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                if !data.is_empty() {
                    data_sections.push((section_name.to_string(), data.to_vec()));
                }
            }
        } else if section_name == ".rodata" || section_name.starts_with(".rodata.") {
            if section_kind == ::object::SectionKind::ReadOnlyData {
                let data = match section.data() {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                if !data.is_empty() {
                    rodata_sections.push((section_name.to_string(), data.to_vec()));
                }
            }
        } else if section_name == ".bss" || section_name.starts_with(".bss.") {
            if section_kind == ::object::SectionKind::UninitializedData && section_size > 0 {
                bss_sections.push((section_name.to_string(), section_size));
            }
        }
    }

    // Sort sections: main section first, then subsections alphabetically
    text_sections.sort_by(|a, b| {
        if a.0 == ".text" {
            std::cmp::Ordering::Less
        } else if b.0 == ".text" {
            std::cmp::Ordering::Greater
        } else {
            a.0.cmp(&b.0)
        }
    });
    data_sections.sort_by(|a, b| {
        if a.0 == ".data" {
            std::cmp::Ordering::Less
        } else if b.0 == ".data" {
            std::cmp::Ordering::Greater
        } else {
            a.0.cmp(&b.0)
        }
    });

    // Load .text sections (main + subsections) into code buffer
    let mut current_text_offset = text_start as usize;
    for (section_name, data) in &text_sections {
        // Align to 4 bytes
        current_text_offset = (current_text_offset + 3) & !3;

        // Ensure code buffer is large enough
        let required_size = current_text_offset + data.len();
        if required_size > code.len() {
            code.resize(required_size, 0);
        }

        // Copy section data
        code[current_text_offset..current_text_offset + data.len()].copy_from_slice(data);

        debug!(
            "Loaded section '{}' at offset 0x{:x} ({} bytes)",
            section_name,
            current_text_offset,
            data.len()
        );

        current_text_offset += data.len();
        text_size = current_text_offset - text_start as usize;
    }

    // Load .data sections (main + subsections) into RAM buffer
    let mut current_data_offset = data_start as usize;
    for (section_name, data) in &data_sections {
        // Align to 4 bytes
        current_data_offset = (current_data_offset + 3) & !3;

        // Ensure RAM buffer is large enough
        let required_size = current_data_offset + data.len();
        if required_size > ram.len() {
            ram.resize(required_size, 0);
        }

        // Copy section data
        ram[current_data_offset..current_data_offset + data.len()].copy_from_slice(data);

        debug!(
            "Loaded section '{}' at RAM offset 0x{:x} ({} bytes)",
            section_name,
            current_data_offset,
            data.len()
        );

        current_data_offset += data.len();
        data_size = current_data_offset - data_start as usize;
    }

    // Load .rodata sections into code buffer (after .text)
    for (section_name, data) in &rodata_sections {
        // Place .rodata after .text
        let rodata_start = text_start + text_size as u32;
        let rodata_start_aligned = (rodata_start + 3) & !3; // Align to 4 bytes

        // Ensure code buffer is large enough
        let required_size = (rodata_start_aligned as usize) + data.len();
        if required_size > code.len() {
            code.resize(required_size, 0);
        }

        // Copy section data
        code[rodata_start_aligned as usize..rodata_start_aligned as usize + data.len()]
            .copy_from_slice(data);

        debug!(
            "Loaded section '{}' at offset 0x{:x} ({} bytes)",
            section_name,
            rodata_start_aligned,
            data.len()
        );
    }

    // Load .bss sections into RAM buffer (after .data)
    for (section_name, section_size) in &bss_sections {
        // Place .bss after .data
        let bss_start = data_start + data_size as u32;
        let bss_start_aligned = (bss_start + 3) & !3; // Align to 4 bytes

        // Ensure RAM buffer is large enough
        let required_size = (bss_start_aligned as usize) + section_size;
        if required_size > ram.len() {
            ram.resize(required_size, 0);
        } else {
            // Zero-initialize the .bss region
            ram[bss_start_aligned as usize..bss_start_aligned as usize + section_size].fill(0);
        }

        debug!(
            "Initialized section '{}' at RAM offset 0x{:x} ({} bytes)",
            section_name, bss_start_aligned, section_size
        );
    }

    debug!(
        "Object section loading complete: .text at 0x{:x} ({} bytes), .data at offset 0x{:x} ({} bytes)",
        text_start, text_size, data_start, data_size
    );

    Ok(ObjectSectionPlacement {
        text_start,
        text_size,
        data_start,
        data_size,
    })
}
