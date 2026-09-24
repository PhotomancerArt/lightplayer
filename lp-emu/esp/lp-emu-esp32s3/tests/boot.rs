//! The machine boots: the ROM executes, the seeded frame survives, slot 1 is
//! held, and a direct load of the shipped image reaches its first strict stop
//! inside the MMIO window.
//!
//! The tests that need a built `fw-esp32s3` are `#[ignore]`d and run by
//! `just test-emu-esp32s3-boot`, which builds the image and names the file it
//! built ([`lp_emu_esp32s3::test_support`] says why a test must never build
//! one itself).

use lp_emu_esp32s3::machine::{
    AppSource, BootFrame, CORES, CPENABLE_RESET_DEFAULT, CoreOneControl, Esp32S3Builder, Outcome,
    StopCondition, UsbHost,
};
use lp_emu_esp32s3::{memmap, rom, test_support};
use lp_xt_inst::{AluRrr, BrZ, CallOp, CallxOp, Inst, Reg, encode};

/// Where these tests put their code: inside the SRAM1 **I-bus** view, which
/// is where the firmware's own `.vectors` and `.rwtext` live and where every
/// JIT'd shader is fetched from.
///
/// ⚠️ **Not a stylistic choice — a windowed call cannot cross a 1 GiB
/// region.** `retw` reconstructs the return address as
/// `PC[31:30] ‖ a0[29:0]`, so the caller and callee must share the top two
/// address bits. Code placed at `0x3FC9_0000` (the D-bus view) calling
/// `memcpy` at `0x4005_6F44` returns to `0x7FC9_0003` and dies in the ROM's
/// debug vector with an undecodable word — which is exactly what the first
/// version of this file did, and it looked like a machine bug. The
/// firmware's own IRAM is at `0x4037_xxxx` for the same reason the mask ROM
/// is at `0x4000_xxxx`: one region, so the ROM is callable.
///
/// It also means these tests **execute through the RAM alias**, which is the
/// property M6 P02 exists for: the bytes are written at `0x4037_9000`, land
/// in the one store at `0x3FC8_9000`, and are fetched back through the I-bus
/// door.
const CODE: u32 = 0x4037_9000;
/// [`CODE`]'s canonical (D-bus) address — `CODE - 0x6F_0000`.
const CODE_DBUS: u32 = CODE - memmap::SRAM1_IBUS_OFFSET;
/// The buffers, on the data side where buffers live.
const SRC: u32 = 0x3FC9_4000;
const DST: u32 = 0x3FC9_5000;

fn a(n: u8) -> Reg {
    Reg::new(n)
}

fn brk() -> Inst {
    Inst::Break(1, 15)
}

/// `call8`'s signed word offset from the instruction at `call_pc` to
/// `target`, per the ISA's own formula (and `lp-xt-emu`'s window tests, which
/// use the identical line).
fn call_offset(call_pc: u32, target: u32) -> i32 {
    (target as i32 - ((call_pc & !3) as i32 + 4)) >> 2
}

fn assemble(program: &[Inst]) -> Vec<u8> {
    program.iter().flat_map(encode).collect()
}

/// **The break bytes are the assembler's, not a transcription.**
///
/// `rom::BREAK_1_15_BYTES` is a `[u8; 3]` in a source file, which is exactly
/// the shape M0's `rev8` mistake took. This is the check that it is what
/// `lp_xt_inst::encode` produces.
#[test]
fn the_hook_patch_is_what_the_assembler_produces() {
    assert_eq!(
        encode(&Inst::Break(1, 15)).as_slice(),
        rom::BREAK_1_15_BYTES.as_slice()
    );
}

