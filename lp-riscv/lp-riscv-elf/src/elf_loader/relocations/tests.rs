//! Relocation tests built on synthesized objects.
//!
//! These fixtures are written by hand with `object::write` rather than
//! compiled from Rust, so the shapes under test are pinned by the ELF psABI
//! and not by whatever code layout a particular rustc happens to choose.

#![cfg(test)]

extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use object::write::{
    Object as WriteObject, Relocation as WriteRelocation, StandardSection, Symbol as WriteSymbol,
    SymbolSection,
};
use object::{
    Architecture, BinaryFormat, Endianness, RelocationFlags, SymbolFlags, SymbolKind, SymbolScope,
    elf,
};

use crate::elf_loader::load_elf;

/// A `.rodata` table of pointers into one merged string constant — the shape
/// rustc emits for a large `match` returning `&'static str` — must come back
/// with every element pointing at its own string.
///
/// Before the fix, `R_RISCV_32` was applied as `S`, dropping the addend that
/// carries the per-element offset, so all `TABLE_LEN` entries pointed at the
/// blob's base address. That is invisible for a small enum (rustc lowers those
/// to branches) and only appears once the match is big enough to become a
/// `.rodata` lookup table, which is why it stayed latent.
#[test]
fn abs32_table_entries_keep_their_addends() {
    const TABLE_LEN: usize = 200;
    const STRIDE: usize = 7;

    let blob: Vec<u8> = (0..TABLE_LEN * STRIDE)
        .map(|i| b'a' + (i % 26) as u8)
        .collect();
    let (elf_bytes, blob_sym, table_sym) = string_table_object(&blob, TABLE_LEN, STRIDE);

    let info = load_elf(&elf_bytes).expect("synthesized object should load");
    let blob_addr = *info
        .symbol_map
        .get(&blob_sym)
        .expect("blob symbol in symbol map");
    let table_addr = *info
        .symbol_map
        .get(&table_sym)
        .expect("table symbol in symbol map") as usize;

    let entries: Vec<u32> = (0..TABLE_LEN)
        .map(|i| {
            let at = table_addr + i * 4;
            u32::from_le_bytes(info.code[at..at + 4].try_into().unwrap())
        })
        .collect();

    let expected: Vec<u32> = (0..TABLE_LEN)
        .map(|i| blob_addr + (i * STRIDE) as u32)
        .collect();

    assert_eq!(
        entries, expected,
        "each R_RISCV_32 entry must be S + A, not S"
    );

    // The failure this pins is *collapse*: without the addend every entry
    // holds the same value. Assert distinctness directly so a future
    // regression cannot pass by getting the first entry right.
    let mut unique = entries.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        TABLE_LEN,
        "table entries collapsed onto {} distinct addresses",
        unique.len()
    );
}

/// A `.rodata` pointer table is not a global offset table, however its target
/// symbols are spelled. The old classifier keyed on `__lp_` / `_ZN` name
/// prefixes and swallowed exactly this case.
#[test]
fn rodata_pointer_table_is_not_a_got() {
    const TABLE_LEN: usize = 8;
    const STRIDE: usize = 4;

    let blob: Vec<u8> = vec![b'x'; TABLE_LEN * STRIDE];
    // The symbol name is deliberately `__lp_`-prefixed: it is what the old
    // heuristic keyed on, and it must no longer matter.
    let (elf_bytes, blob_sym, table_sym) = string_table_object(&blob, TABLE_LEN, STRIDE);
    assert!(
        blob_sym.starts_with("__lp_"),
        "fixture guards the heuristic"
    );

    let relocs = analyze(&elf_bytes);
    assert!(
        relocs.iter().all(|(section, _)| section == ".rodata"),
        "fixture should only relocate .rodata"
    );

    let info = load_elf(&elf_bytes).expect("synthesized object should load");
    let blob_addr = *info.symbol_map.get(&blob_sym).unwrap();
    let table_addr = *info.symbol_map.get(&table_sym).unwrap() as usize;
    for i in 0..TABLE_LEN {
        let at = table_addr + i * 4;
        let word = u32::from_le_bytes(info.code[at..at + 4].try_into().unwrap());
        assert_eq!(word, blob_addr + (i * STRIDE) as u32);
    }
}

