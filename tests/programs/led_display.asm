; Counts on the LED display (port 199).
#start=led_display.exe#
org 100h

        mov ax, 0
again:  out 199, ax
        inc ax
        mov cx, 0FFFFh
wait1:  loop wait1
        cmp ax, 100
        jbe again
        ret
