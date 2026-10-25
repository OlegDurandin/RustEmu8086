; Rotates the stepper motor (port 7) clockwise using the half-step sequence.
#start=stepper_motor.exe#
org 100h

        mov bx, offset steps
        mov si, 0
        mov cx, 48              ; 48 half steps
next:   mov al, [bx][si]
        out 7, al
        inc si
        cmp si, 6
        jb ok
        mov si, 0
ok:     push cx
        mov cx, 8000h
pause:  loop pause
        pop cx
        loop next
        ret

steps   db 001b, 011b, 010b, 110b, 100b, 101b
