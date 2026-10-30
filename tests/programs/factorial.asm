; Recursive factorial of 0..8 printed with PRINT_NUM_UNS.
include 'emu8086.inc'
org 100h

        mov cx, 0
again:  mov ax, cx
        call print_num_uns
        print '! = '
        push cx
        call fact               ; AX = CX!
        pop cx
        call print_num_uns
        printn ''
        inc cx
        cmp cx, 8
        jbe again
        ret

; returns AX = CX! (recursive)
fact    proc
        cmp cx, 1
        ja recurse
        mov ax, 1
        ret
recurse:
        push cx
        dec cx
        call fact
        pop cx
        mul cx
        ret
fact    endp

DEFINE_PRINT_NUM_UNS
END
