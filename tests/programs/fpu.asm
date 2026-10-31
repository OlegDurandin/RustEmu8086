; 8087: the hypotenuse of 3 and 4, sqrt(2) * 10000 and a comparison through FSTSW + SAHF.
include 'emu8086.inc'
org 100h
    finit
    fild a
    fmul st, st
    fild b
    fmul st, st
    faddp st(1), st
    fsqrt
    fistp r
    mov ax, r
    call print_num
    printn ''
    fld two
    fsqrt
    fimul k
    fistp r
    mov ax, r
    call print_num
    printn ''
    ; compare: is pi > 3 ?
    fldpi
    ficomp three
    fstsw sw
    mov ax, sw
    sahf
    ja bigger
    printn 'no'
    ret
bigger:
    printn 'pi > 3'
    ret
a dw 3
b dw 4
k dw 10000
three dw 3
two dd 2.0
r dw ?
sw dw ?
DEFINE_PRINT_NUM
DEFINE_PRINT_NUM_UNS
