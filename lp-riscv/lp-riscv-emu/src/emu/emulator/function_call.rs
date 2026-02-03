//! Function calling logic with ABI setup and argument handling.

extern crate alloc;

use super::super::{
    abi_helper::{self, ArgLocation, ArgSlot, ReturnLocation, ReturnValueLocation},
    error::EmulatorError,
    memory::DEFAULT_RAM_START,
};
use super::state::Riscv32Emulator;
use super::types::StepResult;
use alloc::{format, string::String, vec::Vec};
use cranelift_codegen::data_value::DataValue;
use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{ArgumentPurpose, Signature};
use cranelift_codegen::settings::{self, Configurable, Flags};

impl Riscv32Emulator {
    /// Call a compiled function using the RISC-V calling convention.
    ///
    /// This high-level API handles:
    /// - Setting up arguments in a0-a7 registers (and stack if needed)
    /// - Setting up return address to detect function return
    /// - Executing until function returns
    /// - Extracting return values from a0-a1 (and stack if needed)
    ///
    /// # Arguments
    ///
    /// * `func_entry` - Program counter of function entry point
    /// * `args` - Function arguments as DataValues
    /// * `signature` - Function signature (for ABI conformance)
    ///
    /// # Returns
    ///
    /// Returns the function's return values as DataValues, or an error if execution failed.
    pub fn call_function(
        &mut self,
        func_entry: u32,
        args: &[DataValue],
        signature: &Signature,
    ) -> Result<Vec<DataValue>, EmulatorError> {
        // Clear log buffer for clean output on this function call
        self.clear_logs();
        self.instruction_count = 0;

        // Check if function uses StructReturn
        if has_struct_return(signature) {
            // StructReturn: caller must provide struct size
            // For now, we'll infer it from the number of args provided
            // In practice, the caller should know the struct size
            return Err(EmulatorError::InvalidInstruction {
                pc: self.pc,
                instruction: 0,
                reason: String::from(
                    "StructReturn requires explicit struct size - use call_function_with_struct_return",
                ),
                regs: self.regs,
            });
        }

        // Create flags with enable_multi_ret_implicit_sret enabled
        let flags = create_flags_with_multi_ret()?;

        // Compute return locations to determine if return area is needed
        let return_locations =
            abi_helper::compute_return_locations(signature, &flags).map_err(|e| {
                EmulatorError::InvalidInstruction {
                    pc: self.pc,
                    instruction: 0,
                    reason: format!("Failed to compute return locations: {e:?}"),
                    regs: self.regs,
                }
            })?;

        // Check if return area pointer is needed
        let needs_return_area = needs_return_area(&return_locations);

        // Compute argument locations
        let arg_locations = abi_helper::compute_arg_locations(signature, &flags, needs_return_area)
            .map_err(|e| EmulatorError::InvalidInstruction {
                pc: self.pc,
                instruction: 0,
                reason: format!("Failed to compute argument locations: {e:?}"),
                regs: self.regs,
            })?;

        // Setup stack and return area (needs arg_locations to compute stack space)
        let (entry_sp, return_area_addr, _return_area_size) =
            setup_call_stack(self, &return_locations, &arg_locations)?;

        // Place return area pointer if needed (before arguments)
        if let Some(return_area_addr) = return_area_addr {
            place_return_area_pointer(self, return_area_addr)?;
        }

        // Place arguments
        place_arguments(self, args, &arg_locations, entry_sp)?;

        // Set up return address (ra/x1) to a halt address outside code region
        // When function returns via RET, it will jump to this address
        let code_start = self.memory.code_start();
        let code_size = self.memory.code().len() as u32;
        let code_end = code_start + code_size;
        let halt_address = code_end; // Address just past end of code

        // Reset state for clean function call
        self.pc = func_entry;
        // Don't reset instruction_count - limit should be cumulative across all calls
        self.regs[1] = halt_address as i32; // ra = halt_address

        // Execute until function returns (EBREAK or PC at halt address)
        loop {
            // Check if PC is at halt address (function returned via RET)
            if self.pc == halt_address {
                // Function returned via RET
                break;
            }

            // Check if PC is outside code region
            if self.pc < code_start || self.pc >= code_end {
                // Function returned (PC outside code region)
                break;
            }

            match self.step()? {
                StepResult::Halted => {
                    // Function returned via EBREAK
                    break;
                }
                StepResult::Trap(code) => {
                    return Err(EmulatorError::Trap {
                        code,
                        pc: self.pc,
                        regs: self.regs,
                    });
                }
                StepResult::Panic(info) => {
                    return Err(EmulatorError::Panic {
                        info,
                        pc: self.pc,
                        regs: self.regs,
                    });
                }
                StepResult::Continue => {
                    // Continue execution
                }
                StepResult::Syscall(_) => {
                    return Err(EmulatorError::InvalidInstruction {
                        pc: self.pc,
                        instruction: 0,
                        reason: String::from("Unexpected ECALL in function call"),
                        regs: self.regs,
                    });
                }
                StepResult::FuelExhausted(_) => {
                    // step() never returns FuelExhausted
                    unreachable!("step() should never return FuelExhausted");
                }
            }
        }

        // Extract return values
        extract_return_values(self, &return_locations, entry_sp)
    }

