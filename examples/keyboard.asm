; Shows the scan code and ASCII code of every key; ESC exits.
include 'emu8086.inc'
org 100h

        printn 'Press keys (ESC to exit):'
again:  mov ah, 0
        int 16h
        cmp al, 27
        je quit
        push ax
        print 'scan='
        mov al, ah
        mov ah, 0
        call print_num_uns
        print '  ascii='
        pop ax
        push ax
        mov ah, 0
        call print_num_uns
        print '  char='
        pop ax
        putc al
        printn ''
        jmp again
quit:   ret

DEFINE_PRINT_NUM_UNS
END
