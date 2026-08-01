// Every instruction in M6's normative FP / Boolean / special-register subset,
// once per opcode plus its operand-field corners. This file exists to be
// assembled and fed to `lp-xt-inst`'s objdiff rig: objdump disassembles it, this
// crate disassembles the same bytes, and the two must agree instruction for
// instruction.
//
// Keep it exhaustive. An instruction that is not here is not covered by the
// mechanical oracle, whatever the unit tests say.
//
// Assembled by ./build.sh with xtensa-esp32s3-elf-as. Do not hand-edit encodings
// into this file — it holds mnemonics only, by design.

	.text
	.global fp_subset
fp_subset:

	// --- FP0: three-FR arithmetic ---
	add.s   f0, f1, f2
	add.s   f15, f14, f13
	sub.s   f0, f1, f2
	sub.s   f7, f8, f9
	mul.s   f0, f1, f2
	mul.s   f1, f2, f3
	madd.s  f0, f1, f2
	madd.s  f10, f11, f12
	msub.s  f0, f1, f2
	msub.s  f4, f5, f6
	maddn.s f0, f1, f2
	maddn.s f13, f2, f1
	divn.s  f0, f1, f2
	divn.s  f9, f8, f7

	// --- FP1: two-FR unary, transfers, and the constant generator ---
	mov.s    f0, f1
	mov.s    f15, f14
	abs.s    f0, f1
	abs.s    f15, f14
	neg.s    f0, f1
	neg.s    f15, f14
	div0.s   f0, f1
	div0.s   f15, f14
	recip0.s f0, f1
	recip0.s f15, f14
	sqrt0.s  f0, f1
	sqrt0.s  f15, f14
	rsqrt0.s f0, f1
	rsqrt0.s f15, f14
	nexp01.s f0, f1
	nexp01.s f15, f14
	mkdadj.s f0, f1
	mkdadj.s f15, f14
	addexp.s  f0, f1
	addexp.s  f15, f14
	addexpm.s f0, f1
	addexpm.s f15, f14
	const.s  f0, 0
	const.s  f7, 3
	const.s  f15, 15
	rfr      a0, f1
	rfr      a15, f14
	wfr      f0, a1
	wfr      f15, a14

	// --- FP compares: result to a boolean register ---
	un.s  b0, f1, f2
	un.s  b15, f14, f13
	oeq.s b0, f1, f2
	oeq.s b15, f14, f13
	ueq.s b0, f1, f2
	ueq.s b15, f14, f13
	olt.s b0, f1, f2
	olt.s b15, f14, f13
	ult.s b0, f1, f2
	ult.s b15, f14, f13
	ole.s b0, f1, f2
	ole.s b15, f14, f13
	ule.s b0, f1, f2
	ule.s b15, f14, f13

	// --- FP conditional moves ---
	moveqz.s f0, f1, a2
	moveqz.s f15, f14, a13
	movnez.s f0, f1, a2
	movnez.s f15, f14, a13
	movltz.s f0, f1, a2
	movltz.s f15, f14, a13
	movgez.s f0, f1, a2
	movgez.s f15, f14, a13
	movf.s   f0, f1, b2
	movf.s   f15, f14, b13
	movt.s   f0, f1, b2
	movt.s   f15, f14, b13

	// --- conversions, at both ends of the scale immediate ---
	round.s  a0, f1, 0
	round.s  a15, f14, 15
	trunc.s  a0, f1, 0
	trunc.s  a15, f14, 15
	floor.s  a0, f1, 0
	floor.s  a15, f14, 15
	ceil.s   a0, f1, 0
	ceil.s   a15, f14, 15
	utrunc.s a0, f1, 0
	utrunc.s a15, f14, 15
	float.s  f0, a1, 0
	float.s  f15, a14, 15
	ufloat.s f0, a1, 0
	ufloat.s f15, a14, 15

	// --- FP load / store, at both ends of the offset field ---
	lsi  f0, a1, 8
	lsi  f15, a14, 0
	lsi  f15, a14, 1020
	ssi  f0, a1, 8
	ssi  f15, a14, 1020
	lsip f0, a1, 8
	lsip f15, a14, 1020
	ssip f0, a1, 8
	ssip f15, a14, 1020
	lsx  f0, a1, a2
	lsx  f15, a14, a13
	lsxp f0, a1, a2
	lsxp f15, a14, a13
	ssx  f0, a1, a2
	ssx  f15, a14, a13
	ssxp f0, a1, a2
	ssxp f15, a14, a13

	// --- boolean register file: the readback paths for a compare ---
	movt a0, a1, b2
	movt a15, a14, b13
	movf a0, a1, b2
	movf a15, a14, b13
	bt   b0, fp_subset
	bt   b15, fp_subset
	bf   b0, fp_subset
	bf   b15, fp_subset

	// --- special and user registers (the narrow modeled set) ---
	rsr.br       a0
	rsr.br       a15
	wsr.br       a0
	wsr.br       a15
	xsr.br       a0
	xsr.br       a15
	rsr.cpenable a0
	rsr.cpenable a15
	wsr.cpenable a0
	wsr.cpenable a15
	xsr.cpenable a0
	xsr.cpenable a15
	rur.fcr      a0
	rur.fcr      a15
	wur.fcr      a0
	wur.fcr      a15
	rur.fsr      a0
	rur.fsr      a15
	wur.fsr      a0
	wur.fsr      a15
	isync
	ret
