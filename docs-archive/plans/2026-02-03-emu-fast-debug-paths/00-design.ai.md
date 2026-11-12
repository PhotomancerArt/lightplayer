# Emulator Fast/Debug Paths Optimization - Design

## Scope of Work

Refactor the emulator core to have:
1. **Fast path**: Combined decode/execute with zero logging overhead
2. **Debug path**: Full logging support with runtime control
3. **Monomorphic approach**: Use generics with const trait to eliminate logging overhead at compile time
4. **Code organization**: Split instructions into category files, one instruction per inline function
5. **Decode-execute fusion**: Combine decode and execute steps for better performance
6. **Simplified log levels**: Remove `LogLevel::Verbose`, keep only `None`, `Errors`, `Instructions`

**Expected Impact**: 
- 15-25% improvement from removing logging overhead
- 10-15% improvement from decode-execute fusion
- Better code maintainability through organization

## File Structure

```
lp-riscv/lp-riscv-emu/src/emu/
├── executor/                    # NEW: Category-based executor
│   ├── mod.rs                   # Main dispatch: decode_execute<M>()
│   ├── arithmetic.rs            # R-type arithmetic (ADD, SUB, MUL, etc.)
│   ├── immediate.rs             # I-type immediate (ADDI, SLLI, etc.)
│   ├── load_store.rs            # Load/store (LW, SW, LB, etc.)
│   ├── branch.rs                # Branch (BEQ, BNE, etc.)
│   ├── jump.rs                  # Jump (JAL, JALR)
│   ├── system.rs                # System (ECALL, EBREAK, CSR)
│   ├── compressed.rs            # Compressed instructions
│   └── bitmanip.rs              # Bit manipulation (Zbb, Zbs, Zba)
├── executor.rs                  # DEPRECATED: Keep for backward compat during migration
├── decoder.rs                   # No changes (re-exports lp_riscv_inst)
├── emulator/
│   ├── execution.rs             # UPDATE: step_inner() calls decode_execute<M>()
│   ├── run_loops.rs             # UPDATE: run_inner() dispatches based on log_level
│   └── state.rs                 # No changes
└── logging.rs                    # UPDATE: Remove LogLevel::Verbose
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Riscv32Emulator                                            │
│  ─────────────────────────────────────────────────────────  │
│  log_level: LogLevel                                        │
└─────────────────────────────────────────────────────────────┘
                    │
                    │ calls (once per run)
                    ▼
┌─────────────────────────────────────────────────────────────┐
│  run_inner(fuel)                                            │
│  ─────────────────────────────────────────────────────────  │
│  match log_level {                                          │
│    LogLevel::None => run_inner_fast(fuel),                 │
│    _ => run_inner_logging(fuel),                           │
│  }                                                           │
└─────────────────────────────────────────────────────────────┘
        │                              │
        │                              │
        ▼                              ▼
┌──────────────────┐          ┌──────────────────────┐
│ run_inner_fast() │          │ run_inner_logging()  │
│ ──────────────── │          │ ──────────────────── │
│ Loop:            │          │ Loop:                │
│   inst_word =    │          │   inst_word =         │
│     fetch()      │          │     fetch()          │
│   decode_execute │          │   decode_execute     │
│     ::<Logging   │          │     ::<Logging       │
│     Disabled>()  │          │     Enabled>()       │
└──────────────────┘          └──────────────────────┘
        │                              │
        │                              │
        ▼                              ▼
┌─────────────────────────────────────────────────────────────┐
│  decode_execute<M: LoggingMode>(inst_word, pc, regs, mem) │
│  ─────────────────────────────────────────────────────────  │
│  if compressed:                                             │
│    decode_execute_compressed<M>()                           │
│  else:                                                      │
│    match opcode {                                           │
│      0x33 => decode_execute_rtype<M>()  (arithmetic)       │
│      0x13 => decode_execute_itype<M>()  (immediate)        │
│      0x03 => decode_execute_load<M>()   (load)             │
│      0x23 => decode_execute_store<M>()  (store)            │
│      ...                                                    │
│    }                                                         │
└─────────────────────────────────────────────────────────────┘
        │
        │ (example: rtype)
        ▼
┌─────────────────────────────────────────────────────────────┐
│  decode_execute_rtype<M>(inst_word, pc, regs, mem)        │
│  ─────────────────────────────────────────────────────────  │
│  Extract: rd, rs1, rs2, funct3, funct7                    │
│  match (funct3, funct7) {                                 │
│    (0x0, 0x0) => execute_add<M>(rd, rs1, rs2, pc, ...)    │
│    (0x0, 0x20) => execute_sub<M>(rd, rs1, rs2, pc, ...)     │
│    ...                                                       │
│  }                                                           │
└─────────────────────────────────────────────────────────────┘
        │
        │ (example: ADD)
        ▼
┌─────────────────────────────────────────────────────────────┐
│  execute_add<M: LoggingMode>(rd, rs1, rs2, pc, ...)     │
│  ─────────────────────────────────────────────────────────  │
│  val1 = read_reg(regs, rs1)                                 │
│  val2 = read_reg(regs, rs2)                                │
│  rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 }  │
│  result = val1 + val2                                       │
│  regs[rd] = result                                          │
│  log = if M::ENABLED {                                      │
│    Some(InstLog::Arithmetic { ... })                        │
│  } else {                                                   │
│    None                                                      │
│  }                                                           │
│  Ok(ExecutionResult { log, ... })                            │
└─────────────────────────────────────────────────────────────┘
```

