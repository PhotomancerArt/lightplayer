# funptr fixture, builtin object: table_pick(i) = table[i], counter += 1.
# Carries .data (table), .bss (counter), and its own literal pool referencing
# both — R_XTENSA_32 into the data region plus SLOT0_OP l32r's.
	.data
	.global	table
	.align	4
table:	.word	10, 20, 30, 40

	.bss
	.global	counter
	.align	4
counter:
	.space	4

	.section .literal, "a"
	.align	4
.Ltab:	.word	table
.Lc:	.word	counter

	.text
	.global	table_pick
	.align	4
	.type	table_pick, @function
table_pick:
	entry	a1, 32
	l32r	a4, .Ltab
	addx4	a4, a2, a4
	l32i	a2, a4, 0
	l32r	a4, .Lc
	l32i	a5, a4, 0
	addi	a5, a5, 1
	s32i	a5, a4, 0
	retw
