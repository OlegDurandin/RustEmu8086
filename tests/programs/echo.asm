; Reads a line with INT 21h/0Ah and prints it back in upper case.
org 100h

        lea dx, prompt
        mov ah, 9
        int 21h

        lea dx, buffer
        mov ah, 0Ah
        int 21h

        ; convert to upper case
        xor cx, cx
        mov cl, buffer[1]
        lea si, buffer + 2
        jcxz done
next:   mov al, [si]
        cmp al, 'a'
        jb skip
        cmp al, 'z'
        ja skip
        sub al, 20h
        mov [si], al
skip:   inc si
        loop next
done:   mov byte ptr [si], '$'

        lea dx, crlf
        mov ah, 9
        int 21h
        lea dx, buffer + 2
        mov ah, 9
        int 21h
        ret

prompt  db 'Enter text: $'
crlf    db 0Dh, 0Ah, '$'
buffer  db 40, ?, 40 dup(?)
