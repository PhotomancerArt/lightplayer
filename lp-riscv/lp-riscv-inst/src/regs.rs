//! RISC-V 32-bit general-purpose registers.

extern crate alloc;

use core::fmt;

/// RISC-V 32-bit general-purpose register.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Gpr {
    // x0: zero register
    Zero = 0,
    // x1: return address
    Ra = 1,
    // x2: stack pointer
    Sp = 2,
    // x3: global pointer
    Gp = 3,
    // x4: thread pointer
    Tp = 4,
    // x5: temporary
    T0 = 5,
    // x6: temporary
    T1 = 6,
    // x7: temporary
    T2 = 7,
    // x8: saved register / frame pointer
    S0 = 8,
    // x9: saved register
    S1 = 9,
    // x10: argument / return value
    A0 = 10,
    // x11: argument / return value
    A1 = 11,
    // x12: argument
    A2 = 12,
    // x13: argument
    A3 = 13,
    // x14: argument
    A4 = 14,
    // x15: argument
    A5 = 15,
    // x16: argument
    A6 = 16,
    // x17: argument
    A7 = 17,
    // x18: saved register
    S2 = 18,
    // x19: saved register
    S3 = 19,
    // x20: saved register
    S4 = 20,
    // x21: saved register
    S5 = 21,
    // x22: saved register
    S6 = 22,
    // x23: saved register
    S7 = 23,
    // x24: saved register
    S8 = 24,
    // x25: saved register
    S9 = 25,
    // x26: saved register
    S10 = 26,
    // x27: saved register
    S11 = 27,
    // x28: temporary
    T3 = 28,
    // x29: temporary
    T4 = 29,
    // x30: temporary
    T5 = 30,
    // x31: temporary
    T6 = 31,
}

impl Gpr {
    /// Create a new GPR from register number (0-31).
    ///
    /// # Panics
    ///
    /// Panics if the register number is >= 32.
    pub fn new(num: u8) -> Self {
        assert!(num < 32, "Register number must be < 32");
        // Safety: We've checked that num < 32, so this is safe
        unsafe { core::mem::transmute(num) }
    }

    /// Get the register number (0-31).
    pub fn num(&self) -> u8 {
        *self as u8
    }

    /// Parse a register name string into a Gpr.
    ///
    /// Supports both named registers (zero, ra, sp, a0-a7, s0-s11, t0-t6, etc.)
    /// and numeric registers (x0-x31).
    ///
    /// # Errors
    ///
    /// Returns an error string if the register name is invalid.
    pub fn from_name(name: &str) -> Result<Self, alloc::string::String> {
        match name {
            "zero" | "x0" => Ok(Gpr::Zero),
            "ra" | "x1" => Ok(Gpr::Ra),
            "sp" | "x2" => Ok(Gpr::Sp),
            "gp" | "x3" => Ok(Gpr::Gp),
            "tp" | "x4" => Ok(Gpr::Tp),
            "t0" | "x5" => Ok(Gpr::T0),
            "t1" | "x6" => Ok(Gpr::T1),
            "t2" | "x7" => Ok(Gpr::T2),
            "s0" | "fp" | "x8" => Ok(Gpr::S0),
            "s1" | "x9" => Ok(Gpr::S1),
            "a0" | "x10" => Ok(Gpr::A0),
            "a1" | "x11" => Ok(Gpr::A1),
            "a2" | "x12" => Ok(Gpr::A2),
            "a3" | "x13" => Ok(Gpr::A3),
            "a4" | "x14" => Ok(Gpr::A4),
            "a5" | "x15" => Ok(Gpr::A5),
            "a6" | "x16" => Ok(Gpr::A6),
            "a7" | "x17" => Ok(Gpr::A7),
            "s2" | "x18" => Ok(Gpr::S2),
            "s3" | "x19" => Ok(Gpr::S3),
            "s4" | "x20" => Ok(Gpr::S4),
            "s5" | "x21" => Ok(Gpr::S5),
            "s6" | "x22" => Ok(Gpr::S6),
            "s7" | "x23" => Ok(Gpr::S7),
            "s8" | "x24" => Ok(Gpr::S8),
            "s9" | "x25" => Ok(Gpr::S9),
            "s10" | "x26" => Ok(Gpr::S10),
            "s11" | "x27" => Ok(Gpr::S11),
            "t3" | "x28" => Ok(Gpr::T3),
            "t4" | "x29" => Ok(Gpr::T4),
            "t5" | "x30" => Ok(Gpr::T5),
            "t6" | "x31" => Ok(Gpr::T6),
            _ => {
                // Try parsing as xN or numeric
                if let Some(num_str) = name.strip_prefix("x") {
                    if let Ok(num) = num_str.parse::<u8>() {
                        if num < 32 {
                            return Ok(Gpr::new(num));
                        }
                    }
                }
                Err(alloc::format!("Invalid register name: {name}"))
            }
        }
    }
}

