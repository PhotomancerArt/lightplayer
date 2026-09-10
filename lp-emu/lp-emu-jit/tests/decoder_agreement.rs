//! The guard that makes a second decoder acceptable (M7 JD3).
//!
//! `lp-emu-jit` decodes independently of `lp-riscv-emu`'s executors, because a
//! translator has to know what an instruction *is* without running it and this
//! repository's decode is fused into execution. Two decoders is a divergence
//! risk, and a decode divergence does not announce itself: it shows up as a
//! cycle count that is a few parts per million off, three images and three
//! weeks later. So the two are held to each other here, as a build failure.
//!
//! The oracle is `lp_riscv_emu::emu::class_oracle::class_of`, which runs the
//! executors themselves on scratch state and reports what they did. Not a
//! hand-maintained table: a third classification to keep in step would be
//! worse than the problem.
//!
//! # The property, stated exactly
//!
//! The two decoders are **not** symmetric, and pretending otherwise would mean
//! loosening the half that matters. What is asserted:
//!
//! 1. **Whenever `lp-emu-jit` accepts, the interpreter accepts, with the same
//!    `(width, InstClass)`.** No exceptions, ever. This is the whole safety
//!    property: translated code that charges a different class or advances the
//!    pc by a different width is silently wrong.
//! 2. **Whenever the interpreter refuses, `lp-emu-jit` refuses.** Same thing,
//!    contrapositive.
//! 3. **Where `lp-emu-jit` refuses and the interpreter accepts**, the encoding
//!    must belong to a *named* family — an extension this chip does not have,
//!    or an encoding the base ISA reserves and the interpreter tolerates. An
//!    unnamed one is a failure. Refusing is safe (the block ends and the
//!    interpreter runs it), but "safe" is not "unexamined": without (3) a
//!    decoder that refused everything would pass this test.
//! 4. **Every named family is actually reached** by the sweep, so a family
//!    cannot rot into a permanent excuse for something that no longer happens.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use lp_riscv_emu::emu::class_oracle::class_of;

// ---------------------------------------------------------------------------
// The families of encoding `lp-emu-jit` refuses on purpose.
//
// Each is a thing the C6 either does not have or the base ISA reserves, and
// each is named rather than waved at, so the sweep can insist all of them are
// real and none of them is a wildcard.
// ---------------------------------------------------------------------------

const F_SYSTEM: &str = "SYSTEM: the hart's own (ecall/ebreak/mret/wfi/CSR)";
const F_ATOMIC: &str = "the A extension";
const F_FLOAT: &str = "RV32F";
const F_FENCE_I: &str = "fence.i and the reserved MISC-MEM funct3 values";
const F_BITMANIP_R: &str = "Zba/Zbb/Zbs register-register (the C6 has no B)";
const F_SHIFT_FUNCT7: &str = "an OP-IMM shift whose funct7 the base ISA does not define (a Zb* operation, \
     or a reserved shamt[5] the interpreter masks away)";
const F_JALR_FUNCT3: &str = "jalr with a nonzero funct3, which RV32I reserves";
const F_C_EBREAK: &str = "c.ebreak — SYSTEM, in sixteen bits";
const F_C_SHAMT: &str = "an RVC shift with shamt[5] set, which RV32C reserves and \
                         the interpreter masks away";

const FAMILIES: &[&str] = &[
    F_SYSTEM,
    F_ATOMIC,
    F_FLOAT,
    F_FENCE_I,
    F_BITMANIP_R,
    F_SHIFT_FUNCT7,
    F_JALR_FUNCT3,
    F_C_EBREAK,
    F_C_SHAMT,
];

