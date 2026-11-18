# C Learning catalog

4 -> 12 exercises; 6 x 8086 and 6 x MIPS32; 34 public cases. Existing carry/stack/sum/max are retained. Added topics are MOV halves, guarded memory store, zero-length loop, keyboard/DOS output, wrapping ADDIU, aligned LW/SW, zero branch and JAL/JR. Existing max now checks misleading initial maximum.

| ID | ISA | dialect | level | cases | goal and oracle | limits |
|---|---|---|---|---|---|---|
| 8086-array-store | 8086 | Masm | beginner | 3 | Level: beginner. Store AX into slot. Preserve AX and guard bytes. DS=0700h, slot at 0200h. Check the word and both guard bytes. | 10000 steps / 1000 ms |
| 8086-byte-move | 8086 | Masm | beginner | 3 | Level: beginner. Copy AH to BL and AL to BH, preserving AX. Check AX and BX; HLT. | 10000 steps / 1000 ms |
| 8086-carry | 8086 | Masm | beginner | 3 | Level: beginner. Add 1 to AL. Propagate its carry into AH using ADC. Finish with HLT. Cases set AX; checks cover AX, AL, AH, CF and ZF. | 10000 steps / 1000 ms |
| 8086-key-output | 8086 | Masm | beginner | 3 | Level: beginner. Read one key using INT 16h. Copy its ASCII to BL and print once using DOS AH=2. End with HLT. Input is provided by each case. | 10000 steps / 1000 ms |
| 8086-loop-sum | 8086 | Masm | beginner | 3 | Level: beginner. Sum 1 through initial CX into AX; support CX=0 without wrapping the loop. Leave CX=0. Finish HLT. | 10000 steps / 1000 ms |
| 8086-stack | 8086 | Masm | beginner | 2 | Level: beginner. Swap BX and CX using PUSH and POP. Restore SP. Finish with HLT. Cases set BX and CX. Checks include registers and the two stack words in SS=0700h. | 10000 steps / 1000 ms |
| mips-addiu | MIPS32 | MARS (delay slots off) | beginner | 3 | Level: beginner. Compute $t0=$t1+1 modulo 2^32 without overflow trap. Preserve $t1, exit syscall 10. | 10000 steps / 1000 ms |
| mips-call | MIPS32 | MARS (delay slots off) | beginner | 3 | Level: beginner. Call a procedure that doubles $t1 into $t0; return using $ra. Preserve $t1 and $sp. Delay slots disabled. Exit normally. | 10000 steps / 1000 ms |
| mips-max | MIPS32 | MARS (delay slots off) | beginner | 2 | Level: beginner. Find the signed maximum of values [3,-5,17,8] in $t0, without changing the array. Exit using syscall 10. The package uses the default MARS memory layout. | 10000 steps / 1000 ms |
| mips-memory | MIPS32 | MARS (delay slots off) | beginner | 3 | Level: beginner. Store $t1 in slot at 10010000h and load it into $t0. Preserve $t1. Check register and little endian bytes. | 10000 steps / 1000 ms |
| mips-sum | MIPS32 | MARS (delay slots off) | beginner | 3 | Level: beginner. Read two integers using syscall 5. Store their 32-bit sum in $t0 using unsigned addition; overflow wraps. Exit with syscall 10. Checks cover $t0 and exit status. | 10000 steps / 1000 ms |
| mips-zero-branch | MIPS32 | MARS (delay slots off) | beginner | 3 | Level: beginner. Set $t0 to 1 when $a0 is zero, otherwise 2. Delay slots are explicitly disabled by this package. Exit normally. | 10000 steps / 1000 ms |

Each package includes starter, hint, expected registers/flags/memory/output/termination where relevant. Reference sources: tests/learning-solutions/{id}.asm or .s. Negative variants: immediate halt/exit (register/output mismatch), infinite loop (Step limit), invalid instruction (Assembly failed). Engine limits remain 16 cases, 256 asserted memory bytes per case, bounded source/input/output and no host fallback. No schema migration. Public tests are practice, not secure exams.

Inventory before: carry 3 cases (flags), stack 2 (SP/memory), sum 3 (input/GPR), max 1 (GPR/unchanged memory). Prior shortcomings: max had only one initialization; catalog lacked topic grouping and direct access to case inputs.