/// **The machine executes mask-ROM code, for real, over guest memory.**
///
/// `m6/notes.md` §2.5: `memcpy` alone is 4,769 call sites across 800 caller
/// symbols on this chip, so the ROM is most of the dynamic instruction count
/// and a machine that cannot execute ROM `memcpy` never reaches a
/// peripheral. This calls it the way the firmware does — a windowed `call8`
/// with the arguments in `a10..a12` — and checks the bytes moved.
///
/// No application image is involved: the ROM is loaded in every
/// configuration (PD7), which is the property being exercised.
#[test]
fn the_machine_executes_rom_memcpy_over_guest_memory() {
    let mut machine = Esp32S3Builder::new()
        // Strict, so a stray access during the copy is a stop rather than a
        // silently swallowed zero.
        .strict(true)
        .build()
        .expect("a machine with the vendored ROM and no application");

    let memcpy = machine
        .resolve_symbol("memcpy")
        .expect("the vendored ROM's symbol table has memcpy");
    assert!(
        memcpy >= memmap::ROM_MASK_BASE && memcpy < memmap::ROM_MASK_BASE + memmap::ROM_MASK_LEN,
        "memcpy at {memcpy:#010x} is inside the mask ROM window"
    );

    // A pattern that cannot be confused with a zero-filled region or with
    // whatever the ROM's own data left behind.
    const N: u32 = 64;
    let pattern: Vec<u8> = (0..N).map(|i| (i as u8) ^ 0xA5).collect();
    machine
        .bus_mut()
        .load_image(SRC, &pattern)
        .expect("seeding the source buffer");
    for i in 0..N / 4 {
        assert!(
            machine.poke_word(DST + i * 4, 0),
            "clearing the destination"
        );
    }

    // `callx8 a9; break`, with `a9` holding the target.
    //
    // ⚠️ **`call8` cannot reach the ROM from SRAM1 and the encoder will not
    // say so.** `call8`'s field is an 18-bit signed *word* offset, ±512 KiB;
    // `0x4005_6F44 - 0x3FC9_0004` is 0x3C_6F40, which is 990,160 words and
    // does not fit. The first version of this test used `call8` and the
    // truncated field landed the pc inside the ROM's `.text` at
    // `0x4000_0705`, where the run died on an undecodable word 21
    // instructions in — a wrong answer that looked like a machine bug. The
    // register form has no range, which is why every real cross-section call
    // in the firmware is one.
    //
    // The stub needs no `entry` of its own: `callx8` sets `PS.CALLINC = 2`
    // and the callee's own `entry` performs the rotation, so the caller's
    // `a10..a12` are the callee's `a2..a4` — `memcpy(dest, src, n)` — and the
    // caller's `a9` is where `entry a1, N` writes the callee's new stack
    // pointer, so holding the target there costs nothing.
    let program = assemble(&[Inst::Callx(CallxOp::Callx8, a(9)), brk()]);
    machine
        .bus_mut()
        .load_image(CODE, &program)
        .expect("placing the stub");

    machine
        .seed_boot_state(CODE, BootFrame::rom_pro_stack(machine.rom()))
        .expect("seeding the boot state");
    // The `break` is claimed by the hook table, so the run ends as a machine
    // stop instead of vectoring into the ROM's debug handler — which on this
    // ROM, as on the classic's, ends in an instruction this hart does not
    // decode and would bury the result under an unrelated fault.
    machine
        .break_at_address(CODE + 3)
        .expect("claiming the stub's break");
    machine.harts[0].cpu_mut().set_a(9, memcpy);
    machine.harts[0].cpu_mut().set_a(10, DST);
    machine.harts[0].cpu_mut().set_a(11, SRC);
    machine.harts[0].cpu_mut().set_a(12, N);

    let outcome = machine.run_until(&StopCondition {
        stop_cycle: Some(100_000),
        ..Default::default()
    });

    let copied: Vec<u8> = (0..N)
        .map(|i| {
            let word = machine.peek_word((DST + i) & !3).expect("reading back");
            word.to_le_bytes()[(i & 3) as usize]
        })
        .collect();
    assert!(
        matches!(outcome, Outcome::Breakpoint { .. }),
        "the stub returned to its break with no fault and no strict refusal; \
         got {outcome:?} (first violation {:?})",
        machine.first_strict_violation()
    );
    assert_eq!(
        copied,
        pattern,
        "the ROM's own memcpy moved the bytes ({} instructions)",
        machine.instructions()
    );
    // And the stub that called it was fetched through the SRAM1 I-bus alias:
    // one store, two doors (M6 P02).
    assert_eq!(
        machine.peek_word(CODE_DBUS).expect("the canonical address"),
        machine.peek_word(CODE).expect("the alias address"),
        "the code this run executed from {CODE:#010x} is the one store at \
         {CODE_DBUS:#010x}"
    );
    // And it cost real ROM instructions: the stub is two.
    assert!(
        machine.instructions() > 10,
        "the copy ran ROM code, not just the stub ({} instructions)",
        machine.instructions()
    );
    println!(
        "ROM EXECUTES: memcpy @ {memcpy:#010x} copied {N} B {SRC:#010x} -> {DST:#010x} in {} \
         instructions, {} cycles",
        machine.instructions(),
        machine.cycles()
    );
}

