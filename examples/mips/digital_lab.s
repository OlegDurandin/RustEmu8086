# Digital Lab Sim: the hexadecimal keyboard and the seven-segment displays (Tools -> Digital Lab
# Sim, as in MARS). Click a key: the right display shows it, the left one counts the presses;
# click again to release it. The counter's interrupt blinks the right display's point.
#
#   0xffff0010  right display: bit 0 = segment a ... bit 6 = g, bit 7 = the point
#   0xffff0011  left display
#   0xffff0012  row to scan (1, 2, 4, 8); bit 7 enables the keyboard interrupt
#   0xffff0013  non-zero: an interrupt every 30 instructions (the counter)
#   0xffff0014  the key found in the row: row bit | column bit << 4, 0 if none
        .data
digits: .byte 0x3f, 0x06, 0x5b, 0x4f, 0x66, 0x6d, 0x7d, 0x07, 0x7f, 0x6f, 0x77, 0x7c, 0x39, 0x5e, 0x79, 0x71
        .kdata
ticks:  .word 0                   # counter interrupts
point:  .word 0                   # the point: 0 or 0x80

        .ktext 0x80000180         # the counter's interrupt (the only one enabled)
        lui  $k0, 0xffff          # (no pseudo-instructions here: they would change $at)
        lui  $k1, 0x9000
        lw   $k1, 0($k1)          # ticks
        addi $k1, $k1, 1
        lui  $k0, 0x9000
        sw   $k1, 0($k0)
        andi $k1, $k1, 0x3f       # every 64 interrupts...
        bnez $k1, back
        lw   $k1, 4($k0)
        xori $k1, $k1, 0x80       # ...the point changes
        sw   $k1, 4($k0)
back:   eret

        .text
main:   li   $s0, 0xffff0010      # the device
        li   $t0, 1
        sb   $t0, 3($s0)          # counter on
        li   $s1, 0               # presses
        li   $s2, 0               # the last key code (0: none)
        li   $s3, 0               # the key shown
        sb   $zero, 1($s0)
        lbu  $t0, digits          # 0 on the left display
        sb   $t0, 1($s0)
scan:   li   $t1, 1               # row 1, 2, 4, 8
row:    sb   $t1, 2($s0)
        lbu  $t2, 4($s0)
        bnez $t2, key
        sll  $t1, $t1, 1
        ble  $t1, 8, row
        li   $s2, 0               # no key pressed
        j    show
key:    beq  $t2, $s2, show       # still the same press
        move $s2, $t2
        addi $s1, $s1, 1          # a new press
        andi $t3, $s1, 0xf
        lbu  $t3, digits($t3)
        sb   $t3, 1($s0)          # the number of presses (0..f) on the left
        andi $t3, $t2, 0xf        # the row bit -> 0..3
        srl  $t4, $t2, 4          # the column bit -> 0..3
        li   $s3, 0
rows:   srl  $t3, $t3, 1
        beqz $t3, cols
        addi $s3, $s3, 4
        j    rows
cols:   srl  $t4, $t4, 1
        beqz $t4, show
        addi $s3, $s3, 1
        j    cols
show:   lbu  $t5, digits($s3)
        lw   $t6, point           # with the point of the interrupt
        or   $t5, $t5, $t6
        sb   $t5, 0($s0)
        j    scan