/// The `(funct3, funct7)` pairs of the bit-manipulation extensions the
/// interpreter implements in its R-type executor. Spelled out rather than
/// matched by exclusion, so that a *base* encoding appearing here — which
/// would be a real gap in `lp-emu-jit` — cannot be swallowed as "some Zb*
/// thing".
const BITMANIP_R: &[(u32, u32)] = &[
    (0x1, 0x30), // rol
    (0x5, 0x30), // ror
    (0x7, 0x20), // andn
    (0x6, 0x20), // orn
    (0x4, 0x20), // xnor
    (0x4, 0x04), // zext.h (`pack rd, rs, x0`)
    (0x4, 0x05), // min
    (0x5, 0x05), // minu
    (0x6, 0x05), // max
    (0x7, 0x05), // maxu
    (0x1, 0x24), // bclr
    (0x5, 0x24), // bext
    (0x1, 0x34), // binv
    (0x1, 0x14), // bset
    (0x2, 0x10), // sh1add
    (0x4, 0x10), // sh2add
    (0x6, 0x10), // sh3add
];

/// RV32F's opcodes. `lp-riscv-emu` decodes these itself; this chip has no FPU
/// and the privileged hart rejects them before they reach the executors, but
/// the user-mode path this oracle uses still answers them.
const FLOAT_OPCODES: &[u32] = &[0x07, 0x27, 0x43, 0x47, 0x4b, 0x4f, 0x53];

/// Why `lp-emu-jit` refuses a word the interpreter accepts, or [`None`] if
/// there is no good reason and the disagreement is real.
fn refusal_family(word: u32) -> Option<&'static str> {
    if word & 0b11 != 0b11 {
        let i = word & 0xffff;
        if i == 0x9002 {
            return Some(F_C_EBREAK);
        }
        let q = i & 3;
        let f3 = (i >> 13) & 7;
        let shamt5 = (i >> 12) & 1 == 1;
        // c.srli / c.srai (quadrant 1, funct3 100, funct2 00 or 01) and
        // c.slli (quadrant 2, funct3 000).
        let is_rvc_shift = (q == 1 && f3 == 4 && (i >> 10) & 3 <= 1) || (q == 2 && f3 == 0);
        if is_rvc_shift && shamt5 {
            return Some(F_C_SHAMT);
        }
        return None;
    }

    let op = word & 0x7f;
    let f3 = (word >> 12) & 7;
    let f7 = word >> 25;
    match op {
        0x73 => Some(F_SYSTEM),
        0x2f => Some(F_ATOMIC),
        // funct3 0 is the plain `fence`, which `lp-emu-jit` accepts, so it
        // never reaches here.
        0x0f if f3 != 0 => Some(F_FENCE_I),
        0x67 if f3 != 0 => Some(F_JALR_FUNCT3),
        0x33 if BITMANIP_R.contains(&(f3, f7)) => Some(F_BITMANIP_R),
        // The base ISA defines exactly funct7 0 for `slli`/`srli` and 0x20 for
        // `srai`; `lp-emu-jit` accepts those and nothing else, so reaching
        // here means the funct7 is not a base one.
        0x13 if f3 == 1 || f3 == 5 => Some(F_SHIFT_FUNCT7),
        _ if FLOAT_OPCODES.contains(&op) => Some(F_FLOAT),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// The comparison.
// ---------------------------------------------------------------------------

/// What `lp-emu-jit` and the interpreter each say about one word, and whether
/// that is allowed. `Err` carries the whole story, because a disagreement is a
/// finding someone has to act on.
fn check(word: u32) -> Result<Option<&'static str>, String> {
    let mine = lp_emu_jit::decode::decode(word).map(|d| (d.width, d.class));
    let theirs = class_of(word);
    match (mine, theirs) {
        (Some(a), Some(b)) if a == b => Ok(None),
        (Some((w, c)), Some((ow, oc))) => Err(format!(
            "{word:#010x}: lp-emu-jit says (width {w}, {c:?}) but the interpreter's \
             executors say (width {ow}, {oc:?})"
        )),
        (Some((w, c)), None) => Err(format!(
            "{word:#010x}: lp-emu-jit accepts it as (width {w}, {c:?}) but the \
             interpreter's executors refuse it — translated code would run an \
             instruction the interpreter calls illegal"
        )),
        (None, Some((ow, oc))) => match refusal_family(word) {
            Some(family) => Ok(Some(family)),
            None => Err(format!(
                "{word:#010x}: lp-emu-jit refuses it and the interpreter accepts it as \
                 (width {ow}, {oc:?}), and it belongs to no named refusal family. \
                 Either lp-emu-jit is missing a base-ISA encoding, or this test needs a \
                 new family with a reason attached — decide which, do not widen the \
                 assertion"
            )),
        },
        (None, None) => Ok(None),
    }
}

