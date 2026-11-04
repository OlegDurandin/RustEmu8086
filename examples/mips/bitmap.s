# Bitmap Display: each word of memory from the base address is a pixel, 0x00RRGGBB, row by row.
# Tools -> Bitmap Display with its default settings (unit 1 x 1, display 512 x 256, base
# 0x10010000 - static data) shows 512 x 256 words: a gradient with a white square.
        .eqv WIDTH  512
        .eqv HEIGHT 256

        .text
main:   li   $s0, 0x10010000      # the display's base address
        move $t9, $s0
        li   $s1, 0               # y
rows:   li   $s2, 0               # x
cols:   srl  $t0, $s2, 1          # red = x / 2
        sll  $t0, $t0, 16
        sll  $t1, $s1, 8          # green = y
        or   $t0, $t0, $t1
        ori  $t0, $t0, 0x80       # blue = 128
        sw   $t0, 0($t9)
        addi $t9, $t9, 4
        addi $s2, $s2, 1
        blt  $s2, WIDTH, cols
        addi $s1, $s1, 1
        blt  $s1, HEIGHT, rows

        li   $t0, 0x00ffffff      # white
        li   $s1, 96              # rows 96..159
square: li   $s2, 224             # columns 224..287
line:   sll  $t1, $s1, 9          # (y * 512 + x) * 4
        add  $t1, $t1, $s2
        sll  $t1, $t1, 2
        add  $t1, $t1, $s0
        sw   $t0, 0($t1)
        addi $s2, $s2, 1
        blt  $s2, 288, line
        addi $s1, $s1, 1
        blt  $s1, 160, square

        li   $v0, 10
        syscall