## Main Components

### 1. LoggingMode Trait

```rust
pub trait LoggingMode {
    const ENABLED: bool;
}

pub struct LoggingEnabled;
pub struct LoggingDisabled;

impl LoggingMode for LoggingEnabled {
    const ENABLED: bool = true;
}

impl LoggingMode for LoggingDisabled {
    const ENABLED: bool = false;
}
```

### 2. Top-Level Dispatch (run_loops.rs)

```rust
impl Riscv32Emulator {
    pub(super) fn run_inner(&mut self, mut fuel: u64) -> Result<StepResult, EmulatorError> {
        match self.log_level {
            LogLevel::None => self.run_inner_fast(fuel),
            _ => self.run_inner_logging(fuel),
        }
    }
    
    fn run_inner_fast(&mut self, mut fuel: u64) -> Result<StepResult, EmulatorError> {
        loop {
            fuel -= 1;
            if fuel == 0 {
                return Ok(StepResult::FuelExhausted(...));
            }
            
            let inst_word = self.memory.fetch_instruction(self.pc)?;
            self.instruction_count += 1;
            
            let exec_result = executor::decode_execute::<LoggingDisabled>(
                inst_word,
                self.pc,
                &mut self.regs,
                &mut self.memory,
            )?;
            
            // Update PC, handle results (no logging)
            // ...
        }
    }
    
    fn run_inner_logging(&mut self, mut fuel: u64) -> Result<StepResult, EmulatorError> {
        loop {
            fuel -= 1;
            if fuel == 0 {
                return Ok(StepResult::FuelExhausted(...));
            }
            
            let inst_word = self.memory.fetch_instruction(self.pc)?;
            self.instruction_count += 1;
            
            let exec_result = executor::decode_execute::<LoggingEnabled>(
                inst_word,
                self.pc,
                &mut self.regs,
                &mut self.memory,
            )?;
            
            // Update PC, handle logging, handle results
            if let Some(log) = exec_result.log {
                let log_with_cycle = log.set_cycle(self.instruction_count);
                self.log_instruction(log_with_cycle);
            }
            // ...
        }
    }
}
```

### 3. Main Dispatch (executor/mod.rs)

