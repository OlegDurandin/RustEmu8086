; Keeps the temperature between 60 and 80 degrees: reads port 125, drives the heater on port 127.
#start=thermometer.exe#
org 100h

again:  in al, 125
        cmp al, 60
        jl heat_on
        cmp al, 80
        jle again
        mov al, 0
        out 127, al             ; heater off
        jmp again
heat_on:
        mov al, 1
        out 127, al             ; heater on
        jmp again
