.text
main: beq $a0,$zero,iszero
nop
li $t0,2
b done
nop
iszero: li $t0,1
done: li $v0,10
syscall
