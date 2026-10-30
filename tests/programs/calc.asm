; Adds two numbers entered from the keyboard (uses emu8086.inc).
include 'emu8086.inc'
org 100h

        print 'First number: '
        call scan_num
        mov ax, cx
        printn ''
        print 'Second number: '
        call scan_num
        add ax, cx
        printn ''
        print 'Sum = '
        call print_num
        printn ''
        call pthis
        db 'Done.', 0
        ret

DEFINE_SCAN_NUM
DEFINE_PRINT_NUM
DEFINE_PRINT_NUM_UNS
DEFINE_PTHIS
END
