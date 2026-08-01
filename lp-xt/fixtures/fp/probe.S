// FP capability probe payloads for the ESP32-S3 desk session (M6 P1).
//
// NOT YET RUN ON HARDWARE. These are built and checked in so the desk session is
// a flashing window, not an authoring session. Every verdict in
// `lp-xt-inst/src/fp.rs`'s subset table still reads NOT PROBED.
//
// Each probe is its own section so a single payload can be extracted with
//   objcopy -O binary -j .probe.<name> obj/probe.o obj/probe_<name>.bin
// and handed to the experiment repo's runner as a raw `Vec<u8>` (see
// `xt-testkit/tests/common/mod.rs::stub42_payload` for the shape). ./probes.sh
// does the extraction and prints the whole table.
//
// Runner contract: windowed entry, result in a2, `retw` to return. The harness
// normalizes a hardware crash to `TrapKind::Exception` with the EXCCAUSE, so the
// verdict is mechanical:
//
//   returns the staged value   -> instruction PRESENT and CP0 armed
//   Exception, EXCCAUSE 32     -> instruction present, CPENABLE not armed
//   Exception, EXCCAUSE 0      -> instruction ABSENT (illegal)
//
// Expect one board reboot per faulting probe. That is the design.
//
// Mnemonics only — no hand-written encodings. ./build.sh assembles this file.

// Arm coprocessor 0 and stage two zero-valued FP registers. Every probe except
// `unarmed` starts here.
.macro ARM_CP0
	entry a1, 32
	movi  a2, 1
	wsr.cpenable a2
	isync
	movi  a3, 0
	wfr   f0, a3
	wfr   f1, a3
.endm

// A probe that only has to not fault: run \insn, return \id.
.macro PRESENCE id
	movi  a2, \id
	retw
.endm

// --- the ordering finding: is CPENABLE armed out of reset? ------------------
// This one deliberately omits the arm. If it faults with EXCCAUSE 32 and the
// armed probes do not, the arming requirement is confirmed. If it *succeeds*,
// something else already armed CP0 — that is itself the finding.
	.section .probe.unarmed, "ax", @progbits
	.global probe_unarmed
probe_unarmed:
	entry a1, 32
	movi  a3, 0
	wfr   f0, a3
	movi  a2, 100
	retw

// --- arithmetic --------------------------------------------------------------
	.section .probe.add_s, "ax", @progbits
probe_add_s:
	ARM_CP0
	add.s f2, f0, f1
	PRESENCE 1

	.section .probe.mul_s, "ax", @progbits
probe_mul_s:
	ARM_CP0
	mul.s f2, f0, f1
	PRESENCE 2

	.section .probe.madd_s, "ax", @progbits
probe_madd_s:
	ARM_CP0
	madd.s f2, f0, f1
	PRESENCE 3

	.section .probe.abs_s, "ax", @progbits
probe_abs_s:
	ARM_CP0
	abs.s f2, f0
	PRESENCE 4

	.section .probe.mov_s, "ax", @progbits
probe_mov_s:
	ARM_CP0
	mov.s f2, f0
	PRESENCE 5

// --- transfers: these return a real value, not a staged id -------------------
// wfr/rfr must be bit-transparent. Staging 0x2A through the FR file and back
// returns 42, so a wrong answer is as loud as a fault.
	.section .probe.rfr_wfr, "ax", @progbits
probe_rfr_wfr:
	entry a1, 32
	movi  a2, 1
	wsr.cpenable a2
	isync
	movi  a3, 42
	wfr   f0, a3
	rfr   a2, f0
	retw

// --- compare + boolean readback ---------------------------------------------
// Three independent readback paths for one compare. If the Boolean option is
// absent, all three fault and M6 has a much bigger problem than a missing
// divide (see the plan's escalation A1).
	.section .probe.oeq_bf, "ax", @progbits
probe_oeq_bf:
	ARM_CP0
	oeq.s b0, f0, f1      // 0.0 == 0.0 -> b0 set
	movi  a2, 0
	bf    b0, 1f
	movi  a2, 1
1:
	retw

	.section .probe.oeq_movt, "ax", @progbits
probe_oeq_movt:
	ARM_CP0
	oeq.s b0, f0, f1
	movi  a2, 0
	movi  a4, 1
	movt  a2, a4, b0
	retw

	.section .probe.oeq_rsr_br, "ax", @progbits
