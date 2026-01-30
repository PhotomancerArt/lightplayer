//! Individual relocation type handlers.

use alloc::format;
use alloc::string::String;
use hashbrown::HashMap;

use super::got::GotTracker;
use super::phase1::RelocationInfo;

/// Context needed for applying a relocation.
pub struct RelocationContext<'a> {
    /// Buffer to patch (ROM or RAM slice)
    pub buffer: &'a mut [u8],
    /// PC address (section load address + offset)
    pub pc: u32,
    /// Target symbol address
    pub target_addr: u32,
    /// GOT tracker
    pub got_tracker: &'a GotTracker,
    /// Symbol map
    pub symbol_map: &'a HashMap<String, u32>,
    /// All relocations (for finding related relocations)
    pub all_relocations: Option<&'a [super::phase1::RelocationInfo]>,
}

/// Handle R_RISCV_CALL_PLT (17): Function call via PLT (auipc+jalr pair).
pub fn handle_call_plt(ctx: &mut RelocationContext, reloc: &RelocationInfo) -> Result<(), String> {
    debug!("  Applying R_RISCV_CALL_PLT at 0x{:x}", reloc.address);

    let offset = reloc.offset as usize;
    if offset + 8 > ctx.buffer.len() {
        return Err(format!(
            "CALL_PLT relocation at offset {} requires 8 bytes, but only {} available",
            offset,
            ctx.buffer.len() - offset
        ));
    }

    // Read the two instructions
    let auipc_bytes = &ctx.buffer[offset..offset + 4];
    let jalr_bytes = &ctx.buffer[offset + 4..offset + 8];

    let auipc_word = u32::from_le_bytes([
        auipc_bytes[0],
        auipc_bytes[1],
        auipc_bytes[2],
        auipc_bytes[3],
    ]);
    let jalr_word =
        u32::from_le_bytes([jalr_bytes[0], jalr_bytes[1], jalr_bytes[2], jalr_bytes[3]]);

    // Calculate PC-relative offset
    let pcrel = ctx
        .target_addr
        .wrapping_sub(ctx.pc)
        .wrapping_add(reloc.addend as u32);

    // Calculate new offset encoding
    let new_hi20 = ((pcrel >> 12) + ((pcrel & 0x800) != 0) as u32) & 0xFFFFF;
    let new_lo12 = pcrel & 0xFFF;

    // Patch auipc instruction
    let new_auipc = (auipc_word & 0xFFF) | (new_hi20 << 12);
    ctx.buffer[offset..offset + 4].copy_from_slice(&new_auipc.to_le_bytes());

    // Patch jalr instruction
    let new_jalr = (jalr_word & 0xFFFFF) | (new_lo12 << 20);
    ctx.buffer[offset + 4..offset + 8].copy_from_slice(&new_jalr.to_le_bytes());

    debug!(
        "    PC=0x{:x}, target=0x{:x}, offset=0x{:x}",
        ctx.pc, ctx.target_addr, pcrel
    );
    debug!(
        "    Patched auipc: 0x{:08x} → 0x{:08x}, jalr: 0x{:08x} → 0x{:08x}",
        auipc_word, new_auipc, jalr_word, new_jalr
    );

    Ok(())
}

