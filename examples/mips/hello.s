# Hello, MIPS: print a string with syscall 4 and end with syscall 10.
        .data
msg:    .asciiz "Hello, MIPS!\n"

        .text
main:   li   $v0, 4          # service 4: print the string at $a0
        la   $a0, msg
        syscall
        li   $v0, 10         # service 10: exit
        syscall