/// Runs [`check`] over a stream of words, accumulating the families reached and
/// the first few failures. Reporting several failures rather than the first is
/// deliberate: one bad bit in an immediate lights up a whole funct3 arm, and
/// the shape of the set is what says which bit.
#[derive(Default)]
struct Tally {
    checked: u64,
    distinct: HashSet<u32>,
    families: HashSet<&'static str>,
    failures: Vec<String>,
}

impl Tally {
    fn feed(&mut self, word: u32) {
        self.checked += 1;
        match check(word) {
            Ok(Some(family)) => {
                self.families.insert(family);
            }
            Ok(None) => {}
            Err(why) => {
                if self.failures.len() < 20 {
                    self.failures.push(why);
                }
            }
        }
    }

    fn feed_distinct(&mut self, word: u32) {
        if self.distinct.insert(word) {
            self.feed(word);
        }
    }

    fn assert_clean(&self, what: &str) {
        assert!(
            self.failures.is_empty(),
            "{what}: {} of {} words disagreed. First {}:\n{}",
            self.failures.len(),
            self.checked,
            self.failures.len(),
            self.failures.join("\n")
        );
    }
}

// ---------------------------------------------------------------------------
// Half one: the exhaustive sweep.
// ---------------------------------------------------------------------------

/// Register triples chosen for the fields that change a *class*, not just a
/// value: `rd == 0` splits `jal` into call and tail and `jalr` into call and
/// jump, and `rs1 == 1` with a zero displacement is what makes a `jalr` a
/// return. `x10` stands in for "an ordinary register".
const REG_TRIPLES: &[(u32, u32, u32)] = &[
    (0, 0, 0),
    (1, 0, 0),
    (0, 1, 0),
    (1, 1, 0),
    (0, 2, 0),
    (2, 2, 2),
    (1, 10, 0),
    (10, 11, 12),
];

/// The whole encoding space the corpus cannot reach.
///
/// **Every** 16-bit word — all 65,536 of them, which is the entire compressed
/// space including every reserved and hint encoding — plus, for 32-bit words,
/// every one of the 32 base opcodes crossed with every funct3 and every funct7,
/// eight register triples deep.
///
/// The 32-bit half is bounded rather than exhaustive because 2^32 decodes is
/// not a unit test. What the bound gives up is *operand* coverage, and operands
/// reach the answer through exactly three doors — `rd == 0`, `rs1 == 1`, and a
/// zero displacement — all of which [`REG_TRIPLES`] opens. What it keeps is
/// complete coverage of the opcode/funct3/funct7 space, which is where a
/// decoder actually goes wrong.
#[test]
fn the_encoding_space_sweep_agrees() {
    let mut tally = Tally::default();

    for i in 0..=0xffffu32 {
        tally.feed(i);
    }
    let compressed = tally.checked;
    assert_eq!(compressed, 65_536);

    for opcode in (0u32..128).filter(|o| o & 0b11 == 0b11) {
        for f3 in 0u32..8 {
            for f7 in 0u32..128 {
                for &(rd, rs1, rs2) in REG_TRIPLES {
                    let word = opcode | rd << 7 | f3 << 12 | rs1 << 15 | rs2 << 20 | f7 << 25;
                    tally.feed(word);
                }
            }
        }
    }
    let full = tally.checked - compressed;
    assert_eq!(full, 32 * 8 * 128 * REG_TRIPLES.len() as u64);

    tally.assert_clean("the encoding sweep");

    let unreached: Vec<&&str> = FAMILIES
        .iter()
        .filter(|f| !tally.families.contains(**f))
        .collect();
    assert!(
        unreached.is_empty(),
        "the sweep never reached these refusal families, so they are excuses \
         rather than facts — delete them or fix the sweep: {unreached:#?}"
    );

    eprintln!(
        "encoding sweep: {compressed} compressed words (the whole 16-bit space) and \
         {full} full words ({} opcodes x 8 funct3 x 128 funct7 x {} register triples); \
         {} named refusal families all reached",
        32,
        REG_TRIPLES.len(),
        tally.families.len()
    );
}

