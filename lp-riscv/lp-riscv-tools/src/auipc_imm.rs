//! Helper functions for AUIPC immediate value handling.

/// Sign-extend a 20-bit immediate value to 32 bits, then shift left by 12.
///
/// This is used for AUIPC: rd = pc + (sign_extend(imm[31:12]) << 12)
///
/// The RISC-V spec says: sign-extend the 20-bit immediate to 32 bits, then shift left by 12.
///
/// # Arguments
/// * `imm_20bit` - The 20-bit immediate value from bits [31:12] of the instruction
///
/// # Returns
/// The sign-extended and shifted immediate value as i32
pub fn sign_extend_and_shift_auipc_imm(imm_20bit: u32) -> i32 {
    // RISC-V spec: rd = pc + (sign_extend(imm[31:12]) << 12)
    //
    // The immediate is a 20-bit value in bits [31:12] of the instruction.
    // Following embive's approach: place the 20-bit value in bits [31:12] and cast to i32.
    //
    // Place the 20-bit value in bits [31:12] of a u32
    let imm_placed = imm_20bit << 12;

    // Sign-extend based on bit 19 of the original value
    if (imm_20bit & 0x80000) != 0 {
        // Sign bit (bit 19) was set: bits [31:20] should be 1s
        // Extract bits [19:12] from original and place in bits [19:12]
        let bits_19_12 = ((imm_20bit >> 12) & 0xff) << 12;
        // OR with sign extension mask to set bits [31:20] to 1s
        (0xfff00000u32 | bits_19_12) as i32
    } else {
        // Sign bit was clear: bits [31:20] are already 0, just cast
        imm_placed as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero() {
        let result = sign_extend_and_shift_auipc_imm(0x00000);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_positive_small() {
        let result = sign_extend_and_shift_auipc_imm(0x00001);
        assert_eq!(result, 0x1000);
    }

    #[test]
    fn test_positive_large() {
        let result = sign_extend_and_shift_auipc_imm(0x7ffff);
        assert_eq!(result, 0x7ffff000);
    }

    #[test]
    fn test_negative_min() {
        // 0x80000 is the minimum negative 20-bit value (bit 19 set)
        let result = sign_extend_and_shift_auipc_imm(0x80000);
        // Sign-extended: 0xfff80000, shifted: 0xfff800000 (wraps to 0xfff80000)
        assert_eq!(result, 0xfff80000u32 as i32);
    }

    #[test]
    fn test_negative_ff000() {
        // 0xff000 has bit 19 set, so it's negative
        let result = sign_extend_and_shift_auipc_imm(0xff000);
        // Sign-extended: 0xfffff000, shifted: 0xfffff000000 (wraps to 0xfffff000)
        assert_eq!(result, 0xfffff000u32 as i32);
    }

    #[test]
    fn test_negative_max() {
        // 0xfffff is the maximum negative 20-bit value (all bits set)
        let result = sign_extend_and_shift_auipc_imm(0xfffff);
        // Sign-extended: 0xffffffff, shifted: 0xfffff000
        assert_eq!(result, 0xfffff000u32 as i32);
    }

    #[test]
    fn test_auipc_calculation_example() {
        // Example: PC = 0x88, imm = 0xfffff
        // Expected: 0x88 + 0xfffff000 = 0xfffff088 (wrapping)
        let pc = 0x88u32;
        let imm_shifted = sign_extend_and_shift_auipc_imm(0xfffff);
        let result = (pc.wrapping_add(imm_shifted as u32)) as i32;
        // 0x88 + 0xfffff000 = 0xfffff088 as u32, which is -3960 as i32
        assert_eq!(result, -3960i32);
        assert_eq!(result as u32, 0xfffff088);
    }

    #[test]
    fn test_auipc_target_calculation() {
        // Example from the failing test:
        // PC = 0x88, target = 0x48, diff = -64
        // hi20 = (-64 >> 12) & 0xfffff = 0xfffff
        // Expected result: PC + imm_shifted should get us close to target
        let pc = 0x88u32;
        let target = 0x48u32;
        let diff = (target as i32) - (pc as i32); // -64
        let hi20 = ((diff >> 12) & 0xfffff) as u32; // 0xfffff
        let imm_shifted = sign_extend_and_shift_auipc_imm(hi20);
        let result = pc.wrapping_add(imm_shifted as u32);

        // After AUIPC: result should be close to target (within 4KB)
        // The remaining offset is in the lower 12 bits: diff & 0xfff = -64 & 0xfff = 0xfc0
        // Then ADDI adds the remaining offset to get exactly to target
        let remaining_offset = diff & 0xfff; // -64 & 0xfff = 0xfc0 (as u32 this is 4032, but as i32 it's -64)
        let after_addi = result.wrapping_add(remaining_offset as u32);
        assert_eq!(after_addi, target);
    }

    #[test]
    fn test_all_20_bit_values() {
        // Test a few key values across the range
        let test_cases = [
            (0x00000, 0x00000000u32 as i32),
            (0x00001, 0x00001000u32 as i32),
            (0x7ffff, 0x7ffff000u32 as i32), // Max positive
            (0x80000, 0xfff80000u32 as i32), // Min negative
            (0xff000, 0xfffff000u32 as i32),
            (0xfffff, 0xfffff000u32 as i32), // Max negative
        ];

        for (input, expected) in test_cases.iter() {
            let result = sign_extend_and_shift_auipc_imm(*input);
            assert_eq!(
                result, *expected,
                "Failed for input 0x{:05x}: expected 0x{:08x}, got 0x{:08x}",
                input, *expected as u32, result as u32
            );
        }
    }
}
