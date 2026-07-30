# funptr fixture, user object:
#   lp_main(arg) = table_pick(arg) + table_pick(arg) + counter   (counter ends at 2)
# Exercises R_XTENSA_32 on literal words holding a cross-object *function*
# address (table_pick, reached via callx8) and a cross-object *bss* address
# (counter), plus SLOT0_OP on the l32r's that fetch them.
	.section .literal, "a"
	.align	4
.Lpick:	.word	table_pick
.Lctr:	.word	counter

	.text
	.global	lp_main
	.align	4
	.type	lp_main, @function
lp_main:
	entry	a1, 32
	mov.n	a10, a2
	l32r	a3, .Lpick
	callx8	a3
	mov.n	a6, a10
	mov.n	a10, a2
	call8	table_pick
	add	a6, a6, a10
	l32r	a4, .Lctr
	l32i	a5, a4, 0
	add	a2, a6, a5
	retw
