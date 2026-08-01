//! The M6 FP conformance rig: runs [`lp_xt_fp_vectors`]'s corpus on the FPU of
//! the board it is flashed to, and prints the answers.
//!
//! # Why the vectors are regenerated rather than transferred
//!
//! This harness links the **same generator crate** the host predictions were
//! made with, so vector 4 137 on the device is vector 4 137 on the host by
//! construction — no transfer protocol, no reflash per batch, no chance of the
//! two sides drifting out of step while both look healthy (M6 D3).
//!
//! "By construction" is not good enough on its own, so both sides also print
//! [`lp_xt_fp_vectors::fingerprint`] and the diff tool **aborts** on a mismatch.
//! A fingerprint disagreement means the two sides generated different inputs and
//! every comparison after it is meaningless.
//!
//! # Why the answers come back over the monitor
//!
//! The experiment repo's runner carries exactly one `u32` per payload and costs
//! a board reboot on every fault. Tens of thousands of `(result, FSR)` pairs
//! plus a 2²³ table sweep do not fit that channel, and lp2025 already has the
//! shape that does: a `test_*` feature on this crate, flashed with
//! `espflash flash --monitor`, printing results the host captures to a file
//! (M6 D1, D4). See `just fwtest-xt-fp-esp32s3`.
//!
//! # The vectors are data, not code
//!
//! The device must execute specific FP instructions on specific operands, which
//! is not something the Rust compiler can be asked for. So the instructions live
//! in [`kernels`] as `global_asm!` blocks — one per operation shape, roughly
//! fifteen of them — and every vector is a *call* into one of those, not a
//! compiled program of its own. That is what keeps the kernel count at fifteen
//! instead of 5 630, and it is why the campaign needs no FP emitter.
//!
//! `global_asm!` rather than `asm!` with `FR` operand constraints deliberately:
//! Rust's Xtensa inline-asm support has no float register class, and explicit
//! register names in textual assembly always work.
//!
//! # What this harness does not do
//!
//! It does not interpret anything. No PASS, no FAIL, no comparison against a
//! golden — the goldens live on the host, were committed before any hardware
//! ran, and the classification is the diff tool's job (M6 D2). A device that
//! decided for itself whether it agreed would be the tautology this whole
//! milestone is arranged to avoid.
//!
//! The `<div-sequence>` and `<sqrt-sequence>` pseudo-ops are printed as `skip`:
//! divide and square root are code sequences over helper instructions whose
//! semantics no available document states (the ISA RM's Table 4-46 does not
//! list them), so running "the sequence" here would mean inventing it. P6
//! measures the helpers first.

use esp_println::println;
use lp_xt_fp_vectors::{Family, OpCode, Vector, count, fingerprint, vector};

/// Marker every line carries, so a transcript can be grepped and the host
/// parser can ignore anything else the boot chain prints.
const TAG: &str = "[FPCONF]";

/// Results per output line. Eight keeps a line near 100 columns, which is short
/// enough to survive a serial hiccup as one unit and long enough that the
/// capture is not dominated by line prefixes.
const PER_LINE: usize = 8;

/// What to run, chosen at build time (see `build.rs` and the `just` recipe).
///
/// A build-time switch rather than a runtime one because this harness has no
/// input channel — it prints and never reads. `just fwtest-xt-fp-esp32s3`
/// rebuilds anyway, so the distinction costs nothing.
const MODE: &str = match option_env!("LP_FP_MODE") {
    Some(m) => m,
    None => "families",
};

/// Restrict to one family by [`Family::name`], or empty for all of them.
const ONLY_FAMILY: &str = match option_env!("LP_FP_FAMILY") {
    Some(f) => f,
    None => "",
};

/// Stop each family after this many vectors, or `0` for all of them. The smoke
/// run uses a small number; the campaign uses zero.
const LIMIT: u32 = match option_env!("LP_FP_LIMIT") {
    Some(s) => parse_u32(s),
    None => 0,
};