/// **The seeded boot frame survives a real window overflow, handled by the
/// mask ROM's own vectors.**
///
/// This is what [`BootFrame`] exists for. A hart left with `a1 = 0` and no
/// base save area dies on its first spill — `_WindowOverflow8`'s
/// `l32e a0, a1, -12` reads garbage, faults, and the fault's own spill faults
/// again — nowhere near the cause. The frame is seeded so that cannot happen.
///
/// The handlers are not a stand-in: `VECBASE` at reset is
/// [`memmap::ROM_MASK_BASE`] and the vendored ROM's `.WindowVectors.text` is
/// at exactly that address, so the overflows below are serviced by the real
/// mask ROM.
#[test]
fn a_seeded_boot_frame_survives_real_window_overflows_in_the_mask_roms_vectors() {
    let mut machine = Esp32S3Builder::new()
        .strict(true)
        .build()
        .expect("a machine");

    // f(n) = n == 0 ? 0 : n + f(n - 1), windowed `call8` recursion — twenty
    // deep, which wraps the 64-register ring more than once and forces the
    // overflow handlers to run.
    const DEPTH: u32 = 20;
    let f_at = CODE + 0x40;
    let f = [
        Inst::Entry(a(1), 32),
        Inst::BranchZ(BrZ::Beqz, a(2), 11),
        Inst::Addi(a(10), a(2), -1),
        Inst::Call(CallOp::Call8, -3),
        Inst::Rrr(AluRrr::Add, a(2), a(2), a(10)),
        Inst::Nullary(lp_xt_inst::NullaryOp::Retw),
        Inst::Movi(a(2), 0),
        Inst::Nullary(lp_xt_inst::NullaryOp::Retw),
    ];
    machine
        .bus_mut()
        .load_image(f_at, &assemble(&f))
        .expect("placing f");

    // The outermost frame calls f with `call8`, which is what `PS_BOOT`'s
    // `CALLINC(2)` records about how a bootloader reaches an entry point.
    let call_pc = CODE;
    let program = assemble(&[Inst::Call(CallOp::Call8, call_offset(call_pc, f_at)), brk()]);
    machine
        .bus_mut()
        .load_image(CODE, &program)
        .expect("placing the caller");

    machine
        .seed_boot_state(CODE, BootFrame::rom_pro_stack(machine.rom()))
        .expect("seeding the boot state");
    machine
        .break_at_address(CODE + 3)
        .expect("claiming the caller's break");
    machine.harts[0].cpu_mut().set_a(10, DEPTH);

    let outcome = machine.run_until(&StopCondition {
        stop_cycle: Some(1_000_000),
        ..Default::default()
    });

    let want: u32 = (0..=DEPTH).sum();
    assert!(
        matches!(outcome, Outcome::Breakpoint { .. }),
        "twenty windowed frames, spilled and reloaded through the mask ROM's \
         own vectors, with no fault and no strict refusal; got {outcome:?}"
    );
    assert!(machine.first_strict_violation().is_none());
    assert_eq!(
        machine.harts[0].cpu().a(10),
        want,
        "f({DEPTH}) = {want} through the mask ROM's own window handlers (outcome {outcome:?})"
    );
    // The proof that the handlers really ran rather than the ring simply
    // being deep enough: an overflow spills through the base save area, and
    // the word it reads for the next frame's stack pointer is the one the
    // boot frame seeded.
    let frame = machine.boot_frame().expect("a boot frame was seeded");
    assert_eq!(frame.save_area[1], frame.sp);
    assert!(
        machine.instructions() > 200,
        "twenty windowed frames plus the ROM's handlers is not a handful of \
         instructions ({})",
        machine.instructions()
    );
}