/// Handle R_RISCV_GOT_HI20 (19): GOT high 20 bits (for auipc instruction).
/// Falls back to direct PC-relative addressing if no GOT entry exists (for object files).
pub fn handle_got_hi20(ctx: &mut RelocationContext, reloc: &RelocationInfo) -> Result<(), String> {
    debug!("  Applying R_RISCV_GOT_HI20 at 0x{:x}", reloc.address);

    let offset = reloc.offset as usize;
    if offset + 4 > ctx.buffer.len() {
        return Err(format!(
            "GOT_HI20 relocation at offset {offset} requires 4 bytes"
        ));
    }

    // Check if we have a GOT entry for this symbol
    if let Some(got_entry) = ctx.got_tracker.get_entry(&reloc.symbol_name) {
        // GOT-based access: compute PC-relative offset from auipc instruction to GOT entry
        let got_offset = got_entry.address.wrapping_sub(ctx.pc);

        // Read instruction
        let inst_bytes = &mut ctx.buffer[offset..offset + 4];
        let inst_word =
            u32::from_le_bytes([inst_bytes[0], inst_bytes[1], inst_bytes[2], inst_bytes[3]]);

        // Extract high 20 bits of the offset (with rounding for bit 11)
        let hi20 = ((got_offset >> 12) + ((got_offset & 0x800) != 0) as u32) & 0xFFFFF;
        let patched = (inst_word & 0xFFF) | (hi20 << 12);
        inst_bytes.copy_from_slice(&patched.to_le_bytes());

        debug!(
            "    Patched auipc: 0x{:08x} → 0x{:08x} (hi20=0x{:x})",
            inst_word, patched, hi20
        );
    } else {
        // No GOT entry: fall back to direct PC-relative addressing (for object files)
        // This happens when symbols are resolved directly without GOT indirection
        // For function calls, we need to patch both auipc and jalr (like R_RISCV_CALL_PLT)
        let pcrel = ctx
            .target_addr
            .wrapping_sub(ctx.pc)
            .wrapping_add(reloc.addend as u32);

        // Check if there's a jalr instruction 4 bytes after auipc (typical call pattern)
        let jalr_offset = offset + 4;
        let has_jalr = jalr_offset + 4 <= ctx.buffer.len();

        if has_jalr {
            // Read jalr instruction
            let jalr_bytes = &ctx.buffer[jalr_offset..jalr_offset + 4];
            let jalr_word =
                u32::from_le_bytes([jalr_bytes[0], jalr_bytes[1], jalr_bytes[2], jalr_bytes[3]]);

            // Check if it's a jalr instruction (opcode bits [6:0] = 0x67)
            if (jalr_word & 0x7F) == 0x67 {
                // This is a call pattern: auipc + jalr
                // Patch auipc with hi20
                let inst_bytes = &mut ctx.buffer[offset..offset + 4];
                let inst_word = u32::from_le_bytes([
                    inst_bytes[0],
                    inst_bytes[1],
                    inst_bytes[2],
                    inst_bytes[3],
                ]);

                let hi20 = ((pcrel >> 12) + ((pcrel & 0x800) != 0) as u32) & 0xFFFFF;
                let patched_auipc = (inst_word & 0xFFF) | (hi20 << 12);
                inst_bytes.copy_from_slice(&patched_auipc.to_le_bytes());

                // Patch jalr with lo12
                let lo12 = pcrel & 0xFFF;
                let patched_jalr = (jalr_word & 0xFFFFF) | (lo12 << 20);
                let jalr_bytes_mut = &mut ctx.buffer[jalr_offset..jalr_offset + 4];
                jalr_bytes_mut.copy_from_slice(&patched_jalr.to_le_bytes());

                debug!(
                    "    Patched auipc+jalr call: auipc 0x{:08x} → 0x{:08x} (hi20=0x{:x}), jalr 0x{:08x} → 0x{:08x} (lo12=0x{:x})",
                    inst_word, patched_auipc, hi20, jalr_word, patched_jalr, lo12
                );
                return Ok(());
            }
        }

        // Fall back to just patching auipc (for non-call uses)
        let inst_bytes = &mut ctx.buffer[offset..offset + 4];
        let inst_word =
            u32::from_le_bytes([inst_bytes[0], inst_bytes[1], inst_bytes[2], inst_bytes[3]]);

        // Extract the high 20 bits of the PC-relative offset
        let hi20 = ((pcrel >> 12) + ((pcrel & 0x800) != 0) as u32) & 0xFFFFF;
        let patched = (inst_word & 0xFFF) | (hi20 << 12);
        inst_bytes.copy_from_slice(&patched.to_le_bytes());

        debug!(
            "    Patched auipc: 0x{:08x} → 0x{:08x} (hi20=0x{:x})",
            inst_word, patched, hi20
        );
    }

    Ok(())
}

