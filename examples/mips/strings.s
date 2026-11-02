# Read a line, then print it in upper case and its length.
        .data
ask:    .asciiz "Type a line: "
buf:    .space 64
upper:  .asciiz "Upper case: "
count:  .asciiz "\nCharacters: "

        .text
main:   li   $v0, 4
        la   $a0, ask
        syscall
        li   $v0, 8          # read a string: $a0 buffer, $a1 its size
        la   $a0, buf
        li   $a1, 64
        syscall

        la   $t0, buf        # convert in place, count up to the new line
        li   $t2, 0
conv:   lb   $t1, 0($t0)
        beqz $t1, end
        beq  $t1, '\n', end
        blt  $t1, 'a', keep
        bgt  $t1, 'z', keep
        addi $t1, $t1, -32   # 'a' - 'A'
        sb   $t1, 0($t0)
keep:   addi $t0, $t0, 1
        addi $t2, $t2, 1
        j    conv

end:    sb   $zero, 0($t0)   # drop the new line
        li   $v0, 4
        la   $a0, upper
        syscall
        la   $a0, buf
        syscall
        la   $a0, count
        syscall
        li   $v0, 1
        move $a0, $t2
        syscall
        li   $v0, 10
        syscall
