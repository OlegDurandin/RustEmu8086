.data
slot: .word 0
.text
main:
sw $t1,slot
lw $t0,slot
li $v0,10
syscall