// ---------------------------------------------------------------------------
// Half two: the corpus.
// ---------------------------------------------------------------------------

/// One of the pinned reference images `scripts/emu/bench-c6.sh` and
/// `scripts/emu/oracle-sweep.sh` measure against — the same rows, the same
/// pins.
///
/// The render images sit at their own commit and take no cherry-pick:
/// `bench_render_loop` does not exist at the `d6cfaa205` the other three
/// reference images are pinned at, and the spike feature is already in this
/// tree.
struct RenderImage {
    slug: &'static str,
    env: &'static str,
    features: &'static str,
}

const RENDER_COMMIT: &str = "8ffc4b325";

const RENDER_BASIC: RenderImage = RenderImage {
    slug: "render-basic",
    env: "LP_EMU_C6_REF_RENDER_BASIC",
    features: "esp32c6,server,radio,spike_uart0_link,memory_fs,bench_render_loop",
};

const RENDER_ROCAILLE: RenderImage = RenderImage {
    slug: "render-rocaille",
    env: "LP_EMU_C6_REF_RENDER_ROCAILLE",
    features: "esp32c6,server,radio,spike_uart0_link,memory_fs,bench_project_rocaille",
};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("lp-emu/lp-emu-jit sits two levels below the workspace root")
        .to_path_buf()
}

/// The ELF for `image`: an environment variable, then the conventional path,
/// then a build.
///
/// **Never a skip.** This test is `#[ignore]`d precisely so that a plain
/// `cargo test` cannot trigger a cross-target firmware build; running it at all
/// therefore means someone asked for it, and an image that cannot be had is a
/// failed test rather than a quiet pass. `just test-emu-jit` is what sets the
/// environment.
fn resolve(image: &RenderImage) -> PathBuf {
    if let Ok(from_env) = std::env::var(image.env) {
        let path = PathBuf::from(&from_env);
        assert!(
            path.is_file(),
            "{} points at {from_env}, which is not a file",
            image.env
        );
        return path;
    }

    let root = workspace_root();
    let path = root
        .join("target/emu-ref")
        .join(format!("{RENDER_COMMIT}-{}", image.slug))
        .join("fw-esp32c6");
    if path.is_file() {
        return path;
    }

    assert_eq!(
        std::env::var("LP_EMU_BUILD_FW").as_deref(),
        Ok("1"),
        "no {} reference image at {}. Run `just test-emu-jit`, which builds both \
         render images and runs this test; or point {} at an already-built ELF; or \
         set LP_EMU_BUILD_FW=1 to let this test build it. It is not built \
         automatically, and it is not skipped either.",
        image.slug,
        path.display(),
        image.env,
    );

    let status = Command::new(root.join("scripts/emu/build-reference-image.sh"))
        .arg(image.features)
        .arg(RENDER_COMMIT)
        .arg("none")
        .current_dir(&root)
        .status()
        .unwrap_or_else(|e| panic!("running build-reference-image.sh: {e}"));
    assert!(
        status.success(),
        "build-reference-image.sh {} {RENDER_COMMIT} none failed: {status}",
        image.features
    );
    assert!(
        path.is_file(),
        "build-reference-image.sh reported success but {} is missing",
        path.display()
    );
    path
}

