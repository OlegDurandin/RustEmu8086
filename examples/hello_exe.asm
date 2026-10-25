; Hello, World! (EXE program with segments)
#make_EXE#

data segment
    msg db "Hello from EXE!", 0Dh, 0Ah, '$'
ends

stack segment
    dw 128 dup(0)
ends

code segment
start:
    mov ax, data
    mov ds, ax
    mov es, ax

    lea dx, msg
    mov ah, 9
    int 21h

    mov ax, 4C00h
    int 21h
ends

end start