/// `str::parse` is not `const`, and this has to run in a `const` initializer.
const fn parse_u32(s: &str) -> u32 {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut n = 0u32;
    while i < bytes.len() {
        let b = bytes[i];
        assert!(b >= b'0' && b <= b'9', "LP_FP_LIMIT must be a decimal number");
        n = n * 10 + (b - b'0') as u32;
        i += 1;
    }
    n
}

/// The exponents the estimate-table sweep covers.
///
/// Four adjacent ones so the exponent rule's separability is visible, and two
/// parities because `rsqrt0.s` keys on the exponent's low bit.
const TABLE_EXPONENTS: [u32; 4] = [126, 127, 128, 129];

pub fn run_all() -> ! {
    println!("{TAG} BEGIN");
    println!(
        "{TAG} build commit={} dirty={} profile={}",
        env!("LP_BUILD_COMMIT"),
        env!("LP_BUILD_DIRTY"),
        env!("LP_BUILD_PROFILE"),
    );
    println!("{TAG} chip=esp32s3 arch=xtensa");

    // Arm coprocessor 0 explicitly and print both sides of it. P1 found the
    // FPU already enabled under this boot chain with the provenance unpinned —
    // no write exists in esp-hal or xtensa-lx-rt — so "was the FPU actually on"
    // must be answerable from the log rather than from a belief about the ROM.
    //
    // Measured 2026-07-31 on the desk S3: `before` is `0xff`, so *every*
    // coprocessor arrives enabled, not just the FPU's bit 0. Arming anyway
    // narrows it to bit 0, which is what M7's JIT context will do — and the
    // printed pair is what makes that a measurement rather than a belief.
    let before = unsafe { kernels::lp_fp_get_cpenable() };
    let after = unsafe { kernels::lp_fp_arm_cpenable() };
    println!("{TAG} cpenable before={before:#010x} after={after:#010x}");
    if after & 1 == 0 {
        println!("{TAG} FATAL cpenable did not arm — every result below would be a trap");
        park();
    }

    // Reset values, before anything has run. FCR's is the rounding mode the
    // whole corpus is predicted under; FSR's is the zero the sticky-flag
    // finding is measured against.
    println!(
        "{TAG} fcr-reset={:#010x} fsr-reset={:#010x}",
        unsafe { kernels::lp_fp_get_fcr() },
        unsafe { kernels::lp_fp_get_fsr() },
    );

    println!(
        "{TAG} fingerprint={:#010x} total={}",
        fingerprint(),
        lp_xt_fp_vectors::total()
    );
    println!(
        "{TAG} mode={MODE} family={} limit={LIMIT}",
        if ONLY_FAMILY.is_empty() {
            "all"
        } else {
            ONLY_FAMILY
        }
    );
    for f in Family::ALL {
        println!(
            "{TAG} plan {} {} count={}",
            f.label(),
            f.name(),
            count(f)
        );
    }

    match MODE {
        "families" => run_families(),
        "tables" => run_tables(),
        other => {
            println!("{TAG} FATAL unknown LP_FP_MODE={other}");
        }
    }

    park()
}

fn run_families() {
    let mut families = 0u32;
    let mut vectors = 0u32;
    for family in Family::ALL {
        if !ONLY_FAMILY.is_empty() && ONLY_FAMILY != family.name() {
            continue;
        }
        vectors += run_family(family);
        families += 1;
    }
    // The sentinel. A capture that stops early looks exactly like a capture that
    // finished, unless something at the end says how much there was supposed to
    // be — and a truncated capture that reads as a pass is the failure mode this
    // rig is arranged against.
    println!("{TAG} END-ALL families={families} vectors={vectors}");
}

