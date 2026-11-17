org 100h
mov ah,0
int 16h
mov bl,al
mov dl,al
mov ah,2
int 21h
hlt
