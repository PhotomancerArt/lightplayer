//! The Xtensa builtins image: does the cross-compiled guest code actually
//! load, resolve, and compute the same answers as the host build?
//!
//! This is the base image `rt_emu_xt` links compiled shader code against, so
//! everything downstream assumes these three things hold. The oracle is the
//! **host build of the same `lps-builtins` source** — same algorithm, a
//! different compiler backend — so a mismatch means the Xtensa codegen or the
//! emulator is wrong, not that the math changed.
//!
//! Skips (loudly) when the image has not been built; see
//! `scripts/build-builtins-xt.sh`.

use std::path::PathBuf;

use lp_xt_elf::XtensaElf;
use lp_xt_emu::{Emulator, RunOutcome};

fn image_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../lp-xt/fixtures/elf/lps-builtins-xt-app.elf")
}

/// Load the image, or return `None` with a loud note when it is absent.
fn load_image() -> Option<(Emulator, Vec<u8>)> {
    let p = image_path();
    let Ok(bytes) = std::fs::read(&p) else {
        eprintln!(
            "SKIP: {} not found — run scripts/build-builtins-xt.sh (esp toolchain) first",
            p.display()
        );
        return None;
    };
    let mut emu = Emulator::new();
    {
        let elf = XtensaElf::parse(&bytes).expect("builtins image parses as Xtensa ELF32");
        elf.load_into(&mut emu)
            .expect("image loads into emulator memory");
    }
    Some((emu, bytes))
}

/// Run `symbol(arg)` on the emulator and return its `i32` result.
fn call_builtin(emu: &mut Emulator, bytes: &[u8], symbol: &str, arg: i32) -> i32 {
    let addr = {
        let elf = XtensaElf::parse(bytes).unwrap();
        elf.symbol(symbol)
            .unwrap_or_else(|| panic!("image exports {symbol}"))
    };
    let mut tracer = lp_xt_emu::NoopTracer;
    let mut noop = NoSyscalls;
    match emu.run_loaded(addr, arg as u32, &mut tracer, &mut noop) {
        RunOutcome::Ok(v) => v as i32,
        RunOutcome::Trap(t) => panic!("{symbol}({arg}) trapped: {t:?}"),
    }
}

struct NoSyscalls;
impl lp_xt_emu::SyscallHandler for NoSyscalls {
    fn syscall(
        &mut self,
        _cpu: &mut lp_xt_emu::cpu::Cpu,
        _mem: &mut lp_xt_emu::memory::Memory,
    ) -> lp_xt_emu::SyscallOutcome {
        panic!("builtin made an unexpected syscall")
    }
}

#[test]
fn image_exports_every_builtin_and_they_execute() {
    let Some((mut emu, bytes)) = load_image() else {
        return;
    };

    // Q32 is Q16.16: 1.0 == 1 << 16.
    const ONE: i32 = 1 << 16;

    // Sample across sign and magnitude, including angle-folding territory.
    for x in [0, ONE / 4, ONE / 2, ONE, 2 * ONE, -ONE, -3 * ONE, 7 * ONE] {
        let got = call_builtin(&mut emu, &bytes, "__lps_sin_q32", x);
        let want = lps_builtins::builtins::glsl::sin_q32::__lps_sin_q32(x);
        assert_eq!(
            got, want,
            "__lps_sin_q32({x}): Xtensa guest {got} vs host {want}"
        );

        let got = call_builtin(&mut emu, &bytes, "__lps_cos_q32", x);
        let want = lps_builtins::builtins::glsl::cos_q32::__lps_cos_q32(x);
        assert_eq!(
            got, want,
            "__lps_cos_q32({x}): Xtensa guest {got} vs host {want}"
        );
    }
}

/// The builtins are **flash-resident firmware**, so every one of them must
/// live in the modeled IROM window — and, just as load-bearing, none of them
/// may live in the SRAM code region, which belongs to JIT'd shader code.
///
/// This assertion used to read the other way round, back when the image shared
/// the code region with the shader. Both halves matter: a symbol outside every
/// modeled region would fetch-fault at run time rather than fail here, and a
/// symbol back inside the code region would silently re-couple the largest
/// compilable shader to the size of the builtins
/// (`docs/defects/2026-08-01-xt-f32-builtins-exhaust-the-emulator-code-region.md`).
#[test]
fn builtins_land_in_flash_and_never_in_the_shader_code_region() {
    let Some((_emu, bytes)) = load_image() else {
        return;
    };
    let elf = XtensaElf::parse(&bytes).unwrap();
    let p = lp_xt_emu::board::BoardProfile::esp32s3();
    for name in ["__lps_sin_q32", "__lps_cos_q32", "__lps_pow_q32"] {
        let a = elf.symbol(name).unwrap_or_else(|| panic!("exports {name}"));
        assert!(
            p.irom_offset(a).is_some(),
            "{name} at {a:#x} is outside the modeled IROM window {:#x}..{:#x} \
             — see lp-xt/lps-builtins-xt-app/link.ld",
            p.irom_base,
            p.irom_base + p.irom_len as u32,
        );
        assert!(
            p.code_region_offset(a).is_none(),
            "{name} at {a:#x} is inside the SRAM code region, which belongs to \
             JIT'd shader code"
        );
    }
}