fn run_family(family: Family) -> u32 {
    let total = count(family);
    let n = if LIMIT == 0 { total } else { LIMIT.min(total) };
    println!(
        "{TAG} FAMILY {} label={} count={n} of={total}",
        family.name(),
        family.label()
    );

    let mut digest = 0x811C_9DC5u32;
    let mut cells = 0usize;
    let mut line = Line::empty();

    for i in 0..n {
        let v = vector(family, i);
        if cells % PER_LINE == 0 {
            line.start(family, i);
        }
        match run_vector(&v) {
            Some((result, fsr)) => {
                line.cell(result, fsr);
                digest = mix(digest ^ i);
                digest = mix(digest ^ result).wrapping_add(fsr);
            }
            None => {
                line.skip();
                digest = mix(digest ^ i).wrapping_add(0xDEAD_BEEF);
            }
        }
        cells += 1;
        if cells % PER_LINE == 0 {
            line.flush();
        }
    }
    line.flush();

    println!("{TAG} DIGEST {} {digest:#010x} rows={n}", family.name());
    println!("{TAG} END family={} count={n}", family.name());
    n
}

/// Execute one vector, returning `(result bits, FSR)`, or `None` for a pseudo-op
/// this harness deliberately does not run.
///
/// FSR is cleared immediately before and read immediately after, so the value
/// reported is what *this* operation raised rather than an accumulation over the
/// whole family — the register is sticky (M6 P1) and would otherwise report the
/// first op that ever set a flag, forever.
fn run_vector(v: &Vector) -> Option<(u32, u32)> {
    unsafe {
        let _ = kernels::lp_fp_set_fcr(u32::from(v.fcr));
        let _ = kernels::lp_fp_set_fsr(0);
    }
    let result = dispatch(v)?;
    let fsr = unsafe { kernels::lp_fp_get_fsr() };
    // Leave the default mode installed: a stray non-default FCR leaking into the
    // next vector would corrupt every row after it, silently.
    unsafe {
        let _ = kernels::lp_fp_set_fcr(0);
    }
    Some((result, fsr))
}

fn dispatch(v: &Vector) -> Option<u32> {
    use kernels as k;
    let r = unsafe {
        match v.op {
            OpCode::AddS => k::lp_fp_add_s(v.a, v.b),
            OpCode::SubS => k::lp_fp_sub_s(v.a, v.b),
            OpCode::MulS => k::lp_fp_mul_s(v.a, v.b),
            OpCode::MaddS => k::lp_fp_madd_s(v.c, v.a, v.b),
            OpCode::MsubS => k::lp_fp_msub_s(v.c, v.a, v.b),
            OpCode::AbsS => k::lp_fp_abs_s(v.a),
            OpCode::NegS => k::lp_fp_neg_s(v.a),
            OpCode::MovS => k::lp_fp_mov_s(v.a),
            OpCode::OeqS => k::lp_fp_oeq_s(v.a, v.b),
            OpCode::OltS => k::lp_fp_olt_s(v.a, v.b),
            OpCode::OleS => k::lp_fp_ole_s(v.a, v.b),
            OpCode::UeqS => k::lp_fp_ueq_s(v.a, v.b),
            OpCode::UltS => k::lp_fp_ult_s(v.a, v.b),
            OpCode::UleS => k::lp_fp_ule_s(v.a, v.b),
            OpCode::UnS => k::lp_fp_un_s(v.a, v.b),
            OpCode::Recip0S => k::lp_fp_recip0_s(v.a),
            OpCode::Rsqrt0S => k::lp_fp_rsqrt0_s(v.a),
            OpCode::Sqrt0S => k::lp_fp_sqrt0_s(v.a),
            OpCode::Div0S => k::lp_fp_div0_s(v.a),
            OpCode::FloatS => return scaled(v, kernels::FLOAT_S),
            OpCode::UfloatS => return scaled(v, kernels::UFLOAT_S),
            OpCode::TruncS => return scaled(v, kernels::TRUNC_S),
            OpCode::UtruncS => return scaled(v, kernels::UTRUNC_S),
            OpCode::RoundS => return scaled(v, kernels::ROUND_S),
            OpCode::FloorS => return scaled(v, kernels::FLOOR_S),
            OpCode::CeilS => return scaled(v, kernels::CEIL_S),
            // Not instructions. See the module doc.
            OpCode::Div | OpCode::Sqrt => return None,
        }
    };
    Some(r)
}

