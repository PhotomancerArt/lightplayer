# mix fixture, user object: lp_main(arg) = builtin_mix(arg, 7) + 1.
# Exercises R_XTENSA_SLOT0_OP on a cross-object call8 (builtin_mix is
# undefined here and lives in mix_lib.o).
	.text
	.global	lp_main
	.align	4
	.type	lp_main, @function
lp_main:
	entry	a1, 32
	mov.n	a10, a2
	movi	a11, 7
	call8	builtin_mix
	addi	a2, a10, 1
	retw
