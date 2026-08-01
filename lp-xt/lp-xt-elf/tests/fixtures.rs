//! Runs every fixture ELF (built by `fixtures/build.sh`) through the loader +
//! emulator and asserts its printed output against a host-side oracle that
//! mirrors the guest computation (differential: same Rust semantics, host vs
//! emulated Xtensa).
//!
//! If the fixtures have not been built, each test SKIPs with a note (the esp
//! toolchain is not required for the stable host workspace to stay green).

use lp_xt_elf::{abi, run_elf};
use lp_xt_emu::RunOutcome;
use std::fmt::Write as _;
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../fixtures/elf/{name}.elf"))
}

/// Load and run a fixture with `arg`, returning `(output, run)`. `None` = the
/// ELF is missing (fixtures not built) — the caller should skip.
fn run_fixture(name: &str, arg: u32) -> Option<lp_xt_elf::GuestRun> {
    let path = fixture_path(name);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(_) => {
            eprintln!(
                "SKIP {name}: {} not found — run fixtures/build.sh (esp toolchain) first",
                path.display()
            );
            return None;
        }
    };
    Some(run_elf(&bytes, arg).expect("load fixture ELF"))
}

/// Assert a fixture runs to a clean exit 0 with exactly `expected` output.
#[track_caller]
fn assert_fixture(name: &str, expected: &str) {
    let Some(run) = run_fixture(name, 0) else {
        return;
    };
    assert_eq!(
        run.panic,
        None,
        "{name} panicked; output so far:\n{}",
        run.output_str()
    );
    assert_eq!(
        run.outcome,
        RunOutcome::Ok(0),
        "{name} did not exit cleanly; output so far:\n{}",
        run.output_str()
    );
    assert_eq!(run.exit_code, Some(0), "{name} exit code");
    assert_eq!(run.output_str(), expected, "{name} output");
}

// ---------------------------------------------------------------------------
// Host-side oracles: line-for-line mirrors of the guest fixtures. Keep in
// sync with fixtures/corpus/src/bin/*.rs.
// ---------------------------------------------------------------------------

#[test]
fn arith_overflow() {
    let a: u32 = 0xDEAD_BEEF;
    let b: u32 = 0x1234_5678;
    let s: i32 = -1000;
    let mut e = String::new();
    let _ = writeln!(e, "wadd={}", a.wrapping_add(b));
    let _ = writeln!(e, "wsub={}", b.wrapping_sub(a));
    let _ = writeln!(e, "wmul={}", a.wrapping_mul(2654435761));
    let _ = writeln!(e, "checked_none={:?}", u32::MAX.checked_add(1));
    let _ = writeln!(e, "checked_some={:?}", 40u32.checked_add(2));
    let _ = writeln!(e, "sar={}", s >> 3);
    let _ = writeln!(e, "shl={}", (s as u32) << 5);
    let _ = writeln!(e, "imul={}", s.wrapping_mul(-7654321));
    let _ = writeln!(e, "iwrap={}", i32::MIN.wrapping_sub(1));
    assert_fixture("arith_overflow", &e);
}

#[test]
fn array_sum() {
    let mut arr = [0u32; 64];
    for (i, slot) in arr.iter_mut().enumerate() {
        *slot = (i as u32).wrapping_mul(2654435761) >> 16;
    }
    let sum = arr.iter().fold(0u32, |acc, &v| acc.wrapping_add(v));
    let mut e = String::new();
    let _ = writeln!(e, "sum={sum}");
    arr[10..30].fill(7);
    let sum2 = arr.iter().fold(0u32, |acc, &v| acc.wrapping_add(v));
    let _ = writeln!(e, "sum2={sum2}");
    let bsum: u32 = [0xABu8; 33].iter().map(|&b| b as u32).sum();
    let _ = writeln!(e, "bsum={bsum}");
    assert_fixture("array_sum", &e);
}