/// A hart whose base save area was **not** seeded dies the way
/// [`BootFrame`]'s docs say it does — which is the other half of the claim
/// above, and the reason the four words are not decoration.
///
/// The frame here has a perfectly good `a1`; only the word at `[a1-12]` — the
/// *next* frame's stack pointer, which `_WindowOverflow8`'s
/// `l32e a0, a1, -12` reads — is left at zero instead of at `sp`. That single
/// word is the difference between twenty frames of recursion and a double
/// exception nowhere near its cause.
#[test]
fn without_a_seeded_save_area_the_first_spill_is_a_fault_nowhere_near_its_cause() {
    // ⚠️ **Strict, and that is the point.** Without `--strict-bus` the spill
    // below writes through a null saved stack pointer into `0xFFFF_FFF0`, the
    // bus answers the unmapped write with silence, and the recursion returns
    // the right number anyway. That is exactly the failure mode `BootFrame`
    // describes — nowhere near its cause, and invisible unless something
    // refuses.
    let mut machine = Esp32S3Builder::new()
        .strict(true)
        .build()
        .expect("a machine");
    const DEPTH: u32 = 20;
    let f_at = CODE + 0x40;
    let f = [
        Inst::Entry(a(1), 32),
        Inst::BranchZ(BrZ::Beqz, a(2), 11),
        Inst::Addi(a(10), a(2), -1),
        Inst::Call(CallOp::Call8, -3),
        Inst::Rrr(AluRrr::Add, a(2), a(2), a(10)),
        Inst::Nullary(lp_xt_inst::NullaryOp::Retw),
        Inst::Movi(a(2), 0),
        Inst::Nullary(lp_xt_inst::NullaryOp::Retw),
    ];
    machine
        .bus_mut()
        .load_image(f_at, &assemble(&f))
        .expect("placing f");
    let program = assemble(&[Inst::Call(CallOp::Call8, call_offset(CODE, f_at)), brk()]);
    machine
        .bus_mut()
        .load_image(CODE, &program)
        .expect("placing the caller");

    // The frame a loader that seeded only `a1` would leave: the save area is
    // all zeros, so the saved next-frame stack pointer is null.
    let good = BootFrame::rom_pro_stack(machine.rom());
    assert_eq!(good.save_area[1], good.sp, "the word this test removes");
    machine
        .seed_boot_state(
            CODE,
            BootFrame {
                save_area: [0; 4],
                ..good
            },
        )
        .expect("seeding an unseeded save area");
    machine
        .break_at_address(CODE + 3)
        .expect("claiming the caller's break");
    machine.harts[0].cpu_mut().set_a(10, DEPTH);

    let outcome = machine.run_until(&StopCondition {
        stop_cycle: Some(1_000_000),
        ..Default::default()
    });
    let Outcome::StrictBus { violation } = &outcome else {
        panic!("a null saved stack pointer must be refused, not survived: {outcome:?}");
    };
    assert!(
        violation.address > 0xFFFF_0000,
        "the spill went through the null word at [sp-12]: {violation:?}"
    );
    println!(
        "UNSEEDED: {:?} {:?} at {:#010x} from pc {:#010x} ({}) at cycle {}",
        violation.access,
        violation.width,
        violation.address,
        violation.pc,
        machine
            .symbolize(violation.pc)
            .unwrap_or_else(|| "?".into()),
        violation.cycle
    );
}

