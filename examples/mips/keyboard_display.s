# Keyboard and Display MMIO: echo the keys typed in the KEYBOARD area to the DISPLAY, letters in
# upper case; Esc clears the display. The program polls the ready bits of the device
# (Tools -> Keyboard and Display MMIO Simulator, as in MARS).
#
#   0xffff0000  receiver control     bit 0: a key is ready
#   0xffff0004  receiver data        the key (reading it clears the ready bit)
#   0xffff0008  transmitter control  bit 0: the display is ready
#   0xffff000c  transmitter data     the character to display
        .data
title:  .asciiz "Type in the KEYBOARD area (Esc clears):\n"

        .text
main:   li   $s0, 0xffff0000      # base of the device's registers
        la   $a0, title
        jal  puts
loop:   lw   $t0, 0($s0)          # receiver control
        andi $t0, $t0, 1
        beqz $t0, loop            # no key yet
        lw   $a0, 4($s0)          # the key
        beq  $a0, 27, clear       # Esc
        blt  $a0, 'a', show
        bgt  $a0, 'z', show
        addi $a0, $a0, -32        # a..z -> A..Z
show:   jal  putc
        j    loop
clear:  li   $a0, 12              # form feed clears the display
        jal  putc
        j    loop

# putc: displays the character in $a0 once the display is ready.
putc:   lw   $t0, 8($s0)          # transmitter control
        andi $t0, $t0, 1
        beqz $t0, putc            # still busy with the previous one
        sw   $a0, 12($s0)         # transmitter data
        jr   $ra

# puts: displays the string at $a0.
puts:   move $t1, $a0
        move $t2, $ra
next:   lb   $a0, 0($t1)
        beqz $a0, done
        jal  putc
        addi $t1, $t1, 1
        j    next
done:   jr   $t2