probe_oeq_rsr_br:
	ARM_CP0
	oeq.s b0, f0, f1
	rsr.br a2             // the whole boolean file, as a bitmask
	retw

	.section .probe.movf_s, "ax", @progbits
probe_movf_s:
	ARM_CP0
	oeq.s b0, f0, f1
	movf.s f2, f0, b0
	PRESENCE 6

	.section .probe.moveqz_s, "ax", @progbits
probe_moveqz_s:
	ARM_CP0
	movi  a4, 0
	moveqz.s f2, f0, a4
	PRESENCE 7

// --- conversion --------------------------------------------------------------
// float.s of 7 with scale 0, converted straight back by trunc.s, must be 7.
	.section .probe.float_trunc, "ax", @progbits
probe_float_trunc:
	entry a1, 32
	movi  a2, 1
	wsr.cpenable a2
	isync
	movi  a3, 7
	float.s f0, a3, 0
	trunc.s a2, f0, 0
	retw

// --- load / store ------------------------------------------------------------
// Round-trip 42 through memory: AR -> FR -> stack -> FR -> AR. a1 is the frame
// pointer the ENTRY just established, so [a1+0] is safe scratch.
	.section .probe.lsi_ssi, "ax", @progbits
probe_lsi_ssi:
	entry a1, 32
	movi  a2, 1
	wsr.cpenable a2
	isync
	movi  a3, 42
	wfr   f0, a3
	ssi   f0, a1, 0
	lsi   f1, a1, 0
	rfr   a2, f1
	retw

	.section .probe.lsx_ssx, "ax", @progbits
probe_lsx_ssx:
	entry a1, 32
	movi  a2, 1
	wsr.cpenable a2
	isync
	movi  a3, 42
	wfr   f0, a3
	movi  a4, 0
	ssx   f0, a1, a4
	lsx   f1, a1, a4
	rfr   a2, f1
	retw

// --- the div/sqrt helper family ---------------------------------------------
// The whole point of the probe. If these are ABSENT, M7's division lowering is a
// different design (escalation A1: stop and report, do not pick a replacement).
	.section .probe.div0_s, "ax", @progbits
probe_div0_s:
	ARM_CP0
	div0.s f2, f0
	PRESENCE 10

	.section .probe.nexp01_s, "ax", @progbits
probe_nexp01_s:
	ARM_CP0
	nexp01.s f2, f0
	PRESENCE 11

	.section .probe.const_s, "ax", @progbits
probe_const_s:
	ARM_CP0
	const.s f2, 1
	PRESENCE 12

	.section .probe.maddn_s, "ax", @progbits
probe_maddn_s:
	ARM_CP0
	maddn.s f2, f0, f1
	PRESENCE 13

	.section .probe.mkdadj_s, "ax", @progbits
probe_mkdadj_s:
	ARM_CP0
	mkdadj.s f2, f0
	PRESENCE 14

	.section .probe.divn_s, "ax", @progbits
probe_divn_s:
	ARM_CP0
	divn.s f2, f0, f1
	PRESENCE 15

	.section .probe.recip0_s, "ax", @progbits
probe_recip0_s:
	ARM_CP0
	recip0.s f2, f0
	PRESENCE 16

	.section .probe.rsqrt0_s, "ax", @progbits
probe_rsqrt0_s:
	ARM_CP0
	rsqrt0.s f2, f0
	PRESENCE 17

	.section .probe.sqrt0_s, "ax", @progbits
probe_sqrt0_s:
	ARM_CP0
	sqrt0.s f2, f0
	PRESENCE 18

// --- FP control / status registers ------------------------------------------
// Reads the reset value of FCR (rounding mode) straight back. Whatever comes
// out is a measurement, and P3's D6 decision depends on it.
	.section .probe.rur_fcr, "ax", @progbits
probe_rur_fcr:
	entry a1, 32
	movi  a2, 1
	wsr.cpenable a2
	isync
	rur.fcr a2
	retw

	.section .probe.rur_fsr, "ax", @progbits
probe_rur_fsr:
	entry a1, 32
	movi  a2, 1
	wsr.cpenable a2
	isync
	rur.fsr a2
	retw