#[test]
fn fib_rec() {
    fn fib(n: u32) -> u32 {
        if n < 2 {
            n
        } else {
            fib(n - 1).wrapping_add(fib(n - 2))
        }
    }
    let mut e = String::new();
    for n in [0u32, 1, 2, 5, 10, 15, 20] {
        let _ = writeln!(e, "fib({})={}", n, fib(n));
    }
    assert_fixture("fib_rec", &e);
}

#[test]
fn ackermann() {
    fn ack(m: u32, n: u32) -> u32 {
        if m == 0 {
            n + 1
        } else if n == 0 {
            ack(m - 1, 1)
        } else {
            ack(m - 1, ack(m, n - 1))
        }
    }
    let mut e = String::new();
    let _ = writeln!(e, "ack(1,10)={}", ack(1, 10));
    let _ = writeln!(e, "ack(2,3)={}", ack(2, 3));
    let _ = writeln!(e, "ack(3,3)={}", ack(3, 3));
    let _ = writeln!(e, "ack(3,5)={}", ack(3, 5));
    assert_fixture("ackermann", &e);
}

#[test]
fn call_conv() {
    #[allow(
        clippy::too_many_arguments,
        reason = "mirrors the guest fixture exactly"
    )]
    fn many(a: u32, b: u32, c: u32, d: u32, e: u32, f: u32, g: u32, h: u32) -> u32 {
        (a ^ b)
            .wrapping_add(c.wrapping_mul(d))
            .wrapping_sub(e)
            .wrapping_add(f << 2)
            .wrapping_add(g % h)
    }
    let x: u32 = 0xABCD_EF01;
    let (qa, qb, qc, qd) = (
        x.wrapping_add(1),
        x.wrapping_mul(3),
        x ^ 0x00FF_00FF,
        x.rotate_left(9),
    );
    let y: u32 = 0x1357_9BDF;
    let bsum = (0..8u32).fold(0u32, |acc, i| acc.wrapping_add(y.wrapping_mul(i + 1) ^ i));
    let wide = 0x1_0000_0001u64.wrapping_mul(3).wrapping_add(0xFFFF_FFFF);
    let mut e = String::new();
    let _ = writeln!(e, "many={}", many(1, 2, 3, 4, 5, 6, 7, 8));
    let _ = writeln!(
        e,
        "many2={}",
        many(0xDEAD, 0xBEEF, 0x1234, 0x5678, 99, 1000, 123456, 789)
    );
    let _ = writeln!(e, "quad={qa},{qb},{qc},{qd}");
    let _ = writeln!(e, "bigsum={bsum}");
    let _ = writeln!(e, "wide={wide}");
    assert_fixture("call_conv", &e);
}

#[test]
fn jump_table() {
    fn dispatch(op: u32, x: u32) -> u32 {
        match op % 12 {
            0 => x.wrapping_add(1),
            1 => x.wrapping_mul(3),
            2 => x ^ 0x5A5A,
            3 => x >> 3,
            4 => x << 2,
            5 => x.rotate_left(7),
            6 => x.count_ones(),
            7 => x.wrapping_sub(99),
            8 => x | 0x0001_0101,
            9 => x & 0xFFFF,
            10 => x.swap_bytes(),
            _ => !x,
        }
    }
    let mut x = 1u32;
    for i in 0..48u32 {
        x = dispatch(i, x).wrapping_add(i);
    }
    assert_fixture("jump_table", &format!("x={x}\n"));
}

#[test]
fn bit_ops() {
    let v: u32 = 0xF00D_CAFE;
    let w: u64 = 0x0123_4567_89AB_CDEF;
    let mut e = String::new();
    let _ = writeln!(e, "ones={}", v.count_ones());
    let _ = writeln!(e, "zeros={}", v.count_zeros());
    let _ = writeln!(e, "lz={}", 0x0000_1000u32.leading_zeros());
    let _ = writeln!(e, "tz={}", 0x0000_1000u32.trailing_zeros());
    let _ = writeln!(e, "lz0={}", 0u32.leading_zeros());
    let _ = writeln!(e, "rotl={}", v.rotate_left(13));
    let _ = writeln!(e, "rotr={}", v.rotate_right(7));
    let _ = writeln!(e, "swap={}", v.swap_bytes());
    let _ = writeln!(e, "rev={}", v.reverse_bits());
    let _ = writeln!(e, "ones64={}", w.count_ones());
    let _ = writeln!(e, "lz64={}", w.leading_zeros());
    let _ = writeln!(e, "swap64={}", w.swap_bytes());
    assert_fixture("bit_ops", &e);
}