impl fmt::Display for Gpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match *self {
            Gpr::Zero => "zero",
            Gpr::Ra => "ra",
            Gpr::Sp => "sp",
            Gpr::Gp => "gp",
            Gpr::Tp => "tp",
            Gpr::T0 => "t0",
            Gpr::T1 => "t1",
            Gpr::T2 => "t2",
            Gpr::S0 => "s0",
            Gpr::S1 => "s1",
            Gpr::A0 => "a0",
            Gpr::A1 => "a1",
            Gpr::A2 => "a2",
            Gpr::A3 => "a3",
            Gpr::A4 => "a4",
            Gpr::A5 => "a5",
            Gpr::A6 => "a6",
            Gpr::A7 => "a7",
            Gpr::S2 => "s2",
            Gpr::S3 => "s3",
            Gpr::S4 => "s4",
            Gpr::S5 => "s5",
            Gpr::S6 => "s6",
            Gpr::S7 => "s7",
            Gpr::S8 => "s8",
            Gpr::S9 => "s9",
            Gpr::S10 => "s10",
            Gpr::S11 => "s11",
            Gpr::T3 => "t3",
            Gpr::T4 => "t4",
            Gpr::T5 => "t5",
            Gpr::T6 => "t6",
        };
        write!(f, "{name}")
    }
}

#[cfg(test)]
mod tests {
    use alloc::format;

    use super::*;

    #[test]
    fn test_gpr_creation() {
        let reg = Gpr::new(5);
        assert_eq!(reg.num(), 5);
    }

    #[test]
    #[should_panic(expected = "Register number must be < 32")]
    fn test_gpr_invalid() {
        Gpr::new(32);
    }

    #[test]
    fn test_named_registers() {
        assert_eq!(Gpr::Zero.num(), 0);
        assert_eq!(Gpr::Ra.num(), 1);
        assert_eq!(Gpr::Sp.num(), 2);
        assert_eq!(Gpr::A0.num(), 10);
        assert_eq!(Gpr::A1.num(), 11);
        // Test backward compatibility constants
        assert_eq!(Gpr::Zero.num(), 0);
        assert_eq!(Gpr::Ra.num(), 1);
        assert_eq!(Gpr::Sp.num(), 2);
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", Gpr::Zero), "zero");
        assert_eq!(format!("{}", Gpr::Ra), "ra");
        assert_eq!(format!("{}", Gpr::Sp), "sp");
        assert_eq!(format!("{}", Gpr::Gp), "gp");
        assert_eq!(format!("{}", Gpr::Tp), "tp");
        assert_eq!(format!("{}", Gpr::T0), "t0");
        assert_eq!(format!("{}", Gpr::S0), "s0");
        assert_eq!(format!("{}", Gpr::A0), "a0");
        assert_eq!(format!("{}", Gpr::A1), "a1");
        assert_eq!(format!("{}", Gpr::T6), "t6");
        // Test backward compatibility constants
        assert_eq!(format!("{}", Gpr::Zero), "zero");
        assert_eq!(format!("{}", Gpr::Ra), "ra");
    }
}