    /// Call a function with StructReturn parameter.
    ///
    /// # Arguments
    ///
    /// * `func_entry` - Program counter of function entry point
    /// * `args` - Function arguments as DataValues (excluding StructReturn pointer)
    /// * `signature` - Function signature (must have StructReturn parameter)
    /// * `struct_size` - Size of the struct return buffer in bytes
    ///
    /// # Returns
    ///
    /// Returns the struct data as Vec<DataValue> (one per word), or an error if execution failed.
    pub fn call_function_with_struct_return(
        &mut self,
        func_entry: u32,
        args: &[DataValue],
        signature: &Signature,
        struct_size: usize,
    ) -> Result<Vec<DataValue>, EmulatorError> {
        if !has_struct_return(signature) {
            return Err(EmulatorError::InvalidInstruction {
                pc: self.pc,
                instruction: 0,
                reason: String::from("Function does not use StructReturn"),
                regs: self.regs,
            });
        }

        // Allocate buffer for struct return
        let buffer_addr = allocate_struct_return_buffer(self, struct_size)?;

        // Place struct return pointer in a0
        place_struct_return_pointer(self, buffer_addr)?;

        // Create flags
        let flags = create_flags_with_multi_ret()?;

        // Compute argument locations (skip StructReturn parameter)
        // StructReturn takes a0, so we don't need return area
        let arg_locations =
            abi_helper::compute_arg_locations(signature, &flags, false).map_err(|e| {
                EmulatorError::InvalidInstruction {
                    pc: self.pc,
                    instruction: 0,
                    reason: format!("Failed to compute argument locations: {e:?}"),
                    regs: self.regs,
                }
            })?;

        // Filter out StructReturn parameter from arg_locations
        // StructReturn is typically the first parameter
        let struct_ret_index = signature
            .params
            .iter()
            .position(|p| p.purpose == ArgumentPurpose::StructReturn)
            .ok_or_else(|| EmulatorError::InvalidInstruction {
                pc: self.pc,
                instruction: 0,
                reason: String::from("StructReturn parameter not found in signature"),
                regs: self.regs,
            })?;

        // Place remaining arguments (skip StructReturn)
        let filtered_arg_locations: Vec<ArgLocation> = arg_locations
            .iter()
            .enumerate()
            .filter_map(|(i, loc)| {
                if i == struct_ret_index {
                    None // Skip StructReturn parameter
                } else {
                    Some(loc.clone())
                }
            })
            .collect();

        // Setup stack (no return area needed for StructReturn)
        let ram_size = self.memory.ram().len();
        let stack_top = DEFAULT_RAM_START + ram_size as u32;
        let entry_sp = (stack_top - 16) as i32; // 16-byte aligned, with some space
        self.regs[2] = entry_sp; // SP register

        // Place arguments
        place_arguments(self, args, &filtered_arg_locations, entry_sp)?;

        // Set up return address (ra/x1) to a halt address outside code region
        let code_start = self.memory.code_start();
        let code_size = self.memory.code().len() as u32;
        let code_end = code_start + code_size;
        let halt_address = code_end; // Address just past end of code

        // Reset state for clean function call
        self.pc = func_entry;
        // Don't reset instruction_count - limit should be cumulative across all calls
        self.regs[1] = halt_address as i32; // ra = halt_address

        // Execute until function returns (EBREAK or PC at halt address)
        loop {
            // Check if PC is at halt address (function returned via RET)
            if self.pc == halt_address {
                // Function returned via RET
                break;
            }

            // Check if PC is outside code region
            if self.pc < code_start || self.pc >= code_end {
                // Function returned (PC outside code region)
                break;
            }

            match self.step()? {
                StepResult::Halted => {
                    // Function returned via EBREAK
                    break;
                }
                StepResult::Trap(code) => {
                    return Err(EmulatorError::Trap {
                        code,
                        pc: self.pc,
                        regs: self.regs,
                    });
                }
                StepResult::Panic(info) => {
                    return Err(EmulatorError::Panic {
                        info,
                        pc: self.pc,
                        regs: self.regs,
                    });
                }
                StepResult::Continue => {
                    // Continue execution
                }
                StepResult::Syscall(_) => {
                    return Err(EmulatorError::InvalidInstruction {
                        pc: self.pc,
                        instruction: 0,
                        reason: String::from("Unexpected ECALL in function call"),
                        regs: self.regs,
                    });
                }
                StepResult::FuelExhausted(_) => {
                    // step() never returns FuelExhausted
                    unreachable!("step() should never return FuelExhausted");
                }
            }
        }

        // Extract struct return value from buffer
        extract_struct_return_value(self, buffer_addr, struct_size)
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Create flags with enable_multi_ret_implicit_sret enabled.
fn create_flags_with_multi_ret() -> Result<Flags, EmulatorError> {
    let mut builder = settings::builder();
    builder
        .set("enable_multi_ret_implicit_sret", "true")
        .map_err(|e| EmulatorError::InvalidInstruction {
            pc: 0,
            instruction: 0,
            reason: format!("Failed to set flags: {e:?}"),
            regs: [0; 32],
        })?;
    Ok(Flags::new(builder))
}

/// Check if signature uses StructReturn parameter.
fn has_struct_return(signature: &Signature) -> bool {
    signature
        .params
        .iter()
        .any(|p| p.purpose == ArgumentPurpose::StructReturn)
}

/// Check if return area pointer is needed.
fn needs_return_area(return_locations: &[ReturnValueLocation]) -> bool {
    return_locations.iter().any(|retval| {
        retval
            .slots
            .iter()
            .any(|slot| matches!(slot, ReturnLocation::Stack(_, _)))
    })
}

/// Calculate size needed for return area on stack.
fn compute_return_area_size(return_locations: &[ReturnValueLocation]) -> u32 {
    let mut max_offset = 0i64;
    for retval in return_locations {
        for slot in &retval.slots {
            if let ReturnLocation::Stack(offset, ty) = slot {
                // Calculate offset needed for this slot
                let slot_offset = *offset + (ty.bits() / 8) as i64;
                if slot_offset > max_offset {
                    max_offset = slot_offset;
                }
            }
        }
    }

    // Ensure 4-byte alignment (word alignment for RISC-V 32)
    if max_offset > 0 {
        ((max_offset as u32 + 3) / 4) * 4
    } else {
        0
    }
}

/// Compute maximum stack space needed for arguments.
fn compute_max_stack_arg_offset(arg_locations: &[ArgLocation]) -> u32 {
    let mut max_offset = 0i64;
    for arg_location in arg_locations {
        for slot in &arg_location.slots {
            if let ArgSlot::Stack(offset, ty) = slot {
                // Calculate offset needed for this slot
                let slot_offset = *offset + (ty.bits() / 8) as i64;
                if slot_offset > max_offset {
                    max_offset = slot_offset;
                }
            }
        }
    }
    // Ensure 16-byte alignment (RISC-V stack alignment requirement)
    if max_offset > 0 {
        ((max_offset as u32 + 15) / 16) * 16
    } else {
        0
    }
}

/// Setup call stack and allocate return area if needed.
fn setup_call_stack(
    emulator: &mut Riscv32Emulator,
    return_locations: &[ReturnValueLocation],
    arg_locations: &[ArgLocation],
) -> Result<(i32, Option<u32>, u32), EmulatorError> {
    let ram_size = emulator.memory.ram().len();
    let ram_start = DEFAULT_RAM_START;
    let ram_end = ram_start + ram_size as u32;

    let needs_ret_area = needs_return_area(return_locations);
    let return_area_size = if needs_ret_area {
        compute_return_area_size(return_locations)
    } else {
        0
    };

    // Compute maximum stack space needed for arguments
    let max_stack_arg_size = compute_max_stack_arg_offset(arg_locations);

    // Total stack space needed: return area + stack args + some padding
    // Stack grows downward, so we allocate from the top
    let total_stack_space = return_area_size + max_stack_arg_size + 64; // 64 bytes padding

    // Ensure we don't exceed RAM
    if total_stack_space > ram_size as u32 {
        return Err(EmulatorError::InvalidInstruction {
            pc: emulator.pc,
            instruction: 0,
            reason: format!(
                "Not enough RAM for stack (need {total_stack_space} bytes, have {ram_size} bytes)"
            ),
            regs: emulator.regs,
        });
    }

    // Set entry SP to leave room for stack arguments above it
    // Stack arguments are at positive offsets from SP (above SP)
    let entry_sp = (ram_end - total_stack_space) as i32;
    emulator.regs[2] = entry_sp; // SP register

    let return_area_addr = if needs_ret_area && return_area_size > 0 {
        // Return area starts at entry_sp (positive offsets from SP)
        Some(entry_sp as u32)
    } else {
        None
    };

    Ok((entry_sp, return_area_addr, return_area_size))
}

/// Place return area pointer in a0 register.
fn place_return_area_pointer(
    emulator: &mut Riscv32Emulator,
    return_area_addr: u32,
) -> Result<(), EmulatorError> {
    emulator.regs[10] = return_area_addr as i32; // a0 = return area pointer
    Ok(())
}

/// Place struct return buffer pointer in a0 register.
fn place_struct_return_pointer(
    emulator: &mut Riscv32Emulator,
    buffer_addr: u32,
) -> Result<(), EmulatorError> {
    emulator.regs[10] = buffer_addr as i32; // a0 = struct return buffer pointer
    Ok(())
}

/// Allocate buffer for struct return in emulator RAM.
fn allocate_struct_return_buffer(
    emulator: &mut Riscv32Emulator,
    size: usize,
) -> Result<u32, EmulatorError> {
    let ram = emulator.memory.ram();
    let ram_size = ram.len();
    let ram_start = DEFAULT_RAM_START;

    // Find a suitable location in RAM
    // Ensure 4-byte alignment
    let aligned_size = ((size + 3) / 4) * 4;

    // Allocate at ram_start + some offset to avoid stack overlap
    // Stack is typically at the top, so we allocate lower
    let buffer_addr = ram_start + 1024; // Start after first 1KB

    // Ensure we have enough space
    if buffer_addr + aligned_size as u32 > ram_start + ram_size as u32 {
        return Err(EmulatorError::InvalidInstruction {
            pc: emulator.pc,
            instruction: 0,
            reason: format!("Not enough RAM for struct return buffer (need {aligned_size} bytes)"),
            regs: emulator.regs,
        });
    }

    // Zero out the buffer
    let ram_mut = emulator.memory.ram_mut();
    let offset = (buffer_addr - ram_start) as usize;
    if offset + aligned_size <= ram_mut.len() {
        for i in offset..offset + aligned_size {
            ram_mut[i] = 0;
        }
    }

    Ok(buffer_addr)
}

/// Place a single argument value in a register.
fn place_register_argument(
    emulator: &mut Riscv32Emulator,
    reg_enc: u8,
    value: &DataValue,
    slot_idx: usize,
) -> Result<(), EmulatorError> {
    if reg_enc as usize >= 32 {
        return Err(EmulatorError::InvalidInstruction {
            pc: emulator.pc,
            instruction: 0,
            reason: format!("Invalid register index: {reg_enc}"),
            regs: emulator.regs,
        });
    }

    let reg_idx = reg_enc as usize;

    match value {
        DataValue::I8(v) => {
            if slot_idx == 0 {
                emulator.regs[reg_idx] = *v as i32;
            } else {
                return Err(EmulatorError::InvalidInstruction {
                    pc: emulator.pc,
                    instruction: 0,
                    reason: format!("i8 only has 1 slot, got slot_idx {slot_idx}"),
                    regs: emulator.regs,
                });
            }
        }
        DataValue::I16(v) => {
            if slot_idx == 0 {
                emulator.regs[reg_idx] = *v as i32;
            } else {
                return Err(EmulatorError::InvalidInstruction {
                    pc: emulator.pc,
                    instruction: 0,
                    reason: format!("i16 only has 1 slot, got slot_idx {slot_idx}"),
                    regs: emulator.regs,
                });
            }
        }
        DataValue::I32(v) => {
            if slot_idx == 0 {
                emulator.regs[reg_idx] = *v;
            } else {
                return Err(EmulatorError::InvalidInstruction {
                    pc: emulator.pc,
                    instruction: 0,
                    reason: format!("i32 only has 1 slot, got slot_idx {slot_idx}"),
                    regs: emulator.regs,
                });
            }
        }
        DataValue::I64(v) => {
            let v_u64 = *v as u64;
            match slot_idx {
                0 => {
                    // Low 32 bits
                    emulator.regs[reg_idx] = v_u64 as u32 as i32;
                }
                1 => {
                    // High 32 bits
                    emulator.regs[reg_idx] = (v_u64 >> 32) as u32 as i32;
                }
                _ => {
                    return Err(EmulatorError::InvalidInstruction {
                        pc: emulator.pc,
                        instruction: 0,
                        reason: format!("i64 only has 2 slots, got slot_idx {slot_idx}"),
                        regs: emulator.regs,
                    });
                }
            }
        }
        DataValue::I128(v) => {
            let v_u128 = *v as u128;
            match slot_idx {
                0 => emulator.regs[reg_idx] = v_u128 as u32 as i32,
                1 => emulator.regs[reg_idx] = (v_u128 >> 32) as u32 as i32,
                2 => emulator.regs[reg_idx] = (v_u128 >> 64) as u32 as i32,
                3 => emulator.regs[reg_idx] = (v_u128 >> 96) as u32 as i32,
                _ => {
                    return Err(EmulatorError::InvalidInstruction {
                        pc: emulator.pc,
                        instruction: 0,
                        reason: format!("i128 only has 4 slots, got slot_idx {slot_idx}"),
                        regs: emulator.regs,
                    });
                }
            }
        }
        _ => {
            return Err(EmulatorError::InvalidInstruction {
                pc: emulator.pc,
                instruction: 0,
                reason: format!("Unsupported DataValue type for register argument"),
                regs: emulator.regs,
            });
        }
    }

    Ok(())
}

/// Place a single argument value on the stack.
fn place_stack_argument(
    emulator: &mut Riscv32Emulator,
    offset: i64,
    value: &DataValue,
    slot_idx: usize,
    entry_sp: i32,
) -> Result<(), EmulatorError> {
    let stack_addr = (entry_sp as u64).wrapping_add(offset as u64) as u32;

    match value {
        DataValue::I8(v) => {
            if slot_idx == 0 {
                emulator
                    .memory
                    .write_word(stack_addr, *v as i32)
                    .map_err(|e| EmulatorError::InvalidInstruction {
                        pc: emulator.pc,
                        instruction: 0,
                        reason: format!(
                            "Failed to write i8 stack argument at 0x{stack_addr:08x}: {e}"
                        ),
                        regs: emulator.regs,
                    })?;
            } else {
                return Err(EmulatorError::InvalidInstruction {
                    pc: emulator.pc,
                    instruction: 0,
                    reason: format!("i8 only has 1 slot, got slot_idx {slot_idx}"),
                    regs: emulator.regs,
                });
            }
        }
        DataValue::I16(v) => {
            if slot_idx == 0 {
                emulator
                    .memory
                    .write_word(stack_addr, *v as i32)
                    .map_err(|e| EmulatorError::InvalidInstruction {
                        pc: emulator.pc,
                        instruction: 0,
                        reason: format!(
                            "Failed to write i16 stack argument at 0x{stack_addr:08x}: {e}"
                        ),
                        regs: emulator.regs,
                    })?;
            } else {
                return Err(EmulatorError::InvalidInstruction {
                    pc: emulator.pc,
                    instruction: 0,
                    reason: format!("i16 only has 1 slot, got slot_idx {slot_idx}"),
                    regs: emulator.regs,
                });
            }
        }
        DataValue::I32(v) => {
            if slot_idx == 0 {
                emulator.memory.write_word(stack_addr, *v).map_err(|e| {
                    EmulatorError::InvalidInstruction {
                        pc: emulator.pc,
                        instruction: 0,
                        reason: format!(
                            "Failed to write i32 stack argument at 0x{stack_addr:08x}: {e}"
                        ),
                        regs: emulator.regs,
                    }
                })?;
            } else {
                return Err(EmulatorError::InvalidInstruction {
                    pc: emulator.pc,
                    instruction: 0,
                    reason: format!("i32 only has 1 slot, got slot_idx {slot_idx}"),
                    regs: emulator.regs,
                });
            }
        }
        DataValue::I64(v) => {
            let v_u64 = *v as u64;
            let word_value = match slot_idx {
                0 => {
                    // Low 32 bits
                    v_u64 as u32 as i32
                }
                1 => {
                    // High 32 bits
                    (v_u64 >> 32) as u32 as i32
                }
                _ => {
                    return Err(EmulatorError::InvalidInstruction {
                        pc: emulator.pc,
                        instruction: 0,
                        reason: format!("i64 only has 2 slots, got slot_idx {slot_idx}"),
                        regs: emulator.regs,
                    });
                }
            };
            // ABI provides correct offset for each slot
            emulator
                .memory
                .write_word(stack_addr, word_value)
                .map_err(|e| EmulatorError::InvalidInstruction {
                    pc: emulator.pc,
                    instruction: 0,
                    reason: format!(
                        "Failed to write i64 slot {slot_idx} stack argument at 0x{stack_addr:08x}: {e}"
                    ),
                    regs: emulator.regs,
                })?;
        }
        DataValue::I128(v) => {
            let v_u128 = *v as u128;
            let word_value = match slot_idx {
                0 => v_u128 as u32 as i32,
                1 => (v_u128 >> 32) as u32 as i32,
                2 => (v_u128 >> 64) as u32 as i32,
                3 => (v_u128 >> 96) as u32 as i32,
                _ => {
                    return Err(EmulatorError::InvalidInstruction {
                        pc: emulator.pc,
                        instruction: 0,
                        reason: format!("i128 only has 4 slots, got slot_idx {slot_idx}"),
                        regs: emulator.regs,
                    });
                }
            };
            // ABI provides correct offset for each slot
            emulator
                .memory
                .write_word(stack_addr, word_value)
                .map_err(|e| EmulatorError::InvalidInstruction {
                    pc: emulator.pc,
                    instruction: 0,
                    reason: format!(
                        "Failed to write i128 slot {slot_idx} stack argument at 0x{stack_addr:08x}: {e}"
                    ),
                    regs: emulator.regs,
                })?;
        }
        _ => {
            return Err(EmulatorError::InvalidInstruction {
                pc: emulator.pc,
                instruction: 0,
                reason: format!("Unsupported DataValue type for stack argument"),
                regs: emulator.regs,
            });
        }
    }

    Ok(())
}

/// Orchestrate placing all function arguments.
fn place_arguments(
    emulator: &mut Riscv32Emulator,
    args: &[DataValue],
    arg_locations: &[ArgLocation],
    entry_sp: i32,
) -> Result<(), EmulatorError> {
    if args.len() != arg_locations.len() {
        return Err(EmulatorError::InvalidInstruction {
            pc: emulator.pc,
            instruction: 0,
            reason: format!(
                "Argument count mismatch: {} args provided, {} locations computed",
                args.len(),
                arg_locations.len()
            ),
            regs: emulator.regs,
        });
    }

    for (_arg_idx, (arg, arg_location)) in args.iter().zip(arg_locations.iter()).enumerate() {
        for (slot_idx, slot) in arg_location.slots.iter().enumerate() {
            match slot {
                ArgSlot::Reg(reg_enc, _ty) => {
                    place_register_argument(emulator, *reg_enc, arg, slot_idx)?;
                }
                ArgSlot::Stack(offset, _ty) => {
                    // ABI provides correct offset for each slot
                    place_stack_argument(emulator, *offset, arg, slot_idx, entry_sp)?;
                }
            }
        }
    }

    Ok(())
}

/// Extract a return value slot from a register.
fn extract_register_return_value(
    emulator: &Riscv32Emulator,
    reg_enc: u8,
) -> Result<u32, EmulatorError> {
    if reg_enc as usize >= 32 {
        return Err(EmulatorError::InvalidInstruction {
            pc: emulator.pc,
            instruction: 0,
            reason: format!("Invalid register index: {reg_enc}"),
            regs: emulator.regs,
        });
    }

    Ok(emulator.regs[reg_enc as usize] as u32)
}

/// Extract a return value slot from the stack.
fn extract_stack_return_value(
    emulator: &Riscv32Emulator,
    offset: i64,
    entry_sp: i32,
) -> Result<u32, EmulatorError> {
    let stack_addr = (entry_sp as u64).wrapping_add(offset as u64) as u32;
    let word_value =
        emulator
            .memory
            .read_word(stack_addr)
            .map_err(|e| EmulatorError::InvalidInstruction {
                pc: emulator.pc,
                instruction: 0,
                reason: format!("Failed to read stack return value at 0x{stack_addr:08x}: {e}"),
                regs: emulator.regs,
            })?;
    Ok(word_value as u32)
}

/// Extract a complete return value (all slots).
fn extract_single_return_value(
    emulator: &Riscv32Emulator,
    retval_location: &ReturnValueLocation,
    entry_sp: i32,
) -> Result<DataValue, EmulatorError> {
    match retval_location.ty {
        types::I8 | types::I16 | types::I32 => {
            // Single-slot return values
            if retval_location.slots.len() != 1 {
                return Err(EmulatorError::InvalidInstruction {
                    pc: emulator.pc,
                    instruction: 0,
                    reason: format!(
                        "Expected 1 slot for return value type {:?}, got {}",
                        retval_location.ty,
                        retval_location.slots.len()
                    ),
                    regs: emulator.regs,
                });
            }

            let slot = &retval_location.slots[0];
            let value = match slot {
                ReturnLocation::Reg(reg_enc, _) => {
                    extract_register_return_value(emulator, *reg_enc)?
                }
                ReturnLocation::Stack(offset, _) => {
                    extract_stack_return_value(emulator, *offset, entry_sp)?
                }
            };

            match retval_location.ty {
                types::I8 => Ok(DataValue::I8(value as i8)),
                types::I16 => Ok(DataValue::I16(value as i16)),
                types::I32 => Ok(DataValue::I32(value as i32)),
                _ => unreachable!(),
            }
        }
        types::I64 => {
            // i64 uses 2 slots
            if retval_location.slots.len() != 2 {
                return Err(EmulatorError::InvalidInstruction {
                    pc: emulator.pc,
                    instruction: 0,
                    reason: format!(
                        "Expected 2 slots for i64 return value, got {}",
                        retval_location.slots.len()
                    ),
                    regs: emulator.regs,
                });
            }

            let mut low = 0u64;
            let mut high = 0u64;

            for (slot_idx, slot) in retval_location.slots.iter().enumerate() {
                let value = match slot {
                    ReturnLocation::Reg(reg_enc, _) => {
                        extract_register_return_value(emulator, *reg_enc)?
                    }
                    ReturnLocation::Stack(offset, _) => {
                        extract_stack_return_value(emulator, *offset, entry_sp)?
                    }
                };

                if slot_idx == 0 {
                    low = value as u64;
                } else {
                    high = value as u64;
                }
            }

            Ok(DataValue::I64(((high << 32) | low) as i64))
        }
        types::I128 => {
            // i128 uses 4 slots
            if retval_location.slots.len() != 4 {
                return Err(EmulatorError::InvalidInstruction {
                    pc: emulator.pc,
                    instruction: 0,
                    reason: format!(
                        "Expected 4 slots for i128 return value, got {}",
                        retval_location.slots.len()
                    ),
                    regs: emulator.regs,
                });
            }

            let mut reg_values = [0u32; 4];
            for (j, slot) in retval_location.slots.iter().enumerate() {
                reg_values[j] = match slot {
                    ReturnLocation::Reg(reg_enc, _) => {
                        extract_register_return_value(emulator, *reg_enc)?
                    }
                    ReturnLocation::Stack(offset, _) => {
                        extract_stack_return_value(emulator, *offset, entry_sp)?
                    }
                };
            }

            let reg0 = reg_values[0] as u128;
            let reg1 = reg_values[1] as u128;
            let reg2 = reg_values[2] as u128;
            let reg3 = reg_values[3] as u128;
            Ok(DataValue::I128(
                ((reg3 << 96) | (reg2 << 64) | (reg1 << 32) | reg0) as i128,
            ))
        }
        _ => Err(EmulatorError::InvalidInstruction {
            pc: emulator.pc,
            instruction: 0,
            reason: format!("Unsupported return type: {:?}", retval_location.ty),
            regs: emulator.regs,
        }),
    }
}

/// Extract all return values.
fn extract_return_values(
    emulator: &Riscv32Emulator,
    return_locations: &[ReturnValueLocation],
    entry_sp: i32,
) -> Result<Vec<DataValue>, EmulatorError> {
    let mut results = Vec::new();
    for retval_location in return_locations {
        let result_value = extract_single_return_value(emulator, retval_location, entry_sp)?;
        results.push(result_value);
    }
    Ok(results)
}

/// Extract struct return value from buffer.
fn extract_struct_return_value(
    emulator: &Riscv32Emulator,
    buffer_addr: u32,
    struct_size: usize,
) -> Result<Vec<DataValue>, EmulatorError> {
    // Read struct as words (4 bytes each)
    let num_words = (struct_size + 3) / 4; // Round up
    let mut results = Vec::with_capacity(num_words);

    for i in 0..num_words {
        let addr = buffer_addr + (i * 4) as u32;
        let word_value =
            emulator
                .memory
                .read_word(addr)
                .map_err(|e| EmulatorError::InvalidInstruction {
                    pc: emulator.pc,
                    instruction: 0,
                    reason: format!(
                        "Failed to read struct return value word {i} at 0x{addr:08x}: {e}"
                    ),
                    regs: emulator.regs,
                })?;
        results.push(DataValue::I32(word_value));
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use cranelift_codegen::ir::{AbiParam, types};
    use cranelift_codegen::isa::CallConv;

    fn create_test_emulator() -> Riscv32Emulator {
        let mut code = Vec::with_capacity(1024);
        code.resize(1024, 0);
        let mut ram = Vec::with_capacity(1024 * 1024);
        ram.resize(1024 * 1024, 0); // 1MB RAM
        Riscv32Emulator::new(code, ram)
    }

    fn create_flags() -> Flags {
        use cranelift_codegen::settings::Configurable;
        let mut builder = settings::builder();
        builder
            .set("enable_multi_ret_implicit_sret", "true")
            .unwrap();
        Flags::new(builder)
    }

    #[test]
    fn test_has_struct_return() {
        let mut sig = Signature::new(CallConv::SystemV);
        assert!(!has_struct_return(&sig));

        sig.params
            .push(AbiParam::special(types::I32, ArgumentPurpose::StructReturn));
        assert!(has_struct_return(&sig));
    }

    #[test]
    fn test_needs_return_area() {
        let flags = create_flags();
        let mut sig = Signature::new(CallConv::SystemV);
        sig.returns.push(AbiParam::new(types::I32));

        let return_locations = abi_helper::compute_return_locations(&sig, &flags).unwrap();
        assert!(!needs_return_area(&return_locations)); // Single return in register

        sig.returns.push(AbiParam::new(types::I32));
        sig.returns.push(AbiParam::new(types::I32));
        let return_locations = abi_helper::compute_return_locations(&sig, &flags).unwrap();
        assert!(needs_return_area(&return_locations)); // Third return on stack
    }

    #[test]
    fn test_compute_return_area_size() {
        let flags = create_flags();
        let mut sig = Signature::new(CallConv::SystemV);
        sig.returns.push(AbiParam::new(types::I32));
        sig.returns.push(AbiParam::new(types::I32));
        sig.returns.push(AbiParam::new(types::I32));

        let return_locations = abi_helper::compute_return_locations(&sig, &flags).unwrap();
        let size = compute_return_area_size(&return_locations);
        assert!(size > 0); // Should have some stack space
    }

    #[test]
    fn test_place_return_area_pointer() {
        let mut emulator = create_test_emulator();
        place_return_area_pointer(&mut emulator, 0x80001000).unwrap();
        assert_eq!(emulator.regs[10], 0x80001000u32 as i32);
    }

    #[test]
    fn test_place_struct_return_pointer() {
        let mut emulator = create_test_emulator();
        place_struct_return_pointer(&mut emulator, 0x80002000).unwrap();
        assert_eq!(emulator.regs[10], 0x80002000u32 as i32);
    }

    #[test]
    fn test_place_register_argument_i32() {
        let mut emulator = create_test_emulator();
        place_register_argument(&mut emulator, 10, &DataValue::I32(42), 0).unwrap();
        assert_eq!(emulator.regs[10], 42);
    }

    #[test]
    fn test_place_register_argument_i64() {
        let mut emulator = create_test_emulator();
        let value = DataValue::I64(0x1234567890ABCDEF);
        place_register_argument(&mut emulator, 10, &value, 0).unwrap();
        place_register_argument(&mut emulator, 11, &value, 1).unwrap();
        assert_eq!(emulator.regs[10] as u32, 0x90ABCDEF);
        assert_eq!(emulator.regs[11] as u32, 0x12345678);
    }

    #[test]
    fn test_place_stack_argument_i32() {
        let mut emulator = create_test_emulator();
        let entry_sp = 0x8000F000u32 as i32;
        emulator.regs[2] = entry_sp;
        place_stack_argument(&mut emulator, 0, &DataValue::I32(42), 0, entry_sp).unwrap();
        let value = emulator.memory.read_word(entry_sp as u32).unwrap();
        assert_eq!(value, 42);
    }

    #[test]
    fn test_extract_register_return_value() {
        let mut emulator = create_test_emulator();
        emulator.regs[10] = 42;
        let value = extract_register_return_value(&emulator, 10).unwrap();
        assert_eq!(value, 42);
    }

    #[test]
    fn test_extract_stack_return_value() {
        let mut emulator = create_test_emulator();
        let entry_sp = 0x8000F000u32 as i32;
        emulator.regs[2] = entry_sp;
        emulator.memory.write_word(entry_sp as u32, 42).unwrap();
        let value = extract_stack_return_value(&emulator, 0, entry_sp).unwrap();
        assert_eq!(value, 42);
    }

    #[test]
    fn test_extract_single_return_value_i32() {
        let mut emulator = create_test_emulator();
        emulator.regs[10] = 42;
        let retval_location = ReturnValueLocation {
            slots: vec![ReturnLocation::Reg(10, types::I32)],
            ty: types::I32,
        };
        let value = extract_single_return_value(&emulator, &retval_location, 0).unwrap();
        assert_eq!(value, DataValue::I32(42));
    }

    #[test]
    fn test_extract_single_return_value_i64() {
        let mut emulator = create_test_emulator();
        emulator.regs[10] = 0x90ABCDEFu32 as i32;
        emulator.regs[11] = 0x12345678i32;
        let retval_location = ReturnValueLocation {
            slots: vec![
                ReturnLocation::Reg(10, types::I32),
                ReturnLocation::Reg(11, types::I32),
            ],
            ty: types::I64,
        };
        let value = extract_single_return_value(&emulator, &retval_location, 0).unwrap();
        assert_eq!(value, DataValue::I64(0x1234567890ABCDEF));
    }

    #[test]
    fn test_allocate_struct_return_buffer() {
        let mut emulator = create_test_emulator();
        let buffer_addr = allocate_struct_return_buffer(&mut emulator, 16).unwrap();
        assert!(buffer_addr >= DEFAULT_RAM_START);
        // Verify buffer is zeroed
        for i in 0..4 {
            let value = emulator.memory.read_word(buffer_addr + i * 4).unwrap();
            assert_eq!(value, 0);
        }
    }
}
