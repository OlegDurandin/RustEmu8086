; Rotates the stepper motor (port 7): half steps clockwise, then counter-clockwise.
; Bit 7 of port 7 is set when the motor is ready for the next command.
#start=stepper_motor.exe#
org 100h

        mov bx, offset clockwise
again:  mov cx, 32              ; 32 half steps = one turn
        mov si, 0
next:
ready:  in al, 7
        test al, 10000000b
        jz ready
        mov al, [bx][si]
        out 7, al
        inc si
        and si, 3               ; 4 patterns per cycle
        push cx
        mov cx, 4000h
pause:  loop pause
        pop cx
        loop next
        ; switch direction
        cmp bx, offset clockwise
        mov bx, offset counter
        je again
        ret

; half-step coil patterns (bits 0..2 = magnets)
clockwise db 110b, 100b, 011b, 010b
counter   db 011b, 001b, 110b, 010b