/// Handle R_RISCV_PCREL_HI20 (20): PC-relative high 20 bits (may be used for GOT).
pub fn handle_pcrel_hi20(
    ctx: &mut RelocationContext,
    reloc: &RelocationInfo,
) -> Result<(), String> {
    debug!("  Applying R_RISCV_PCREL_HI20 at 0x{:x}", reloc.address);

    let offset = reloc.offset as usize;
    if offset + 4 > ctx.buffer.len() {
        return Err(format!(
            "PCREL_HI20 relocation at offset {offset} requires 4 bytes"
        ));
    }

    // Check if this is a GOT access
    // Only treat as GOT if there's actually a GOT entry for this symbol
    let is_got_access = ctx.got_tracker.has_entry(&reloc.symbol_name);

    if is_got_access {
        // This is a GOT access - use GOT entry address
        let got_entry = ctx.got_tracker.get_entry(&reloc.symbol_name)
            .ok_or_else(|| format!(
                "PCREL_HI20 (GOT) relocation at offset 0x{:x} targets '{}', but no GOT entry found",
                reloc.offset, reloc.symbol_name
            ))?;

        // Compute PC-relative offset from auipc to GOT entry
        let got_offset = got_entry.address.wrapping_sub(ctx.pc);

        let inst_bytes = &mut ctx.buffer[offset..offset + 4];
        let inst_word =
            u32::from_le_bytes([inst_bytes[0], inst_bytes[1], inst_bytes[2], inst_bytes[3]]);

        // Extract high 20 bits of the offset (with rounding for bit 11)
        let hi20 = ((got_offset >> 12) + ((got_offset & 0x800) != 0) as u32) & 0xFFFFF;
        let patched = (inst_word & 0xFFF) | (hi20 << 12);
        inst_bytes.copy_from_slice(&patched.to_le_bytes());

        debug!(
            "    Patched auipc: 0x{:08x} → 0x{:08x} (hi20=0x{:x})",
            inst_word, patched, hi20
        );
    } else {
        // Regular PCREL_HI20 relocation
        let pcrel = ctx
            .target_addr
            .wrapping_sub(ctx.pc)
            .wrapping_add(reloc.addend as u32);

        let inst_bytes = &mut ctx.buffer[offset..offset + 4];
        let inst_word =
            u32::from_le_bytes([inst_bytes[0], inst_bytes[1], inst_bytes[2], inst_bytes[3]]);

        // Extract the high 20 bits of the PC-relative offset
        let hi20 = ((pcrel >> 12) + ((pcrel & 0x800) != 0) as u32) & 0xFFFFF;
        let patched = (inst_word & 0xFFF) | (hi20 << 12);
        inst_bytes.copy_from_slice(&patched.to_le_bytes());

        debug!(
            "    Patched auipc: 0x{:08x} → 0x{:08x} (hi20=0x{:x})",
            inst_word, patched, hi20
        );
    }

    Ok(())
}