```rust
pub fn decode_execute<M: LoggingMode>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError>
where
    [(); M::ENABLED as usize]:,
{
    // Check if compressed instruction
    if (inst_word & 0x3) != 0x3 {
        return compressed::decode_execute_compressed::<M>(inst_word, pc, regs, memory);
    }
    
    let opcode = (inst_word & 0x7f) as u8;
    
    match opcode {
        0x33 => arithmetic::decode_execute_rtype::<M>(inst_word, pc, regs, memory),
        0x13 => immediate::decode_execute_itype::<M>(inst_word, pc, regs, memory),
        0x03 => load_store::decode_execute_load::<M>(inst_word, pc, regs, memory),
        0x23 => load_store::decode_execute_store::<M>(inst_word, pc, regs, memory),
        0x63 => branch::decode_execute_branch::<M>(inst_word, pc, regs, memory),
        0x6f => jump::decode_execute_jal::<M>(inst_word, pc, regs, memory),
        0x67 => jump::decode_execute_jalr::<M>(inst_word, pc, regs, memory),
        0x37 => immediate::decode_execute_lui::<M>(inst_word, pc, regs, memory),
        0x17 => immediate::decode_execute_auipc::<M>(inst_word, pc, regs, memory),
        0x73 => system::decode_execute_system::<M>(inst_word, pc, regs, memory),
        // ... other opcodes
        _ => Err(EmulatorError::InvalidInstruction { ... }),
    }
}
```

### 4. Category Functions (example: arithmetic.rs)

```rust
pub(super) fn decode_execute_rtype<M: LoggingMode>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError>
where
    [(); M::ENABLED as usize]:,
{
    let r = TypeR::from_riscv(inst_word);
    let rd = Gpr::new(r.rd);
    let rs1 = Gpr::new(r.rs1);
    let rs2 = Gpr::new(r.rs2);
    let funct3 = (r.func & 0x7) as u8;
    let funct7 = ((r.func >> 3) & 0x7f) as u8;
    
    match (funct3, funct7) {
        (0x0, 0x0) => execute_add::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x0, 0x20) => execute_sub::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x0, 0x01) => execute_mul::<M>(rd, rs1, rs2, inst_word, pc, regs),
        // ... other instructions
        _ => Err(EmulatorError::InvalidInstruction { ... }),
    }
}

#[inline(always)]
fn execute_add<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError>
where
    [(); M::ENABLED as usize]:,
{
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED {
        read_reg(regs, rd)
    } else {
        0  // Dummy value, never used
    };
    let result = val1.wrapping_add(val2);
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
        Some(InstLog::Arithmetic {
            cycle: 0,  // Will be set by emu
            pc,
            instruction: instruction_word,
            rd,
            rs1_val: val1,
            rs2_val: Some(val2),
            rd_old,
            rd_new: result,
        })
    } else {
        None
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}
```

## Key Design Decisions

1. **Single dispatch point**: `log_level` checked once in `run_inner()`, not per instruction
2. **Monomorphic generics**: Single generic function per category, compiler creates two versions
3. **No log_level parameter**: Instruction functions don't take `log_level` - logging is binary at instruction level
4. **Decode-execute fusion**: Decode and execute combined in single functions, eliminating `Inst` enum in hot path
5. **Category organization**: Instructions grouped by category for maintainability
6. **One instruction per function**: Each instruction has its own `#[inline]` function for clarity

## Benefits

- **Zero overhead in fast path**: Compiler eliminates all logging code when `M::ENABLED` is false
- **Runtime control**: Can switch between fast and logging paths at runtime
- **Better organization**: Category files make code easier to navigate and maintain
- **Performance**: Decode-execute fusion eliminates intermediate `Inst` enum allocation
- **Maintainability**: One instruction per function makes code easier to understand and modify

## Migration Strategy

1. Keep old `executor.rs` for backward compatibility during migration
2. Implement new executor structure alongside old code
3. Update `step_inner()` to use new `decode_execute<M>()` function
4. Gradually migrate tests and call sites
5. Remove old `executor.rs` once migration complete