/// The file bytes of every executable `PT_LOAD` segment, in order.
///
/// A hand-rolled ELF32 program-header walk rather than a dependency: this
/// crate's whole licence posture (JD2) is that it adds no workspace-local AGPL
/// edge, and `lp-riscv-elf` — the crate that would otherwise do this — is on
/// the far side of the fence. Sixty lines of little-endian header reading is a
/// cheaper price than an allowlist entry.
fn executable_segments(elf: &[u8]) -> Vec<(u32, &[u8])> {
    const PT_LOAD: u32 = 1;
    const PF_X: u32 = 1;

    let u16at = |o: usize| u16::from_le_bytes([elf[o], elf[o + 1]]);
    let u32at = |o: usize| u32::from_le_bytes([elf[o], elf[o + 1], elf[o + 2], elf[o + 3]]);

    assert!(elf.len() > 52, "not an ELF: {} bytes", elf.len());
    assert_eq!(&elf[0..4], b"\x7fELF", "not an ELF");
    assert_eq!(elf[4], 1, "expected a 32-bit ELF");
    assert_eq!(elf[5], 1, "expected a little-endian ELF");

    let phoff = u32at(0x1c) as usize;
    let phentsize = u16at(0x2a) as usize;
    let phnum = u16at(0x2c) as usize;

    let mut out = Vec::new();
    for n in 0..phnum {
        let ph = phoff + n * phentsize;
        if u32at(ph) != PT_LOAD || u32at(ph + 0x18) & PF_X == 0 {
            continue;
        }
        let offset = u32at(ph + 0x04) as usize;
        let vaddr = u32at(ph + 0x08);
        let filesz = u32at(ph + 0x10) as usize;
        if filesz == 0 {
            continue;
        }
        out.push((vaddr, &elf[offset..offset + filesz]));
    }
    assert!(!out.is_empty(), "no executable PT_LOAD segments");
    out
}

/// Decode every 16- and 32-bit word of both render images' executable regions
/// with both decoders.
///
/// Every 2-byte-aligned position is fed, because half the image's instructions
/// start at 2 mod 4 (the spike measured 48.99 % of block starts there), so a
/// 4-byte walk would miss half the corpus. Each position's word is handed to
/// both decoders unchanged and each decides the width itself — which is the
/// thing being compared.
///
/// `#[ignore]` and not a skip: see [`resolve`].
#[test]
#[ignore = "needs the pinned render reference images; run `just test-emu-jit`"]
fn the_render_images_agree() {
    let mut report = String::new();
    for image in [&RENDER_BASIC, &RENDER_ROCAILLE] {
        let path = resolve(image);
        let bytes =
            std::fs::read(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));

        let mut tally = Tally::default();
        let mut positions = 0u64;
        for (_vaddr, text) in executable_segments(&bytes) {
            let mut at = 0;
            while at + 2 <= text.len() {
                let word = if at + 4 <= text.len() {
                    u32::from_le_bytes([text[at], text[at + 1], text[at + 2], text[at + 3]])
                } else {
                    u32::from(u16::from_le_bytes([text[at], text[at + 1]]))
                };
                positions += 1;
                tally.feed_distinct(word);
                at += 2;
            }
        }

        tally.assert_clean(image.slug);
        assert!(
            tally.distinct.len() > 100_000,
            "{}: only {} distinct words came out of {}; that is not an image's worth \
             of code, and a corpus test that decodes nothing passes for the wrong \
             reason",
            image.slug,
            tally.distinct.len(),
            path.display()
        );

        writeln!(
            report,
            "{}: {positions} half-word positions, {} distinct words, \
             {} reached refusal families",
            image.slug,
            tally.distinct.len(),
            tally.families.len()
        )
        .expect("writing to a String");
    }
    eprint!("{report}");
}