/// Handle R_RISCV_PCREL_LO12_I (21, 24): PC-relative low 12 bits (for lw instruction).
pub fn handle_pcrel_lo12_i(
    ctx: &mut RelocationContext,
    reloc: &RelocationInfo,
) -> Result<(), String> {
    debug!("  Applying R_RISCV_PCREL_LO12_I at 0x{:x}", reloc.address);

    let offset = reloc.offset as usize;
    if offset + 4 > ctx.buffer.len() {
        return Err(format!(
            "PCREL_LO12_I relocation at offset {offset} requires 4 bytes"
        ));
    }

    // Read the instruction first (before any mutable borrows)
    let inst_word = u32::from_le_bytes([
        ctx.buffer[offset],
        ctx.buffer[offset + 1],
        ctx.buffer[offset + 2],
        ctx.buffer[offset + 3],
    ]);

    // Extract the immediate field (bits [31:20])
    let current_imm = (inst_word >> 20) & 0xFFF;
    debug!(
        "    Instruction=0x{:08x}, current_imm=0x{:x} ({})",
        inst_word, current_imm, current_imm
    );

    // Check if this is a GOT access (immediate is 12, which is typical for GOT)
    if current_imm == 12 && ctx.got_tracker.has_entry(&reloc.symbol_name) {
        // This is a GOT access - the target label is the auipc, and the GOT entry is 12 bytes after it
        let got_entry = ctx.got_tracker.get_entry(&reloc.symbol_name)
            .ok_or_else(|| format!(
                "PCREL_LO12_I (GOT) relocation at offset 0x{:x} targets '{}', but no GOT entry found",
                reloc.offset, reloc.symbol_name
            ))?;

        // The auipc address is got_entry.address - 12
        let auipc_addr = got_entry.address.wrapping_sub(12);
        let lw_pc = ctx.pc;

        // Compute offset from lw to GOT entry
        let offset_to_got = got_entry.address.wrapping_sub(lw_pc);

        debug!(
            "    PCREL_LO12_I (GOT): lw PC=0x{:x}, auipc label=0x{:x}, GOT entry=0x{:x}, offset=0x{:x} (signed: {})",
            lw_pc, auipc_addr, got_entry.address, offset_to_got, offset_to_got as i32
        );

        // Extract low 12 bits of the offset
        let lo12 = offset_to_got & 0xFFF;
        let patched = (inst_word & 0xFFFFF) | (lo12 << 20);
        let inst_bytes = &mut ctx.buffer[offset..offset + 4];
        inst_bytes.copy_from_slice(&patched.to_le_bytes());

        debug!(
            "    Patched lw instruction for GOT: 0x{:08x} → 0x{:08x} (lo12=0x{:x}, imm was 12)",
            inst_word, patched, lo12
        );
    } else {
        // Regular PCREL_LO12_I relocation
        // The target is the auipc label (.L0_XX), which is at the auipc PC
        // The auipc (already patched by PCREL_HI20) computes: auipc_result = auipc_pc + (hi20 << 12)
        // The lw loads from: (auipc_result) + lo12
        // We need to read the hi20 from the auipc instruction to compute auipc_result
        // Then: lo12 = (final_target - auipc_result) & 0xFFF
        // The PCREL_HI20 relocation targeted the final symbol, so we need to find it

        // The auipc instruction is 4 bytes before the lw instruction
        // The auipc PC is the address of the auipc instruction, which is reloc.address - 4
        let auipc_pc = ctx.pc.wrapping_sub(4); // auipc is 4 bytes before lw
        let auipc_buffer_offset = offset.wrapping_sub(4); // auipc is 4 bytes before lw

        if auipc_buffer_offset >= ctx.buffer.len() || auipc_buffer_offset + 4 > ctx.buffer.len() {
            return Err(format!(
                "Cannot read auipc instruction for PCREL_LO12_I: auipc would be at buffer offset {auipc_buffer_offset}"
            ));
        }

        let auipc_word = u32::from_le_bytes([
            ctx.buffer[auipc_buffer_offset],
            ctx.buffer[auipc_buffer_offset + 1],
            ctx.buffer[auipc_buffer_offset + 2],
            ctx.buffer[auipc_buffer_offset + 3],
        ]);

        // Extract hi20 from the already-patched auipc instruction
        let hi20 = (auipc_word >> 12) & 0xFFFFF;

        // Sign-extend hi20 properly: if bit 19 is set, it's negative
        let hi20_signed = if (hi20 & 0x80000) != 0 {
            (hi20 | 0xFFF00000) as i32
        } else {
            hi20 as i32
        };

        // Compute auipc_result = auipc_pc + (hi20 << 12)
        let auipc_result = auipc_pc.wrapping_add((hi20_signed << 12) as u32);

        // For PCREL_LO12_I, the target is the label (.L0_XX), but we need the final target
        // Find the corresponding PCREL_HI20 or GOT_HI20 relocation that comes before this one
        // It should be at auipc_pc and target the actual symbol
        let (final_target, is_got_without_entry) = if let Some(all_relocs) = ctx.all_relocations {
            // Find PCREL_HI20 (20) or GOT_HI20 (19) relocation at auipc_pc
            let hi20_reloc = all_relocs
                .iter()
                .find(|r| (r.r_type == 20 || r.r_type == 19) && r.address == auipc_pc)
                .ok_or_else(|| format!(
                    "Could not find corresponding PCREL_HI20 or GOT_HI20 relocation for PCREL_LO12_I at 0x{:x} (looking for relocation at auipc_pc=0x{:x})",
                    reloc.address, auipc_pc
                ))?;
            let target = ctx
                .symbol_map
                .get(&hi20_reloc.symbol_name)
                .copied()
                .ok_or_else(|| {
                    format!(
                        "Could not resolve symbol '{}' for {} relocation at 0x{:x}",
                        hi20_reloc.symbol_name,
                        if hi20_reloc.r_type == 19 {
                            "GOT_HI20"
                        } else {
                            "PCREL_HI20"
                        },
                        auipc_pc
                    )
                })?;
            // Check if this is GOT_HI20 or PCREL_HI20 without a GOT entry (need to convert lw to addi)
            // For function calls, we convert lw to addi to compute address directly instead of loading from memory
            let has_got_entry = ctx.got_tracker.has_entry(&hi20_reloc.symbol_name);
            // Convert to addi if: (1) GOT_HI20 without GOT entry, or (2) PCREL_HI20 without GOT entry (direct function call)
            let is_got_without_entry =
                (hi20_reloc.r_type == 19 || hi20_reloc.r_type == 20) && !has_got_entry;
            (target, is_got_without_entry)
        } else {
            return Err(format!(
                "PCREL_LO12_I at 0x{:x} requires all_relocations to find corresponding PCREL_HI20 or GOT_HI20",
                reloc.address
            ));
        };

        // According to RISC-V ELF psabi doc:
        // LO12: symbol_address - hi20_reloc_offset (low 12 bits extracted)
        // Where hi20_reloc_offset is the PC of the auipc instruction.
        // The auipc computes: auipc_result = auipc_pc + (hi20 << 12)
        // The lw loads from: (auipc_result) + lo12
        // So: final_target = auipc_result + lo12
        // Therefore: lo12 = (final_target - auipc_result) & 0xFFF
        let lo12 = (final_target.wrapping_sub(auipc_result)) & 0xFFF;

        if is_got_without_entry {
            // For GOT_HI20/PCREL_HI20 without a GOT entry, convert lw to addi to compute address directly
            // lw: bits [6:0]=0000011 (0x03), bits [14:12]=010 (funct3=2)
            // addi: bits [6:0]=0010011 (0x13), bits [14:12]=000 (funct3=0)
            // Clear bits [14:12] (funct3) and bits [6:0] (opcode), then set to addi opcode
            // Mask: clear bits [14:12] (0x7000) and bits [6:0] (0x7F), keep everything else
            // 0xFFFF8F80 = clears bits [14:12] and [6:0]
            let patched = (inst_word & 0xFFFF8F80) | (lo12 << 20) | 0x00000013;
            let inst_bytes = &mut ctx.buffer[offset..offset + 4];
            inst_bytes.copy_from_slice(&patched.to_le_bytes());

            debug!(
                "    Converted instruction: 0x{:08x} → 0x{:08x} (lw → addi, lo12=0x{:x})",
                inst_word, patched, lo12
            );
        } else {
            // Regular PCREL_LO12_I: patch the lw instruction
            let patched = (inst_word & 0xFFFFF) | (lo12 << 20);
            let inst_bytes = &mut ctx.buffer[offset..offset + 4];
            inst_bytes.copy_from_slice(&patched.to_le_bytes());

            debug!(
                "    Patched lw instruction: 0x{:08x} → 0x{:08x} (lo12=0x{:x})",
                inst_word, patched, lo12
            );
        }
    }

    Ok(())
}