/// `auipc` + `lw` must agree on the address they are jointly computing.
/// `handle_pcrel_hi20` folds the addend into the high half; the low half has
/// to fold in the same one or the pair lands `A` bytes off target.
#[test]
fn pcrel_pair_folds_the_addend_into_both_halves() {
    for addend in [0i64, 4, 0x123, 0x7ff, 0x800, 0x1234] {
        let (elf_bytes, data_sym) = pcrel_pair_object(addend);
        let info = load_elf(&elf_bytes).expect("synthesized object should load");
        let target = *info.symbol_map.get(&data_sym).unwrap();

        // The pair lives at .text offsets 4 (auipc) and 8 (second instruction).
        let auipc = u32::from_le_bytes(info.code[4..8].try_into().unwrap());
        let lo = u32::from_le_bytes(info.code[8..12].try_into().unwrap());

        let hi20 = (auipc >> 12) & 0xF_FFFF;
        let hi20_signed = if hi20 & 0x8_0000 != 0 {
            (hi20 | 0xFFF0_0000) as i32
        } else {
            hi20 as i32
        };
        let auipc_result = 4u32.wrapping_add((hi20_signed << 12) as u32);

        let imm12 = (lo >> 20) & 0xFFF;
        let imm12_signed = if imm12 & 0x800 != 0 {
            (imm12 | 0xFFFF_F000) as i32
        } else {
            imm12 as i32
        };
        let effective = auipc_result.wrapping_add(imm12_signed as u32);

        assert_eq!(
            effective,
            target.wrapping_add(addend as u32),
            "auipc/lo12 pair resolved to 0x{effective:x} for addend {addend} \
             (symbol 0x{target:x})"
        );
    }
}

/// Collect `(section_name, symbol_name)` for every relocation the loader sees.
fn analyze(elf_bytes: &[u8]) -> Vec<(String, String)> {
    use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget};
    let obj = object::File::parse(elf_bytes).unwrap();
    let mut out = Vec::new();
    for section in obj.sections() {
        let name = section.name().unwrap_or("<unnamed>");
        for (_, reloc) in section.relocations() {
            let sym = match reloc.target() {
                RelocationTarget::Symbol(idx) => obj
                    .symbol_by_index(idx)
                    .ok()
                    .and_then(|s| s.name().ok().map(String::from))
                    .unwrap_or_default(),
                _ => String::new(),
            };
            out.push((String::from(name), sym));
        }
    }
    out
}

/// Build a relocatable object whose single `.rodata` section holds a string
/// blob followed by a `TABLE_LEN`-entry pointer table, one `R_RISCV_32` per
/// entry against the blob with addend `i * stride`.
///
/// Returns `(elf bytes, blob symbol name, table symbol name)`. The object has
/// no `.text`, so `load_elf` places `.rodata` at ROM offset 0 and symbol
/// addresses in the returned map are the addresses the table entries should
/// hold.
fn string_table_object(blob: &[u8], table_len: usize, stride: usize) -> (Vec<u8>, String, String) {
    let mut obj = WriteObject::new(BinaryFormat::Elf, Architecture::Riscv32, Endianness::Little);
    let rodata = obj.section_id(StandardSection::ReadOnlyData);

    // Lead padding: the loader rejects a relocation target that resolves to
    // address 0 as undefined, and this object is placed at ROM offset 0.
    obj.append_section_data(rodata, &[0xFFu8; 16], 4);
    let blob_off = obj.append_section_data(rodata, blob, 4);
    let table_off = obj.append_section_data(rodata, &vec![0u8; table_len * 4], 4);

    let blob_name = String::from("__lp_string_blob");
    let table_name = String::from("__lp_string_table");

    let blob_sym = obj.add_symbol(WriteSymbol {
        name: blob_name.clone().into_bytes(),
        value: blob_off,
        size: blob.len() as u64,
        kind: SymbolKind::Data,
        scope: SymbolScope::Linkage,
        weak: false,
        section: SymbolSection::Section(rodata),
        flags: SymbolFlags::None,
    });
    obj.add_symbol(WriteSymbol {
        name: table_name.clone().into_bytes(),
        value: table_off,
        size: (table_len * 4) as u64,
        kind: SymbolKind::Data,
        scope: SymbolScope::Linkage,
        weak: false,
        section: SymbolSection::Section(rodata),
        flags: SymbolFlags::None,
    });

    for i in 0..table_len {
        obj.add_relocation(
            rodata,
            WriteRelocation {
                offset: table_off + (i * 4) as u64,
                symbol: blob_sym,
                addend: (i * stride) as i64,
                flags: RelocationFlags::Elf {
                    r_type: elf::R_RISCV_32,
                },
            },
        )
        .expect("add R_RISCV_32");
    }

    (obj.write().expect("write elf"), blob_name, table_name)
}

