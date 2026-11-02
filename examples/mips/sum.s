# Sum 1 + 2 + ... + n for n read from the keyboard.
        .data
ask:    .asciiz "n = "
result: .asciiz "sum = "

        .text
main:   li   $v0, 4
        la   $a0, ask
        syscall
        li   $v0, 5          # read an integer into $v0
        syscall
        move $t0, $v0        # $t0 = n
        li   $t1, 0          # $t1 = sum

loop:   blez $t0, done       # while n > 0
        add  $t1, $t1, $t0   #   sum += n
        addi $t0, $t0, -1    #   n--
        j    loop

done:   li   $v0, 4
        la   $a0, result
        syscall
        li   $v0, 1          # print the integer in $a0
        move $a0, $t1
        syscall
        li   $v0, 10
        syscall
