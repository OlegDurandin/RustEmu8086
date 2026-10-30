; Hello, World! (COM program)
#make_COM#
org 100h

        mov dx, offset msg
        mov ah, 9
        int 21h
        ret

msg     db 'Hello, World!', 0Dh, 0Ah, '$'