/// The conversions' scale is an *immediate*, so it cannot be passed in a
/// register — there is one kernel per `(operation, scale)` pair.
///
/// Only the four scales the corpus actually generates are built, and an
/// unexpected one is a loud `None` rather than a silently substituted zero: if
/// the generator grows a fifth scale, this must fail visibly rather than report
/// answers for the wrong instruction.
fn scaled(v: &Vector, table: kernels::ScaleTable) -> Option<u32> {
    let f = table.get(v.imm)?;
    Some(unsafe { f(v.a) })
}

/// The estimate-table extraction (M6 D5).
///
/// `recip0.s` and friends read an implementation-defined lookup ROM, so the only
/// way to be exact rather than close is to read the whole thing back. The
/// significand space is 2²³ wide but the output is a step function over it, so
/// the sweep is run-length encoded and the transcript stays small — a few
/// hundred runs per exponent instead of eight million values.
fn run_tables() {
    let ops: [(&str, unsafe extern "C" fn(u32) -> u32); 4] = [
        ("recip0.s", kernels::lp_fp_recip0_s),
        ("rsqrt0.s", kernels::lp_fp_rsqrt0_s),
        ("sqrt0.s", kernels::lp_fp_sqrt0_s),
        ("div0.s", kernels::lp_fp_div0_s),
    ];
    let mut tables = 0u32;
    for (name, f) in ops {
        for exp in TABLE_EXPONENTS {
            tables += 1;
            sweep_one(name, exp, f);
        }
    }
    println!("{TAG} END-ALL tables={tables} exponents={}", TABLE_EXPONENTS.len());
}

fn sweep_one(name: &str, exp: u32, f: unsafe extern "C" fn(u32) -> u32) {
    println!("{TAG} TABLE op={name} exp={exp} significand-bits=23");
    let base = exp << 23;
    let mut runs = 0u32;
    let mut run_start = 0u32;
    let mut run_value = unsafe { f(base) };
    let mut line = Line::empty();
    let mut cells = 0usize;

    // `..=` on the last significand so the final run closes inside the loop
    // rather than needing a duplicate emit after it.
    for frac in 1..=0x0080_0000u32 {
        let value = if frac == 0x0080_0000 {
            // One past the end: a sentinel that can never equal a real result,
            // so the last real run is always flushed by the inequality below.
            !run_value
        } else {
            unsafe { f(base | frac) }
        };
        if value == run_value {
            continue;
        }
        if cells % PER_LINE == 0 {
            line.start_table(name, exp);
        }
        line.run(run_start, frac - run_start, run_value);
        cells += 1;
        if cells % PER_LINE == 0 {
            line.flush();
        }
        runs += 1;
        run_start = frac;
        run_value = value;
    }
    line.flush();
    println!("{TAG} END table={name} exp={exp} runs={runs}");
}

/// One output line, assembled without an allocator.
///
/// `esp_println` writes a whole `println!` at a time, so building the line here
/// and printing it once is what keeps eight results on one line instead of
/// eight lines with a prefix each.
struct Line {
    buf: heapless_line::Buf,
    open: bool,
}

impl Line {
    fn empty() -> Line {
        Line {
            buf: heapless_line::Buf::empty(),
            open: false,
        }
    }

    fn start(&mut self, family: Family, first: u32) {
        self.buf.clear();
        self.buf.str("D ");
        self.buf.str(family.name());
        self.buf.byte(b' ');
        self.buf.dec5(first);
        self.open = true;
    }

    fn start_table(&mut self, op: &str, exp: u32) {
        self.buf.clear();
        self.buf.str("T ");
        self.buf.str(op);
        self.buf.byte(b' ');
        self.buf.dec5(exp);
        self.open = true;
    }

