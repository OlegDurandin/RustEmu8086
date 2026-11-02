# The first 10 Fibonacci numbers with a recursive procedure: fib(n) = fib(n-1) + fib(n-2).
# Each call saves $ra and its argument on the stack.
        .data
sep:    .asciiz " "

        .text
main:   li   $s0, 0          # n = 0
next:   move $a0, $s0
        jal  fib
        move $a0, $v0        # print fib(n)
        li   $v0, 1
        syscall
        li   $v0, 4
        la   $a0, sep
        syscall
        addi $s0, $s0, 1
        blt  $s0, 10, next
        li   $v0, 10
        syscall

# fib($a0) -> $v0
fib:    blt  $a0, 2, small   # fib(0) = 0, fib(1) = 1
        addi $sp, $sp, -12
        sw   $ra, 8($sp)
        sw   $a0, 4($sp)
        addi $a0, $a0, -1
        jal  fib             # fib(n - 1)
        sw   $v0, 0($sp)
        lw   $a0, 4($sp)
        addi $a0, $a0, -2
        jal  fib             # fib(n - 2)
        lw   $t0, 0($sp)
        add  $v0, $v0, $t0
        lw   $ra, 8($sp)
        addi $sp, $sp, 12
        jr   $ra
small:  move $v0, $a0
        jr   $ra