#[test]
fn state_machine() {
    const INPUT: &[u8] = b"let x1 = 42; if x1 > 9 { emit(x1, 0xFF); } // done 2026";
    #[derive(Clone, Copy, PartialEq)]
    enum State {
        Space,
        Word,
        Number,
    }
    let mut state = State::Space;
    let (mut words, mut numbers, mut transitions) = (0u32, 0u32, 0u32);
    for &b in INPUT {
        let next = if b.is_ascii_alphabetic() || b == b'_' {
            State::Word
        } else if b.is_ascii_digit() {
            if state == State::Word {
                State::Word
            } else {
                State::Number
            }
        } else {
            State::Space
        };
        if next != state {
            transitions += 1;
            match next {
                State::Word => words += 1,
                State::Number => numbers += 1,
                State::Space => {}
            }
        }
        state = next;
    }
    let mut e = String::new();
    let _ = writeln!(e, "words={words}");
    let _ = writeln!(e, "numbers={numbers}");
    let _ = writeln!(e, "transitions={transitions}");
    let _ = writeln!(e, "len={}", INPUT.len());
    assert_fixture("state_machine", &e);
}

#[test]
fn string_fmt() {
    let mut e = String::new();
    let _ = writeln!(e, "[{:>8}]", 42);
    let _ = writeln!(e, "[{:<8}]", -42);
    let _ = writeln!(e, "[{:08x}]", 0xBEEFu32);
    let _ = writeln!(e, "[{:#010X}]", 0xDEAD_BEEFu32);
    let _ = writeln!(e, "[{:b}]", 0b1011_0101u32);
    let _ = writeln!(e, "[{:o}]", 0o755u32);
    let _ = writeln!(e, "[{:+}]", 17i32);
    let _ = writeln!(e, "[{}]", i32::MIN);
    let _ = writeln!(e, "[{}]", u64::MAX);
    let _ = writeln!(e, "[{}]", 1234567890123456789u64);
    let _ = writeln!(e, "[{:^11}]", "mid");
    let _ = writeln!(e, "[{:?}]", (1u8, -2i16, 3u32));
    assert_fixture("string_fmt", &e);
}

#[test]
fn div_rem() {
    fn div32(a: i32, b: i32) -> (i32, i32) {
        (a / b, a % b)
    }
    fn divu32(a: u32, b: u32) -> (u32, u32) {
        (a / b, a % b)
    }
    let big: u64 = 0x0123_4567_89AB_CDEF;
    let mut e = String::new();
    let _ = writeln!(e, "s={:?}", div32(-7, 2));
    let _ = writeln!(e, "s2={:?}", div32(7, -2));
    let _ = writeln!(e, "s3={:?}", div32(-2147483647, 3));
    let _ = writeln!(e, "u={:?}", divu32(0xFFFF_FFFF, 10));
    let _ = writeln!(e, "u2={:?}", divu32(12345, 12346));
    let _ = writeln!(e, "edge={:?}", i32::MIN.checked_div(-1));
    let _ = writeln!(e, "zero={:?}", 5i32.checked_div(0));
    let _ = writeln!(e, "d64={}", big / 1_000_000_007);
    let _ = writeln!(e, "r64={}", big % 1_000_000_007);
    let _ = writeln!(e, "i64={}", (-1234567890123i64) / 4096);
    assert_fixture("div_rem", &e);
}

