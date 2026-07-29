# pingpong fixture, builtin object: wrap(x) = mask_low(x + 0x123), where
# mask_low lives back in the *user* object (backward cross-object call8).
	.text
	.global	wrap
	.align	4
	.type	wrap, @function
wrap:
	entry	a1, 32
	movi	a3, 0x123
	add	a10, a2, a3
	call8	mask_low
	mov.n	a2, a10
	retw
