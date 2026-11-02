# An exception handler: an arithmetic overflow jumps to 0x80000180 (kernel text), the handler
# prints a message and returns after the faulting instruction.
        .kdata
oops:   .asciiz "overflow caught, going on\n"

        .ktext 0x80000180
        move $k1, $a0        # $a0 and $v0 are the program's: keep them
        move $k0, $v0
        li   $v0, 4
        la   $a0, oops
        syscall
        move $v0, $k0
        move $a0, $k1
        mfc0 $k0, $14        # EPC: the instruction that caused the exception
        addi $k0, $k0, 4     # return to the next one
        mtc0 $k0, $14
        eret

        .text
main:   li   $t0, 0x7fffffff
        addi $t1, $t0, 1     # signed overflow: exception
        li   $v0, 1          # the program goes on
        li   $a0, 42
        syscall
        li   $v0, 10
        syscall
