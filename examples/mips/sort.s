# Bubble sort of a word array in the data segment.
        .data
array:  .word 34, 7, 23, 32, 5, 62, 14, 1
len:    .word 8
sep:    .asciiz " "

        .text
main:   lw   $s1, len        # passes left
outer:  addi $s1, $s1, -1
        blez $s1, print
        la   $t0, array
        li   $t1, 0          # index
inner:  bge  $t1, $s1, outer
        lw   $t2, 0($t0)
        lw   $t3, 4($t0)
        ble  $t2, $t3, noswap
        sw   $t3, 0($t0)     # swap neighbours
        sw   $t2, 4($t0)
noswap: addi $t0, $t0, 4
        addi $t1, $t1, 1
        j    inner

print:  la   $t0, array
        lw   $t1, len
show:   beqz $t1, exit
        lw   $a0, 0($t0)
        li   $v0, 1
        syscall
        li   $v0, 4
        la   $a0, sep
        syscall
        addi $t0, $t0, 4
        addi $t1, $t1, -1
        j    show
exit:   li   $v0, 10
        syscall
