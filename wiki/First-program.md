# Your first program

**English** · [Русский](Первая-программа)

In 10 minutes: write a program, run it, step through it and read a number from the keyboard. Everything below
works the same in the [web version](https://olegdurandin.github.io/RustEmu8086/) and in the Mac app (the keys
are the same in the browser).

## 1. Hello, World

Press **new** on the toolbar. In the *choose code template* window pick **COM template** and press **OK** — a
template opens. Replace its text with:

```asm
org 100h
    mov ah, 9               ; DOS: print a string ending with '$'
    mov dx, offset msg
    int 21h
    ret                     ; back to DOS
msg db 'Hello, 8086!', 13, 10, '$'
```

Press **▶ emulate** (or **F9**). The program is assembled and loaded into the emulator: as in Emu8086, the
**emulator** (registers and disassembly), **emulator screen** and **original source code** windows open. If
**options** is set to panels (**windows: panels**), the same opens as tabs next to the editor, without overlapping.

Press **Run** (**F5**) — `Hello, 8086!` appears on the emulator screen, and the bottom of the main window says
`Program terminated (exit code 0)`.

What is going on:

- `org 100h` — the program is a COM file: DOS loads it at offset 100h.
- `mov ah, 9` + `int 21h` — DOS function 09h prints the string at DS:DX up to the `$` character.
- `13, 10` — a line break (CR, LF).
- `ret` in a COM program returns control to DOS.

## 2. An assembly error

Change `mov ah, 9` to `mov ah, 300` and press **F9**. A message like `(2:13): value 300 out of byte range`
appears at the bottom, and the number is underlined with a wavy line in the editor. Clicking the message selects
the error. Put the `9` back.

## 3. Step by step

Take a program that adds up 1 + 2 + 3 + 4 + 5:

```asm
org 100h
    mov cx, 5               ; counter
    xor ax, ax              ; sum = 0
    mov bx, 1
next:
    add ax, bx              ; sum += bx
    inc bx
    loop next               ; cx--, repeat while cx != 0
    ret                     ; AX = 1+2+3+4+5 = 15
```

Press **F9**, then **F8** (**Single step**) a few times:

- the current line is marked with a ▶ arrow in the editor and highlighted in the source window;
- registers that changed are shown in red in the emulator window;
- `loop next` decrements CX and jumps back until CX becomes 0.

Went past the interesting spot? Press **⇧F8** (**Step back**): the emulator goes back one step, with all registers
and memory. You can go back up to 2000 steps.

Hover over `ax` in the editor — the tooltip shows its current value. At the end AX = 000Fh = 15.

## 4. A breakpoint

Click the margin to the left of the line number of `inc bx` — a red dot appears. Press **F5**: the program stops
on that line. Right-click the line → **Breakpoint condition…** opens the **breakpoints** window; enter a
condition such as `bx == 4` — now it stops only on the fourth pass. More in [Debugging](Debugging).

## 5. Keyboard input

The `emu8086.inc` library has ready-made macros for reading and printing numbers:

```asm
include 'emu8086.inc'
org 100h
    print 'Enter a number: '
    call scan_num           ; the number -> CX
    mov ax, cx
    add ax, ax
    printn ''
    print 'Twice: '
    call print_num          ; prints AX
    ret
DEFINE_SCAN_NUM
DEFINE_PRINT_NUM
DEFINE_PRINT_NUM_UNS
```

Press **F9** and **F5**. When the program waits for input, click the **emulator screen** window and type a number,
for example `21`, then Enter — the program prints `Twice: 42`. On a tablet the PC keyboard opens.

`DEFINE_SCAN_NUM` and the other `DEFINE_…` macros insert the code of the procedures — write them once, after
`ret`. All macros are described in [emu8086.inc library](Emu8086-inc-library).

## What next

- Ready-made examples: **examples** on the toolbar — the same as in Emu8086 (Hello, world, add and subtract,
  compare numbers, traffic lights, LED display, stepper motor…), plus two NASM examples; **more examples...** —
  all 84 examples shipped with Emu8086, each with a description; the filter searches names and descriptions (for
  example, `stack` or `mouse`).
- Instruction and interrupt reference: hover over an instruction or `int 21h`, **F1** opens the article.
- [Debugging](Debugging), [Emu8086 syntax](Emu8086-syntax), [BIOS and DOS interrupts](BIOS-and-DOS-interrupts).
