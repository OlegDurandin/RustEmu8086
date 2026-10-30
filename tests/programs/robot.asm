; Robot (ports 9, 10, 11): explores the room and switches on every lamp it finds.
; Commands (port 9): 1 forward, 2 turn left, 3 turn right, 4 examine, 5 lamp on, 6 lamp off.
; Data (port 10): 0 empty, 255 wall, 7 lamp on, 8 lamp off.
; Status (port 11): bit 0 data ready, bit 1 busy, bit 2 error.
#start=robot.exe#
org 100h

R_CMD   equ 9
R_DATA  equ 10
R_STAT  equ 11

again:  call examine            ; AL = what is in front of the robot
        cmp al, 8               ; a lamp that is off?
        je light_it
        cmp al, 0               ; free cell?
        jne turn
        call random             ; now and then turn even when the way is free
        and al, 7
        jz turn
        mov al, 1               ; move forward
        call command
        jmp again
turn:   call random
        and al, 1
        add al, 2               ; 2 = turn left, 3 = turn right
        call command
        jmp again
light_it:
        mov al, 5
        call command
        jmp again

; Pseudo-random number in AL (16-bit linear congruential generator).
random  proc
        push dx
        mov ax, seed
        mov dx, 25173
        mul dx
        add ax, 13849
        mov seed, ax
        mov al, ah
        pop dx
        ret
random  endp

; Sends command AL and waits until the robot is idle again.
command proc
        out R_CMD, al
busy:   in al, R_STAT
        test al, 2
        jnz busy
        ret
command endp

; Returns in AL the result of examining the cell in front.
examine proc
        mov al, 4
        call command
no_data:
        in al, R_STAT
        test al, 1
        jz no_data
        in al, R_DATA
        ret
examine endp

seed    dw 1234
