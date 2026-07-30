# mix fixture, builtin object: builtin_mix(a, b) = 2*a + b.
# The intra-object call8 to the local double_it lands either pre-resolved with
# an R_XTENSA_ASM_EXPAND annotation or as a SLOT0_OP against a local symbol —
# both paths the linker prototype must handle.
	.text
	.align	4
	.type	double_it, @function
double_it:
	entry	a1, 32
	add	a2, a2, a2
	retw

	.global	builtin_mix
	.align	4
	.type	builtin_mix, @function
builtin_mix:
	entry	a1, 32
	mov.n	a10, a2
	call8	double_it
	add	a2, a10, a3
	retw
