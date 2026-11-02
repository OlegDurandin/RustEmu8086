# Random numbers (syscalls 40, 42) in an array taken from the heap with sbrk (syscall 9).
        .data
nl:     .asciiz "\n"

        .text
main:   li   $v0, 40         # seed stream 0 with 2026: the same numbers on every run
        li   $a0, 0
        li   $a1, 2026
        syscall
        li   $v0, 9          # sbrk: 5 words of heap
        li   $a0, 20
        syscall
        move $s0, $v0        # $s0 = the array
        li   $t0, 0
fill:   li   $v0, 42         # a random number below 100 into $a0
        li   $a0, 0
        li   $a1, 100
        syscall
        sll  $t1, $t0, 2
        add  $t1, $s0, $t1
        sw   $a0, 0($t1)
        addi $t0, $t0, 1
        blt  $t0, 5, fill

        li   $t0, 0
show:   sll  $t1, $t0, 2
        add  $t1, $s0, $t1
        lw   $a0, 0($t1)
        li   $v0, 1
        syscall
        li   $v0, 4
        la   $a0, nl
        syscall
        addi $t0, $t0, 1
        blt  $t0, 5, show
        li   $v0, 10
        syscall
