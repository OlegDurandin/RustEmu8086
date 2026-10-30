; Bubble sort of a word array, prints the result.
include 'emu8086.inc'
org 100h

        mov cx, count
        dec cx
outer:  push cx
        mov si, offset array
inner:  mov ax, [si]
        cmp ax, [si+2]
        jle no_swap
        xchg ax, [si+2]
        mov [si], ax
no_swap:
        add si, 2
        loop inner
        pop cx
        loop outer

        mov si, offset array
        mov cx, count
show:   mov ax, [si]
        call print_num
        putc ' '
        add si, 2
        loop show
        ret

array   dw 31, -4, 15, 9, 0, 100, -27, 8
count   equ ($ - array) / 2

DEFINE_PRINT_NUM
DEFINE_PRINT_NUM_UNS
END
