; Sends text to the virtual printer (port 130).
#start=printer.exe#
org 100h

        mov si, offset text
next:   lodsb
        cmp al, 0
        je done
        out 130, al
        jmp next
done:   ret

text    db 'Hello, printer!', 0Dh, 0Ah, 'This is RustEmu8086.', 0Dh, 0Ah, 0
