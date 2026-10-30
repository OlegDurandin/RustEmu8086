; VGA mode 13h: colour bars and a diagonal line.
org 100h

        mov ax, 13h
        int 10h
        mov ax, 0A000h
        mov es, ax

        ; 256 vertical colour bars, 1 pixel wide, rows 0..149
        xor di, di
        mov dx, 150
bars:   xor al, al
        mov cx, 256
bar:    stosb
        inc al
        loop bar
        add di, 320 - 256
        dec dx
        jnz bars

        ; white diagonal using INT 10h/0Ch
        mov cx, 0
        mov dx, 0
line:   mov ah, 0Ch
        mov al, 15
        int 10h
        inc cx
        inc dx
        cmp dx, 200
        jb line

        mov ah, 0
        int 16h                 ; wait for a key
        mov ax, 3
        int 10h
        ret
