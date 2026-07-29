# pingpong fixture, user object: lp_main(arg) = wrap(arg), and the helper
# mask_low(x) = x & 0xff that the *builtin* object calls back into — cross-
# object call8 relocations in both directions.
	.text
	.global	lp_main
	.align	4
	.type	lp_main, @function
lp_main:
	entry	a1, 32
	mov.n	a10, a2
	call8	wrap
	mov.n	a2, a10
	retw

	.global	mask_low
	.align	4
	.type	mask_low, @function
mask_low:
	entry	a1, 32
	extui	a2, a2, 0, 8
	retw
