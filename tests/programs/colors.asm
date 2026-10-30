; Prints all 16x16 text attribute combinations using INT 10h/09h.
org 100h

        mov ax, 3
        int 10h                 ; 80x25 text mode, clear screen
        mov ax, 1003h
        mov bx, 0
        int 10h                 ; bit 7 = bright background instead of blinking

        mov dh, 2               ; row
        mov bl, 0               ; attribute
rows:   mov dl, 8               ; column
cols:   mov ah, 2
        mov bh, 0
        int 10h                 ; set cursor
        mov ah, 9
        mov al, 'a'
        add al, dl
        sub al, 8
        mov cx, 1
        int 10h                 ; write char with attribute BL
        inc bl
        inc dl
        cmp dl, 24
        jb cols
        inc dh
        cmp dh, 18
        jb rows

        mov ah, 2
        mov dx, 1400h
        int 10h
        mov dx, offset msg
        mov ah, 9
        int 21h
        mov ah, 0
        int 16h
        ret

msg     db 'Press any key...$'