/// Two hart slots; slot 1 held, taking no cycles; and the report says what
/// holds it.
#[test]
fn slot_one_is_held_by_the_machine_and_by_the_chips_own_reset_state() {
    let mut machine = Esp32S3Builder::new().build().expect("a machine");
    assert_eq!(machine.harts.len(), CORES);

    // The chip's half, before anything the machine did: the PAC's reset value
    // for `SYSTEM.core_1_control_0` is `0x04`.
    assert_eq!(machine.core_1_control(), CoreOneControl::reset());
    assert_eq!(machine.core_1_control().bits(), 0x04);
    assert!(machine.core_1_control().holds_core1());
    assert_eq!(
        CoreOneControl::from_bits(0x04),
        CoreOneControl::reset(),
        "the word and the fields are the same register"
    );

    assert!(!machine.core_stalled(0), "core 0 runs");
    assert!(machine.core_stalled(1), "core 1 is held");

    let inputs = machine.stall_inputs(1);
    assert!(
        inputs.iter().any(|s| s.starts_with("machine")),
        "the machine holds it: {inputs:?}"
    );
    assert!(
        inputs
            .iter()
            .any(|s| s.contains("core_1_control_0.reseting")),
        "and so does the chip's reset state: {inputs:?}"
    );
    assert!(
        inputs
            .iter()
            .any(|s| s.contains("core_1_control_0.!clkgate_en")),
        "and its clock gate: {inputs:?}"
    );
    assert!(
        machine.stall_inputs(0).is_empty(),
        "core 0 is held by nothing"
    );

    let report = machine.core_report();
    assert!(report[1].contains("held by ["), "{}", report[1]);
    assert!(
        report[1].contains("SYSTEM.core_1_control_0"),
        "{}",
        report[1]
    );
    assert!(report.last().unwrap().contains("quantum:"));

    // A held core takes no cycles at all, whatever the run does.
    let program = assemble(&[Inst::Movi(a(2), 1), brk()]);
    machine
        .bus_mut()
        .load_image(CODE, &program)
        .expect("placing a stub");
    machine
        .seed_boot_state(CODE, BootFrame::rom_pro_stack(machine.rom()))
        .expect("seeding");
    machine.run_until(&StopCondition {
        stop_cycle: Some(10_000),
        ..Default::default()
    });
    assert_eq!(machine.core_instructions(1), 0, "slot 1 retired nothing");
    assert_eq!(
        machine.harts[1].cycle_count(),
        0,
        "and its cycle counter did not move"
    );
    assert_eq!(
        machine.harts[1].pc(),
        memmap::ROM_MASK_BASE + lp_emu_esp32s3::machine::RESET_VECTOR_OFS,
        "it is still at the reset vector"
    );
}

/// The reset state this machine claims, and the one it deliberately does not.
#[test]
fn cpenable_is_a_parameter_whose_default_is_the_isas_reset_and_not_the_classics() {
    let machine = Esp32S3Builder::new().build().expect("a machine");
    assert_eq!(machine.cpenable_reset(), CPENABLE_RESET_DEFAULT);
    assert_eq!(
        CPENABLE_RESET_DEFAULT, 0,
        "the ISA's generic reset. The classic's 0xff is a measurement on \
         CLASSIC silicon, and the S3 firmware's own fpu.rs records its 0xff \
         reading as a fact about that boot chain rather than about the \
         architecture (A4). P09's capture is what changes this."
    );
    for core in 0..CORES {
        assert_eq!(machine.harts[core].cpu().cpenable, CPENABLE_RESET_DEFAULT);
    }

    // And it really is a parameter, so P09 changes one line.
    let armed = Esp32S3Builder::new()
        .cpenable_reset(0xff)
        .build()
        .expect("a machine");
    assert_eq!(armed.harts[0].cpu().cpenable, 0xff);
    assert_eq!(armed.harts[1].cpu().cpenable, 0xff);
}

/// The two cores' `PRID`s reach the harts, and differ in the bit esp-hal
/// reads.
#[test]
fn each_slot_answers_its_own_prid() {
    let machine = Esp32S3Builder::new().build().expect("a machine");
    assert_eq!(
        lp_emu_esp32s3::machine::core_config(0).prid,
        memmap::PRID_CORE0
    );
    assert_eq!(
        lp_emu_esp32s3::machine::core_config(1).prid,
        memmap::PRID_CORE1
    );
    assert_eq!(machine.harts.len(), CORES);
}

// ---------------------------------------------------------------------------
// The shipped image. `#[ignore]`d — `just test-emu-esp32s3-boot` builds it.
// ---------------------------------------------------------------------------

