# Macros for printing and exiting: a library for .include "print_macros.s".
.macro print_int (%x)
        li   $v0, 1
        add  $a0, $zero, %x     # %x: a register or a constant
        syscall
.end_macro

.macro print_str (%text)
        .data
text:   .asciiz %text           # each expansion has its own label: text_M0, text_M1...
        .text
        li   $v0, 4
        la   $a0, text
        syscall
.end_macro

.macro done
        li   $v0, 10
        syscall
.end_macro