    fn cell(&mut self, result: u32, fsr: u32) {
        self.buf.byte(b' ');
        self.buf.hex8(result);
        self.buf.byte(b':');
        self.buf.hex8(fsr);
    }

    fn skip(&mut self) {
        self.buf.str(" skip");
    }

    /// One run-length-encoded step of an estimate table: first significand,
    /// how many consecutive significands share the answer, and the answer.
    fn run(&mut self, start: u32, len: u32, value: u32) {
        self.buf.byte(b' ');
        self.buf.hex8(start);
        self.buf.byte(b':');
        self.buf.hex8(len);
        self.buf.byte(b':');
        self.buf.hex8(value);
    }

    fn flush(&mut self) {
        if self.open {
            println!("{TAG} {}", self.buf.as_str());
            self.open = false;
        }
    }
}

/// A fixed-capacity line buffer. Deliberately not `heapless` or `String`: the
/// harness must be able to run before, and independently of, anything that
/// allocates, and the only formatting it needs is fixed-width hex.
mod heapless_line {
    const CAP: usize = 200;

    pub struct Buf {
        bytes: [u8; CAP],
        len: usize,
    }

    impl Buf {
        pub fn empty() -> Buf {
            Buf {
                bytes: [0; CAP],
                len: 0,
            }
        }

        pub fn clear(&mut self) {
            self.len = 0;
        }

        pub fn byte(&mut self, b: u8) {
            if self.len < CAP {
                self.bytes[self.len] = b;
                self.len += 1;
            }
        }

        pub fn str(&mut self, s: &str) {
            for b in s.bytes() {
                self.byte(b);
            }
        }

        pub fn hex8(&mut self, v: u32) {
            for shift in (0..8).rev() {
                let nib = ((v >> (shift * 4)) & 0xF) as u8;
                self.byte(if nib < 10 { b'0' + nib } else { b'a' + nib - 10 });
            }
        }

        /// Five zero-padded decimal digits, matching the corpus files' index
        /// column so a row can be found in both by eye.
        pub fn dec5(&mut self, v: u32) {
            let mut d = [0u8; 5];
            let mut n = v;
            for slot in d.iter_mut().rev() {
                *slot = b'0' + (n % 10) as u8;
                n /= 10;
            }
            for b in d {
                self.byte(b);
            }
        }

        pub fn as_str(&self) -> &str {
            // Every byte written above is ASCII.
            core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("<non-utf8>")
        }
    }
}

/// The same mix the generator uses, so a digest can be recomputed on the host
/// with no extra agreement to maintain.
const fn mix(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    x
}

fn park() -> ! {
    println!("{TAG} done");
    loop {
        core::hint::spin_loop();
    }
}

/// The FP instructions themselves.
///
/// Every kernel has the same shape: `entry` to open a window, move operands from
/// address registers into float registers with `wfr`, run exactly one
/// instruction, move the answer back with `rfr`, `retw`. Nothing else happens in
/// between, so the value that comes out is that instruction's answer and not the
/// compiler's opinion of it.
///
/// Assembled from mnemonics — no hand-written encodings, and no assembler source
/// adapted from binutils, GCC, or QEMU (AGENTS.md license rule). The syntax
/// matches `lp-xt/fixtures/fp/probe.S`, which assembled and ran on this silicon
/// in the M6 P1 desk session.
mod kernels {
    /// Emit one kernel and its declaration together, so a kernel can never exist
    /// in assembly without a Rust signature or the other way round.
    macro_rules! fp_kernel {
        ($name:ident ( $($arg:ident : u32),* ) { $( [ $($piece:literal),* ] ),* $(,)? }) => {
            core::arch::global_asm!(concat!(
                ".section .text.", stringify!($name), ",\"ax\",@progbits\n",
                ".align 4\n",
                ".global ", stringify!($name), "\n",
                ".type ", stringify!($name), ",@function\n",
                stringify!($name), ":\n",
                "  entry a1, 32\n",
                $("  ", $($piece,)* "\n",)*
                "  retw\n",
                ".size ", stringify!($name), ", .-", stringify!($name), "\n",
            ));
            unsafe extern "C" {
                pub fn $name($($arg: u32),*) -> u32;
            }
        };
    }

