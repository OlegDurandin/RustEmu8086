.text
main: jal twice
nop
li $v0,10
syscall
twice: addu $t0,$t1,$t1
jr $ra
nop
