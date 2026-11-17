.text
main:
li $v0,5
syscall
move $t0,$v0
li $v0,5
syscall
addu $t0,$t0,$v0
li $v0,10
syscall