#[test]
fn mul_wide() {
    fn mulhi_u(a: u32, b: u32) -> u32 {
        ((a as u64 * b as u64) >> 32) as u32
    }
    fn mulhi_s(a: i32, b: i32) -> i32 {
        ((a as i64 * b as i64) >> 32) as i32
    }
    let mut e = String::new();
    let _ = writeln!(e, "lo={}", 0xDEAD_BEEFu32.wrapping_mul(0xCAFE_BABE));
    let _ = writeln!(e, "hiu={}", mulhi_u(0xDEAD_BEEF, 0xCAFE_BABE));
    let _ = writeln!(e, "his={}", mulhi_s(-559038737, 19088743));
    let _ = writeln!(e, "m64={}", 0x1234_5678_9ABCu64.wrapping_mul(0xFEDC_BA98));
    let _ = writeln!(e, "m64s={}", (-123456789012i64).wrapping_mul(987654321));
    let _ = writeln!(e, "sq={}", 0xFFFF_FFFFu64.wrapping_mul(0xFFFF_FFFF));
    assert_fixture("mul_wide", &e);
}

#[test]
fn sort_insertion() {
    let mut arr = [0u32; 32];
    let mut seed = 0x1234_5678u32;
    for slot in arr.iter_mut() {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        *slot = seed >> 8;
    }
    arr.sort_unstable();
    let checksum = arr.iter().enumerate().fold(0u32, |acc, (i, &v)| {
        acc.wrapping_add(v.rotate_left(i as u32))
    });
    let mut e = String::new();
    let _ = writeln!(e, "sorted=true");
    let _ = writeln!(e, "min={}", arr[0]);
    let _ = writeln!(e, "max={}", arr[31]);
    let _ = writeln!(e, "checksum={checksum}");
    assert_fixture("sort_insertion", &e);
}

#[test]
fn alloc_vec() {
    let mut v: Vec<u32> = Vec::new();
    let mut seed = 0xCAFE_F00Du32;
    for _ in 0..50 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        v.push(seed % 1000);
    }
    let sum = v.iter().fold(0u32, |acc, &x| acc.wrapping_add(x));
    let mut e = String::new();
    let _ = writeln!(e, "len={} sum={}", v.len(), sum);
    v.sort_unstable();
    let _ = writeln!(e, "first={} last={}", v[0], v[49]);
    let mut s = String::new();
    for &x in v.iter().take(5) {
        let _ = write!(s, "{x:03},");
    }
    let _ = writeln!(e, "head={s}");
    assert_fixture("alloc_vec", &e);
}

/// The panic path: SYS_PANIC reports the message, the run terminates with the
/// panic exit code, and output printed before the panic is preserved.
#[test]
fn panic_report() {
    let Some(run) = run_fixture("panic_report", 7) else {
        return;
    };
    assert_eq!(run.output_str(), "before_panic\n");
    let msg = run.panic.expect("panic message recorded");
    assert!(
        msg.contains("boom: arg=7"),
        "unexpected panic message: {msg}"
    );
    assert_eq!(run.exit_code, Some(abi::PANIC_EXIT_CODE));
    assert_eq!(run.outcome, RunOutcome::Ok(abi::PANIC_EXIT_CODE));
}

/// Loader validation: flipping e_machine makes parse reject the file.
#[test]
fn rejects_non_xtensa() {
    let path = fixture_path("fib_rec");
    let Ok(mut bytes) = std::fs::read(&path) else {
        eprintln!("SKIP rejects_non_xtensa: fixtures not built");
        return;
    };
    bytes[18] = 0x3e; // e_machine = EM_X86_64
    let err = lp_xt_elf::XtensaElf::parse(&bytes).unwrap_err();
    assert!(
        matches!(err, lp_xt_elf::ElfError::NotXtensaElf32 { .. }),
        "got {err:?}"
    );
}

/// Symbol lookup finds `_start` at the ELF entry point.
#[test]
fn symbol_lookup() {
    let path = fixture_path("fib_rec");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("SKIP symbol_lookup: fixtures not built");
        return;
    };
    let elf = lp_xt_elf::XtensaElf::parse(&bytes).expect("parse");
    assert_eq!(elf.symbol("_start"), Some(elf.entry()));
    assert_eq!(elf.symbol("no_such_symbol_exists"), None);
}