/// Handle R_RISCV_32 (1): 32-bit absolute relocation (used for GOT entry initialization).
#[allow(dead_code, reason = "Reserved for future GOT handling")]
pub fn handle_abs32(
    ctx: &mut RelocationContext,
    reloc: &RelocationInfo,
    got_tracker: &mut GotTracker,
) -> Result<(), String> {
    debug!("  Applying R_RISCV_32 at 0x{:x}", reloc.address);

    let offset = reloc.offset as usize;
    if offset + 4 > ctx.buffer.len() {
        return Err(format!(
            "R_RISCV_32 relocation at offset {offset} requires 4 bytes"
        ));
    }

    // Write the absolute target address directly
    let reloc_bytes = &mut ctx.buffer[offset..offset + 4];
    reloc_bytes.copy_from_slice(&ctx.target_addr.to_le_bytes());

    // If this is a GOT entry, mark it as initialized
    if got_tracker.has_entry(&reloc.symbol_name) {
        got_tracker.mark_initialized(&reloc.symbol_name);
        debug!(
            "    ✓ GOT entry initialized: '{}' = 0x{:x}",
            reloc.symbol_name, ctx.target_addr
        );
    } else {
        debug!(
            "    Wrote 0x{:x} to offset 0x{:x} for '{}'",
            ctx.target_addr, offset, reloc.symbol_name
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    /// Test PCREL_LO12_I calculation for the exact failing case
    #[test]
    fn test_pcrel_lo12_i_calculation() {
        // Test case: auipc at 0x184c, symbol at 0xfa4
        let auipc_pc: u32 = 0x184c;
        let symbol_addr: u32 = 0xfa4;

        // PCREL_HI20 would compute: hi20 = ((symbol_addr - auipc_pc + 0x800) >> 12) & 0xFFFFF
        let offset = symbol_addr.wrapping_sub(auipc_pc);
        let hi20 = ((offset.wrapping_add(0x800)) >> 12) & 0xFFFFF;

        // Sign-extend hi20
        let hi20_signed = if (hi20 & 0x80000) != 0 {
            (hi20 | 0xFFF00000) as i32
        } else {
            hi20 as i32
        };
        let auipc_result = auipc_pc.wrapping_add((hi20_signed << 12) as u32);

        // PCREL_LO12_I: lo12 = (symbol_addr - auipc_result) & 0xFFF
        let lo12 = (symbol_addr.wrapping_sub(auipc_result)) & 0xFFF;

        // Verify: auipc_result + lo12 should equal symbol_addr
        let effective_addr = auipc_result.wrapping_add(lo12);
        assert_eq!(
            effective_addr, symbol_addr,
            "auipc_result=0x{auipc_result:x}, lo12=0x{lo12:x}, effective=0x{effective_addr:x}, expected=0x{symbol_addr:x}"
        );
    }

    /// Test PCREL_HI20 calculation with rounding
    #[test]
    fn test_pcrel_hi20_calculation() {
        // Test positive offset
        let auipc_pc: u32 = 0x1000;
        let symbol_addr: u32 = 0x2000;
        let offset = symbol_addr.wrapping_sub(auipc_pc);
        let hi20 = ((offset.wrapping_add(0x800)) >> 12) & 0xFFFFF;
        assert_eq!(hi20, 0x1, "hi20 should be 1 for offset 0x1000");

        // Test negative offset
        let auipc_pc: u32 = 0x2000;
        let symbol_addr: u32 = 0x1000;
        let offset = symbol_addr.wrapping_sub(auipc_pc);
        let hi20 = ((offset.wrapping_add(0x800)) >> 12) & 0xFFFFF;
        // For negative offset, we expect a large hi20 value (sign-extended)
        assert_eq!(hi20, 0xFFFFF, "hi20 should be 0xFFFFF for negative offset");
    }

    /// Test PCREL pair calculation for various offsets
    #[test]
    fn test_pcrel_pair() {
        let test_cases: Vec<(u32, u32)> = {
            let mut v = Vec::new();
            v.push((0x1000, 0x2000)); // Positive offset
            v.push((0x2000, 0x1000)); // Negative offset
            v.push((0x184c, 0xfa4)); // The failing test case
            v.push((0x0, 0x1000)); // Near zero
            v.push((0x1000, 0x0)); // Near zero (negative)
            v
        };

        for (auipc_pc, symbol_addr) in test_cases {
            // Compute hi20
            let offset = symbol_addr.wrapping_sub(auipc_pc);
            let hi20 = ((offset.wrapping_add(0x800)) >> 12) & 0xFFFFF;

            // Sign-extend hi20
            let hi20_signed = if (hi20 & 0x80000) != 0 {
                (hi20 | 0xFFF00000) as i32
            } else {
                hi20 as i32
            };

            // Compute auipc_result
            let auipc_result = auipc_pc.wrapping_add((hi20_signed << 12) as u32);

            // Compute lo12
            let lo12 = (symbol_addr.wrapping_sub(auipc_result)) & 0xFFF;

            // Verify: auipc_result + lo12 = symbol_addr
            let effective_addr = auipc_result.wrapping_add(lo12);
            assert_eq!(
                effective_addr, symbol_addr,
                "Failed for auipc_pc=0x{auipc_pc:x}, symbol_addr=0x{symbol_addr:x}: auipc_result=0x{auipc_result:x}, lo12=0x{lo12:x}, effective=0x{effective_addr:x}"
            );
        }
    }
}
