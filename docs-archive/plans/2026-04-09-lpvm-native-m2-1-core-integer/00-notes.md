# Plan: lpvm-native M2.1 Core Integer Operations

## Scope of Work

Implement lowering for core integer operations required by rainbow and general shader compilation:

1. **Integer division and remainder**
   - `IdivS` (signed division)
   - `IdivU` (unsigned division)
   - `IremS` (signed remainder)
   - `IremU` (unsigned remainder)

2. **Integer comparisons**
   - Signed: `Ieq`, `Ine`, `IltS`, `IleS`, `IgtS`, `IgeS`
   - Unsigned: `IltU`, `IleU`, `IgtU`, `IgeU`

3. **Selection**
   - `Select` (conditional: `cond ? if_true : if_false`)

## Current State

**Already implemented in `lower.rs`:**
- `Iadd`, `Isub`, `Imul` → VInst arithmetic
- `Copy` → Mov32
- `IconstI32` → IConst32
- `Return` → Ret
- `Fadd`, `Fsub`, `Fmul` (Q32) → Call to builtins
- `Load`, `Store` → Load32, Store32

**Current VInst set in `vinst.rs`:**
- Add32, Sub32, Mul32
- Mov32, IConst32
- Load32, Store32
- Call, Ret, Label

**Missing for M2.1:**
- Div32, Rem32 (for IdivS/IdivU/IremS/IremU)
- Icmp variant or separate compare VInsts (for Ieq/Ine/Ilt/etc)
- Select VInst

## Questions

### Q1: Should we use separate VInsts for signed vs unsigned div/rem?

**Context:** LPIR has distinct IdivS/IdivU and IremS/IremU ops. RV32 has `div`/`divu` and `rem`/`remu` instructions.

**Options:**
- A) One `Div32` VInst with a `signed: bool` flag
- B) Separate `DivS32` and `DivU32` VInsts (same for Rem)
- C) Lower directly to RV32 without separate VInst (handle in emission)

**Decision:** B - Separate VInsts. This mirrors the LPIR structure and makes emission straightforward. It's more explicit and matches RV32's separate instructions.

### Q2: How to handle comparisons?

**Context:** LPIR has 10 comparison ops (eq, ne, ltS, leS, gtS, geS, ltU, leU, gtU, geU). These need to lower to RV32 compare sequences.

**Options:**
- A) One `Icmp32` VInst with a condition code enum
- B) Separate VInsts for each comparison (Ieq32, IltS32, etc.)
- C) Lower to arithmetic sequence (slt + masking)

**Decision:** A - Single `Icmp32` with condition code enum (IcmpCond). This is cleaner, reduces VInst variants, and matches how Cranelift/QBE handle it. Emission maps each condition to appropriate RV32 sequences.

### Q3: Select implementation strategy?

**Context:** `Select` needs to pick between two values based on a condition. RV32 doesn't have a native CMOV.

**Options:**
- A) Branchless sequence: `cond & true | ~cond & false` using arithmetic
- B) Branching: Emit actual if/else with labels (requires M2.2 control flow)
- C) Use a temporary VInst that gets expanded during emission

**Decision:** A - Branchless arithmetic sequence. Lower Select to a sequence of arithmetic VInsts (sub, and, add) that compute the result without control flow. This avoids M2.2 dependency and is faster (no branch misprediction).

Lowered sequence:
```
tmp1 = sub(if_true, if_false)      # tmp1 = true - false
tmp2 = and(tmp1, cond)              # tmp2 = (true - false) & cond (cond is 0 or 1)
result = add(tmp2, if_false)        # result = false + ((true - false) & cond)
```

### Q4: Division by zero handling?

**Context:** RV32 div/rem by zero returns -1 (for signed) or max unsigned, but LPIR/GLSL may have different semantics.

**Options:**
- A) Use RV32 behavior directly (hardware-defined)
- B) Add checks before division (expensive)
- C) Document that we follow RV32 hardware semantics

**Decision:** A - Use RV32 hardware behavior directly. Division by zero is undefined behavior in GLSL, so hardware-defined results are acceptable. No extra checks needed.

## Open Questions

None - all questions answered.

None after the above are answered.

## Notes

- RV32M extension provides `div`, `divu`, `rem`, `remu` instructions
- We need to check if target has M extension or trap gracefully
- For now, assume M extension is available (ESP32-C6 has it)
- Comparisons emit as: slt/sltu for lt, then combine for le/eq/etc
