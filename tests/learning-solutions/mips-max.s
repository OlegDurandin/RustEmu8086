.data
values: .word 3,-5,17,8
.text
main:
la $t1,values
lw $t0,0($t1)
li $t2,3
loop:
addiu $t1,$t1,4
lw $t3,0($t1)
ble $t3,$t0,skip
nop
move $t0,$t3
skip:
addiu $t2,$t2,-1
bgtz $t2,loop
nop
li $v0,10
syscall