    // --- coprocessor and control registers --------------------------------
    // `wsr.cpenable` + `isync` is the arming sequence; the read-back afterwards
    // is what makes the printed line evidence rather than an assertion.
    fp_kernel!(lp_fp_arm_cpenable() {
        ["movi a2, 1"],
        ["wsr.cpenable a2"],
        ["isync"],
        ["rsr.cpenable a2"],
    });
    fp_kernel!(lp_fp_get_cpenable() { ["rsr.cpenable a2"] });
    fp_kernel!(lp_fp_set_fcr(v: u32) { ["wur.fcr a2"] });
    fp_kernel!(lp_fp_get_fcr() { ["rur.fcr a2"] });
    fp_kernel!(lp_fp_set_fsr(v: u32) { ["wur.fsr a2"] });
    fp_kernel!(lp_fp_get_fsr() { ["rur.fsr a2"] });

    // --- arithmetic ---------------------------------------------------------
    macro_rules! fp_binop {
        ($name:ident, $mnemonic:literal) => {
            fp_kernel!($name(a: u32, b: u32) {
                ["wfr f1, a2"],
                ["wfr f2, a3"],
                [$mnemonic, " f0, f1, f2"],
                ["rfr a2, f0"],
            });
        };
    }
    fp_binop!(lp_fp_add_s, "add.s");
    fp_binop!(lp_fp_sub_s, "sub.s");
    fp_binop!(lp_fp_mul_s, "mul.s");

    // madd/msub accumulate into their destination, so the accumulator is a
    // third operand and has to be staged in the destination register.
    macro_rules! fp_ternop {
        ($name:ident, $mnemonic:literal) => {
            fp_kernel!($name(acc: u32, a: u32, b: u32) {
                ["wfr f0, a2"],
                ["wfr f1, a3"],
                ["wfr f2, a4"],
                [$mnemonic, " f0, f1, f2"],
                ["rfr a2, f0"],
            });
        };
    }
    fp_ternop!(lp_fp_madd_s, "madd.s");
    fp_ternop!(lp_fp_msub_s, "msub.s");

    macro_rules! fp_unop {
        ($name:ident, $mnemonic:literal) => {
            fp_kernel!($name(a: u32) {
                ["wfr f1, a2"],
                [$mnemonic, " f0, f1"],
                ["rfr a2, f0"],
            });
        };
    }
    fp_unop!(lp_fp_abs_s, "abs.s");
    fp_unop!(lp_fp_neg_s, "neg.s");
    fp_unop!(lp_fp_mov_s, "mov.s");
    fp_unop!(lp_fp_recip0_s, "recip0.s");
    fp_unop!(lp_fp_rsqrt0_s, "rsqrt0.s");
    fp_unop!(lp_fp_sqrt0_s, "sqrt0.s");
    fp_unop!(lp_fp_div0_s, "div0.s");

    // --- compares -----------------------------------------------------------
    // The result lands in a *boolean* register, never an AR, so it has to be
    // brought back across with `movt`. Three readback paths were proven on this
    // silicon in P1 (`bf`, `movt`, `rsr.br`); `movt` is the branch-free one.
    macro_rules! fp_cmp {
        ($name:ident, $mnemonic:literal) => {
            fp_kernel!($name(a: u32, b: u32) {
                ["wfr f1, a2"],
                ["wfr f2, a3"],
                [$mnemonic, " b0, f1, f2"],
                ["movi a2, 0"],
                ["movi a4, 1"],
                ["movt a2, a4, b0"],
            });
        };
    }
    fp_cmp!(lp_fp_oeq_s, "oeq.s");
    fp_cmp!(lp_fp_olt_s, "olt.s");
    fp_cmp!(lp_fp_ole_s, "ole.s");
    fp_cmp!(lp_fp_ueq_s, "ueq.s");
    fp_cmp!(lp_fp_ult_s, "ult.s");
    fp_cmp!(lp_fp_ule_s, "ule.s");
    fp_cmp!(lp_fp_un_s, "un.s");

