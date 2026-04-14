//! GPR index helpers for fastalloc PInst (`PReg` = `u8` x0–x31).

/// Physical GPR index (x0–x31).
pub type PReg = u8;

pub const RA_REG: PReg = 1;
pub const SP_REG: PReg = 2;
pub const FP_REG: PReg = 8;

/// Argument / return GPRs (a0–a7).
pub const ARG_REGS: [PReg; 8] = [10, 11, 12, 13, 14, 15, 16, 17];
pub const RET_REGS: [PReg; 2] = [10, 11];

/// Scratch for lowering sequences (not in [`ALLOC_POOL`]).
pub const SCRATCH: PReg = 28;

/// Registers available for temporaries (excludes zero, ra, sp, fp, a0–a7, [`SCRATCH`]).
pub const ALLOC_POOL: &[PReg] = &[5, 6, 7, 29, 30, 31, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27];

/// Caller-saved registers within [`ALLOC_POOL`] (the t-regs).
/// A call clobbers these; live vregs must be saved/restored.
pub const CALLER_SAVED_POOL: &[PReg] = &[5, 6, 7, 29, 30, 31]; // t0, t1, t2, t4, t5, t6

pub fn is_caller_saved_pool(r: PReg) -> bool {
    CALLER_SAVED_POOL.iter().any(|&x| x == r)
}

pub fn is_arg_reg(r: PReg) -> bool {
    (10..=17).contains(&r)
}

/// Parse register name to physical register number (standard RISC-V ABI names).
pub fn parse_reg(name: &str) -> Result<PReg, ()> {
    match name {
        "x0" | "zero" => Ok(0),
        "x1" | "ra" => Ok(1),
        "x2" | "sp" => Ok(2),
        "x3" | "gp" => Ok(3),
        "x4" | "tp" => Ok(4),
        "x5" | "t0" => Ok(5),
        "x6" | "t1" => Ok(6),
        "x7" | "t2" => Ok(7),
        "x8" | "s0" | "fp" => Ok(8),
        "x9" | "s1" => Ok(9),
        "x10" | "a0" => Ok(10),
        "x11" | "a1" => Ok(11),
        "x12" | "a2" => Ok(12),
        "x13" | "a3" => Ok(13),
        "x14" | "a4" => Ok(14),
        "x15" | "a5" => Ok(15),
        "x16" | "a6" => Ok(16),
        "x17" | "a7" => Ok(17),
        "x18" | "s2" => Ok(18),
        "x19" | "s3" => Ok(19),
        "x20" | "s4" => Ok(20),
        "x21" | "s5" => Ok(21),
        "x22" | "s6" => Ok(22),
        "x23" | "s7" => Ok(23),
        "x24" | "s8" => Ok(24),
        "x25" | "s9" => Ok(25),
        "x26" | "s10" => Ok(26),
        "x27" | "s11" => Ok(27),
        "x28" | "t3" => Ok(28),
        "x29" | "t4" => Ok(29),
        "x30" | "t5" => Ok(30),
        "x31" | "t6" => Ok(31),
        _ => Err(()),
    }
}

/// ABI-style name for debugging / text format (preferred alias when unambiguous).
pub fn reg_name(reg: PReg) -> &'static str {
    match reg {
        0 => "zero",
        1 => "ra",
        2 => "sp",
        3 => "gp",
        4 => "tp",
        5 => "t0",
        6 => "t1",
        7 => "t2",
        8 => "s0",
        9 => "s1",
        10 => "a0",
        11 => "a1",
        12 => "a2",
        13 => "a3",
        14 => "a4",
        15 => "a5",
        16 => "a6",
        17 => "a7",
        18 => "s2",
        19 => "s3",
        20 => "s4",
        21 => "s5",
        22 => "s6",
        23 => "s7",
        24 => "s8",
        25 => "s9",
        26 => "s10",
        27 => "s11",
        28 => "t3",
        29 => "t4",
        30 => "t5",
        31 => "t6",
        _ => "???",
    }
}

#[inline]
pub fn pool_contains(r: PReg) -> bool {
    ALLOC_POOL.iter().any(|&x| x == r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reg() {
        assert_eq!(parse_reg("a0"), Ok(10));
        assert_eq!(parse_reg("x10"), Ok(10));
        assert_eq!(parse_reg("s0"), Ok(8));
        assert_eq!(parse_reg("fp"), Ok(8));
        assert_eq!(parse_reg("ra"), Ok(1));
    }

    #[test]
    fn test_reg_name_roundtrip() {
        for i in 0..32u8 {
            let name = reg_name(i);
            assert_eq!(parse_reg(name), Ok(i), "Roundtrip failed for {}", i);
        }
    }
}
