; Traffic lights controller (virtual device on port 4).
; Each light uses 3 bits: bit0 = red, bit1 = yellow, bit2 = green.
; Lights 1..4 occupy bits 0-2, 3-5, 6-8, 9-11.
#make_COM#
#start=traffic_lights.exe#
org 100h

        mov si, 0
next:   mov ax, states[si]
        out 4, ax
        mov cx, waits[si]
delay:  push cx
        mov cx, 0FFFFh
spin:   loop spin
        pop cx
        loop delay
        add si, 2
        cmp si, 8
        jb next
        mov si, 0
        jmp next

;               4   3   2   1
states  dw      001_100_001_100b    ; 1,3 red  2,4 green
        dw      001_010_001_010b    ; 1,3 red  2,4 yellow
        dw      100_001_100_001b    ; 1,3 green 2,4 red
        dw      010_001_010_001b    ; 1,3 yellow 2,4 red
waits   dw      20, 6, 20, 6