    // --- conversions --------------------------------------------------------
    // The scale is encoded in the instruction word, so it cannot be a register
    // argument: each `(operation, scale)` pair is its own kernel. Only the four
    // scales `lp-xt-fp-vectors` generates are built — `ScaleTable::get` returns
    // `None` for anything else rather than substituting a scale that would
    // silently answer for a different instruction.
    macro_rules! fp_to_int {
        ($name:ident, $mnemonic:literal, $imm:literal) => {
            fp_kernel!($name(a: u32) {
                ["wfr f1, a2"],
                [$mnemonic, " a2, f1, ", $imm],
            });
        };
    }
    macro_rules! fp_from_int {
        ($name:ident, $mnemonic:literal, $imm:literal) => {
            fp_kernel!($name(a: u32) {
                [$mnemonic, " f0, a2, ", $imm],
                ["rfr a2, f0"],
            });
        };
    }

    /// The scales the corpus uses. Kept next to the kernels so adding one is a
    /// single edit that fails to compile until every kernel exists.
    pub const SCALES: [u8; 4] = [0, 1, 4, 15];

    /// The four `(scale, kernel)` pairs for one conversion instruction.
    pub struct ScaleTable(pub [unsafe extern "C" fn(u32) -> u32; 4]);

    impl ScaleTable {
        pub fn get(&self, imm: u8) -> Option<unsafe extern "C" fn(u32) -> u32> {
            let i = SCALES.iter().position(|s| *s == imm)?;
            Some(self.0[i])
        }
    }

    macro_rules! conversions {
        ($mnemonic:literal, $table:ident, $emit:ident, $k0:ident, $k1:ident, $k4:ident, $k15:ident) => {
            $emit!($k0, $mnemonic, 0);
            $emit!($k1, $mnemonic, 1);
            $emit!($k4, $mnemonic, 4);
            $emit!($k15, $mnemonic, 15);
            pub const $table: ScaleTable = ScaleTable([$k0, $k1, $k4, $k15]);
        };
    }

    conversions!(
        "trunc.s", TRUNC_S, fp_to_int,
        lp_fp_trunc_s_0, lp_fp_trunc_s_1, lp_fp_trunc_s_4, lp_fp_trunc_s_15
    );
    conversions!(
        "utrunc.s", UTRUNC_S, fp_to_int,
        lp_fp_utrunc_s_0, lp_fp_utrunc_s_1, lp_fp_utrunc_s_4, lp_fp_utrunc_s_15
    );
    conversions!(
        "round.s", ROUND_S, fp_to_int,
        lp_fp_round_s_0, lp_fp_round_s_1, lp_fp_round_s_4, lp_fp_round_s_15
    );
    conversions!(
        "floor.s", FLOOR_S, fp_to_int,
        lp_fp_floor_s_0, lp_fp_floor_s_1, lp_fp_floor_s_4, lp_fp_floor_s_15
    );
    conversions!(
        "ceil.s", CEIL_S, fp_to_int,
        lp_fp_ceil_s_0, lp_fp_ceil_s_1, lp_fp_ceil_s_4, lp_fp_ceil_s_15
    );
    conversions!(
        "float.s", FLOAT_S, fp_from_int,
        lp_fp_float_s_0, lp_fp_float_s_1, lp_fp_float_s_4, lp_fp_float_s_15
    );
    conversions!(
        "ufloat.s", UFLOAT_S, fp_from_int,
        lp_fp_ufloat_s_0, lp_fp_ufloat_s_1, lp_fp_ufloat_s_4, lp_fp_ufloat_s_15
    );
}