/// Build a relocatable object with `nop; auipc a0, 0; lw a0, 0(a0)` in `.text`
/// and a PCREL_HI20 / PCREL_LO12_I pair pointing at a `.rodata` symbol with
/// the given addend.
///
/// Returns `(elf bytes, data symbol name)`.
fn pcrel_pair_object(addend: i64) -> (Vec<u8>, String) {
    let mut obj = WriteObject::new(BinaryFormat::Elf, Architecture::Riscv32, Endianness::Little);

    // `.text` is created first so the loader places it at ROM offset 0 and the
    // instruction pair sits at the offsets the assertions read.
    let text = obj.section_id(StandardSection::Text);
    // nop; auipc a0, 0; lw a0, 0(a0)
    let code: [u8; 12] = [
        0x13, 0x00, 0x00, 0x00, // addi x0, x0, 0
        0x17, 0x05, 0x00, 0x00, // auipc a0, 0
        0x03, 0x25, 0x05, 0x00, // lw a0, 0(a0)
    ];
    obj.append_section_data(text, &code, 4);

    // Lead padding keeps the data symbol away from address 0, which the loader
    // treats as an unresolved symbol.
    let rodata = obj.section_id(StandardSection::ReadOnlyData);
    obj.append_section_data(rodata, &vec![0u8; 0x40], 4);
    let data_off = obj.append_section_data(rodata, &vec![0xAB; 0x4000], 4);
    let data_name = String::from("__lp_pcrel_target");
    let data_sym = obj.add_symbol(WriteSymbol {
        name: data_name.clone().into_bytes(),
        value: data_off,
        size: 0x4000,
        kind: SymbolKind::Data,
        scope: SymbolScope::Linkage,
        weak: false,
        section: SymbolSection::Section(rodata),
        flags: SymbolFlags::None,
    });

    // The label the LO12 relocation names, as the assembler would emit it.
    let label = String::from("__lp_pcrel_hi_label");
    let label_sym = obj.add_symbol(WriteSymbol {
        name: label.into_bytes(),
        value: 4,
        size: 0,
        kind: SymbolKind::Label,
        scope: SymbolScope::Compilation,
        weak: false,
        section: SymbolSection::Section(text),
        flags: SymbolFlags::None,
    });

    // NOTE: the loader's HI20 numbering does not match the psABI — it routes
    // `handle_pcrel_hi20` from type 20, which is really `R_RISCV_GOT_HI20`,
    // and rejects the psABI's `R_RISCV_PCREL_HI20` (23) outright. Both
    // handlers do the right thing for a static link, so the mislabel is
    // currently harmless, but a `-C relocation-model=static` object would hit
    // the hard error. Filed as
    // `docs/defects/2026-07-31-elf-loader-riscv-reloc-numbering.md`; this
    // fixture uses the number the loader actually dispatches on.
    const LOADER_PCREL_HI20: u32 = 20;
    obj.add_relocation(
        text,
        WriteRelocation {
            offset: 4,
            symbol: data_sym,
            addend,
            flags: RelocationFlags::Elf {
                r_type: LOADER_PCREL_HI20,
            },
        },
    )
    .expect("add PCREL_HI20");
    obj.add_relocation(
        text,
        WriteRelocation {
            offset: 8,
            symbol: label_sym,
            addend: 0,
            flags: RelocationFlags::Elf {
                r_type: elf::R_RISCV_PCREL_LO12_I,
            },
        },
    )
    .expect("add PCREL_LO12_I");

    (obj.write().expect("write elf"), data_name)
}