/// A trace sink a test can read back.
#[derive(Clone, Default)]
struct SharedSink(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for SharedSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl SharedSink {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

/// **The deliverable of M6 P05 at the machine level**: a direct load of the
/// shipped image runs, executes mask-ROM code, gets past `esp_hal::init`,
/// and **crosses the console** — where P04 stopped — with **no strict stop
/// at all**.
///
/// P03 pinned the *shape* of its stop (inside the MMIO window, from a ROM
/// pc); P04 pinned the block (`USB_DEVICE`, made by `esp_println`'s writer).
/// P05 models that block, so there is nothing left for a strict run of the
/// pre-console set to refuse: the run reaches its deadline with
/// `unmapped == 0`. What the console *said* is `tests/boot_idle.rs`'s.
///
/// ⚠️ **The run does not finish booting, and that is P06's, not a defect.**
/// It ends spinning on `SPI1.cmd` bit 28 (`usr`) — a flash read that no
/// hardware here will complete — which is exactly the stop
/// `crate::periph::accept::spi1`'s own doc predicted: *"a flash read sets
/// `cmd.usr` and spins until hardware clears it, and a block that remembers
/// holds it set forever. That spin is P06's."* Everything the firmware
/// prints **after** that spin — the `lpfs` mount failure, the hardware
/// manifest, `[INIT] fw-esp32 initialized, starting server loop` — is
/// therefore P06's reading, not this phase's.
///
/// That the mask ROM really executed is asserted separately, off the trace:
/// the first `SENSITIVE` access is made from a mask-ROM pc.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF; run through `just test-emu-esp32s3-boot`"]
fn the_shipped_image_gets_past_esp_hal_init_and_crosses_the_console() {
    let Ok(elf) = test_support::fw_esp32s3_image() else {
        test_support::skip_notice(
            "the_shipped_image_gets_past_esp_hal_init_and_crosses_the_console",
            "no image",
        );
        return;
    };
    let sink = SharedSink::default();
    let mut machine = Esp32S3Builder::new()
        .app(AppSource::Path(elf))
        .strict(true)
        .usb_host(UsbHost::Attached { draining: true })
        .trace(Box::new(sink.clone()), vec!["SENSITIVE".into()])
        .build()
        .expect("the shipped image direct-loads");

    // The image's own segments went somewhere, including the two that are
    // interesting on this chip: an executable one through the SRAM1 I-bus
    // alias, and `.rtc_fast.persistent`, whose vaddr and paddr differ.
    let segments = machine.app_segments();
    assert!(!segments.is_empty());
    let vectors = segments
        .iter()
        .find(|s| s.vaddr == memmap::VECTORS_BASE)
        .expect("the app's .vectors at the I-bus base");
    assert_eq!(
        vectors.regions,
        vec!["sram1-dbus"],
        "placed through the alias, and the region it lives in is the canonical one"
    );
    let rtc = segments
        .iter()
        .find(|s| s.vaddr == memmap::RTC_FAST_BASE)
        .expect("the app's .rtc_fast.persistent");
    assert!(
        rtc.relocated(),
        "its paddr is in the DROM window; placing by paddr would put \
         lp_recovery's ledger in flash"
    );

    let outcome = machine.run_until(&StopCondition {
        stop_cycle: Some(300_000 * memmap::CYCLES_PER_US),
        ..Default::default()
    });

    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "nothing the pre-console set or the console can refuse is left: {outcome:?}"
    );
    assert_eq!(outcome.exit_code(), 0, "the cross-machine contract");
    assert!(
        machine.first_strict_violation().is_none(),
        "P04's stop was the console and P05 models it"
    );
    assert_eq!(
        machine.bus().unmapped_reads() + machine.bus().unmapped_writes(),
        0,
        "zero unmapped accesses: every block this boot reaches answers"
    );
    // The console said something, and it reached a host rather than dying in
    // a committed endpoint nobody drained.
    assert!(
        machine.usb_sj().starts_with(b"[INIT] fw-esp32s3 boot\n"),
        "the first line out of the link"
    );
    // A draining host took it all. From P06 until 2026-09-23 the server
    // loop's first framed write — the hello — lost one 64-byte packet here:
    // a stale `serial_in_empty` woke esp-hal's write future early (see
    // `docs/defects/2026-09-13-the-s3-link-drops-the-io-tasks-next-chunk-on-a-stale-serial-in-empty.md`,
    // still open). The io_task's own `int_clr` of that bit now lands after
    // the `[INIT]` chain's last packet drains rather than 140 cycles before
    // it, so the hello arrives whole; `tests/boot_idle.rs`'s module docs
    // have the trace. A packet merely tried here means that race is back.
    let tried = machine.usb_sj_tried();
    assert_eq!(
        tried.len(),
        0,
        "nothing merely tried: {:?}",
        String::from_utf8_lossy(&tried)
    );

    // Where P05's run stopped, and where P06's goes: past the flash read —
    // on a **blank** chip, since this machine has no `--flash`. The ROM's
    // driver reads erased bytes, the firmware finds no partition table,
    // says so by name, falls back to its memory filesystem and still
    // reaches the server loop. The flashed chip's reading — the `lpfs`
    // mount — is `tests/boot_idle.rs`'s, on the merged image.
    let pc = machine.harts[0].pc();
    let symbol = machine.symbolize(pc).unwrap_or_else(|| "?".into());
    let text = String::from_utf8_lossy(&machine.usb_sj()).into_owned();
    assert!(
        text.contains("[ERROR] no `lpfs` partition in the flashed table"),
        "P06: the flash read `esp_storage` spun on is served, and a blank chip has no \
         table:\n{text}"
    );
    assert!(text.contains("using memory FS"), "{text}");
    assert!(
        text.contains("[INIT] fw-esp32 initialized, starting server loop"),
        "and the boot goes on to the server loop regardless:\n{text}"
    );
    let census = machine.flash().lock().expect("flash").command_census();
    assert!(
        census.reads > 0 && census.programs == 0 && census.sector_erases == 0,
        "a blank chip is read and never written by this boot: {census}"
    );
    println!(
        "P05's stop was SPI1.cmd bit 28 (`usr`); P06 serves it ({census}), and the run ends \
         at {pc:#010x} ({symbol}) past the mount, on the memory FS"
    );
    // The mask ROM really executed: the first SENSITIVE access — P03's own
    // first stop — is made from a mask-ROM pc.
    let trace = sink.text();
    let first = trace
        .lines()
        .find(|l| l.contains("SENSITIVE+0x004"))
        .expect("the ROM's Cache_Occupy_ICache_MEMORY reads cache_dataarray_connect_1");
    let pc = first
        .split_whitespace()
        .find_map(|w| w.strip_prefix("pc=0x"))
        .and_then(|h| u32::from_str_radix(h, 16).ok())
        .expect("a pc on the trace line");
    assert!(
        (memmap::ROM_MASK_BASE..memmap::ROM_MASK_BASE + memmap::ROM_MASK_LEN).contains(&pc),
        "made by mask-ROM code — the ROM really executes on this machine, which is the \
         property 4,769 memcpy call sites make load-bearing: {first}"
    );
    println!(
        "PAST esp_hal::init AND PAST THE CONSOLE: {} bytes reached the host in {} us, \
         unmapped=0, no strict stop; first ROM MMIO: {first}",
        machine.usb_sj().len(),
        machine.cycles() / memmap::CYCLES_PER_US,
    );
    for line in machine.core_report() {
        println!("  {line}");
    }
}

/// The image loads and the two hart slots are where a run report says they
/// are, with no strict bus — the scouting run.
///
/// ⚠️ **It reaches nothing unmapped any more**, and that changed with P05:
/// P04's version asserted `unmapped_reads() > 0` because the console was the
/// one block nothing modelled. With the console modelled, twenty emulated
/// milliseconds of this image touch only blocks that answer — so the
/// assertion is inverted, deliberately, and a future unmapped read here is a
/// block a later phase has to name.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF; run through `just test-emu-esp32s3-boot`"]
fn a_non_strict_direct_load_now_reaches_nothing_the_machine_does_not_model() {
    let Ok(elf) = test_support::fw_esp32s3_image() else {
        test_support::skip_notice(
            "a_non_strict_direct_load_now_reaches_nothing_the_machine_does_not_model",
            "no image",
        );
        return;
    };
    let mut machine = Esp32S3Builder::new()
        .app(AppSource::Path(elf))
        .usb_host(UsbHost::Attached { draining: true })
        .build()
        .expect("the shipped image direct-loads");

    let outcome = machine.run_until(&StopCondition {
        stop_cycle: Some(20_000 * memmap::CYCLES_PER_US),
        ..Default::default()
    });
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "got {outcome:?}"
    );
    assert_eq!(
        machine.bus().unmapped_reads() + machine.bus().unmapped_writes(),
        0,
        "every block twenty milliseconds of this image touches now answers"
    );
    assert_eq!(machine.core_instructions(1), 0, "slot 1 still ran nothing");
    println!(
        "SCOUTING RUN: {} instructions, pc {:#010x} ({}), {} console bytes, {} unmapped reads",
        machine.instructions(),
        machine.harts[0].pc(),
        machine
            .symbolize(machine.harts[0].pc())
            .unwrap_or_else(|| "?".into()),
        machine.usb_sj().len(),
        machine.bus().unmapped_reads(),
    );
}
