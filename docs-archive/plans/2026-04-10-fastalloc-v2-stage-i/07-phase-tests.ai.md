# Phase 7: Unit Tests

## Scope

Add unit tests for PhysInst parser, allocator, and emitter.

## Implementation

### 1. Tests for PhysInst parser/formatter

Add to `rv32fa/debug/physinst.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_format_add() {
        let inst = PhysInst::Add { dst: 10, src1: 11, src2: 12 }; // a0, a1, a2
        assert_eq!(format(&inst), "add a0, a1, a2");
    }

    #[test]
    fn test_format_li() {
        let inst = PhysInst::Li { dst: 10, imm: 42 };
        assert_eq!(format(&inst), "li a0, 42");
    }

    #[test]
    fn test_format_lw() {
        let inst = PhysInst::Lw { dst: 10, base: 2, offset: 4 };
        assert_eq!(format(&inst), "lw a0, 4(sp)");
    }

    #[test]
    fn test_format_sw() {
        let inst = PhysInst::Sw { src: 10, base: 2, offset: 8 };
        assert_eq!(format(&inst), "sw a0, 8(sp)");
    }

    #[test]
    fn test_format_ret() {
        let inst = PhysInst::Ret;
        assert_eq!(format(&inst), "ret");
    }

    #[test]
    fn test_parse_add() {
        let inst = parse_line("add a0, a1, a2", 1).unwrap();
        assert!(matches!(inst, PhysInst::Add { dst: 10, src1: 11, src2: 12 }));
    }

    #[test]
    fn test_parse_li() {
        let inst = parse_line("li a0, 42", 1).unwrap();
        assert!(matches!(inst, PhysInst::Li { dst: 10, imm: 42 }));
    }

    #[test]
    fn test_parse_ret() {
        let inst = parse_line("ret", 1).unwrap();
        assert!(matches!(inst, PhysInst::Ret));
    }

    #[test]
    fn test_roundtrip_add() {
        let original = PhysInst::Add { dst: 10, src1: 11, src2: 12 };
        let text = format(&original);
        let parsed = parse_line(&text, 1).unwrap();
        assert_eq!(original, parsed);
    }
}
```

### 2. Tests for allocator

Add to `rv32fa/alloc.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::vinst::{VReg, VInst, SymbolRef};
    use crate::isa::rv32fa::abi;
    use alloc::vec;

    #[test]
    fn test_alloc_simple_iconst() {
        let vinsts = vec![
            VInst::IConst32 { dst: VReg(0), val: 42, size: 4 },
            VInst::Ret { src: VReg(0) },
        ];

        let phys = allocate(&vinsts, false, vec![], vec![VReg(0)]).unwrap();

        // Should have frame setup, li, ret, frame teardown
        assert!(matches!(phys[0], PhysInst::FrameSetup { .. }));
        assert!(matches!(phys[1], PhysInst::Li { dst: 10, imm: 42 })); // a0
        assert!(matches!(phys[2], PhysInst::Ret));
        assert!(matches!(phys[3], PhysInst::FrameTeardown { .. }));
    }

    #[test]
    fn test_alloc_add() {
        let vinsts = vec![
            VInst::IConst32 { dst: VReg(0), val: 10, size: 4 },
            VInst::IConst32 { dst: VReg(1), val: 20, size: 4 },
            VInst::Add32 { dst: VReg(2), src1: VReg(0), src2: VReg(1), size: 4 },
            VInst::Ret { src: VReg(2) },
        ];

        let phys = allocate(&vinsts, false, vec![], vec![VReg(2)]).unwrap();

        // Result should be in a0
        let ret_inst = &phys[phys.len() - 2];
        assert!(matches!(ret_inst, PhysInst::Ret));
    }

    #[test]
    fn test_alloc_error_on_branch() {
        let vinsts = vec![
            VInst::Br { target: 1, metadata: () },
            VInst::Label(1, ()),
            VInst::Ret { src: VReg(0) },
        ];

        let result = allocate(&vinsts, false, vec![], vec![]);
        assert!(matches!(result, Err(AllocError::UnsupportedControlFlow)));
    }
}
```

### 3. Tests for emitter

Add to `rv32fa/emit.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_emit_add() {
        let mut emitter = PhysEmitter::new();
        emitter.emit(&PhysInst::Add { dst: 10, src1: 11, src2: 12 });
        let code = emitter.finish();

        // add a0, a1, a2 = add x10, x11, x12
        // R-type: funct7(7) | rs2(5) | rs1(5) | funct3(3) | rd(5) | opcode(7)
        // add: funct7=0000000, funct3=000, opcode=0110011
        // rs2=12, rs1=11, rd=10
        // 0000000 01100 01011 000 01010 0110011 = 0x00C58533
        assert_eq!(code, &[0x33, 0x85, 0xC5, 0x00]);
    }

    #[test]
    fn test_emit_li() {
        let mut emitter = PhysEmitter::new();
        emitter.emit(&PhysInst::Li { dst: 10, imm: 42 });
        let code = emitter.finish();

        // li a0, 42 = addi x10, x0, 42
        // addi: opcode=0010011, funct3=000
        // imm=42, rs1=0, rd=10
        // 000000101010 00000 000 01010 0010011 = 0x02A00513
        assert_eq!(code, &[0x13, 0x05, 0xA0, 0x02]);
    }

    #[test]
    fn test_emit_ret() {
        let mut emitter = PhysEmitter::new();
        emitter.emit(&PhysInst::Ret);
        let code = emitter.finish();

        // ret = jalr x0, 0(ra)
        // jalr: opcode=1100111, funct3=000
        // imm=0, rs1=1(ra), rd=0
        // 000000000000 00001 000 00000 1100111 = 0x00008067
        assert_eq!(code, &[0x67, 0x80, 0x00, 0x00]);
    }
}
```

## Validate

```bash
cargo test -p lpvm-native --lib -- rv32fa
cargo test -p lp-cli -- shader_rv32
```
