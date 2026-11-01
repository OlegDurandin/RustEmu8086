# emu8086.inc library

**English** · [Русский](Библиотека-emu8086)

`emu8086.inc` is a set of input and output macros and procedures, as in Emu8086. It is built into the program,
no separate file is needed:

```asm
include 'emu8086.inc'
```

All macros and procedures print through BIOS INT 10h, function 0Eh. This is our own implementation with the same
visible behaviour as in Emu8086, not the original Emu8086 file.

## Macros

Expanded right where they are written.

| Macro | What it does | Preserves |
|---|---|---|
| `PUTC char` | prints one character; `char` is a constant (`'A'`, `13`) or an 8-bit register | AX |
| `GOTOXY col, row` | moves the cursor: `col` 0..79, `row` 0..24 (constants or 8-bit registers) | AX, BX, DX |
| `PRINT 'text'` | prints a string without a line break | AX, SI |
| `PRINTN 'text'` | prints a string and a line break (CR, LF) | AX, SI |
| `CURSOROFF` | hides the text cursor | AX, CX |
| `CURSORON` | shows the cursor again | AX, CX |

## Procedures

The code of a procedure is inserted by a `DEFINE_…` macro — once, usually after the program's last `RET`. It is
called with `CALL`.

| Insert | Call | What it does |
|---|---|---|
| `DEFINE_SCAN_NUM` | `CALL SCAN_NUM` | reads a signed number (−32768..32767) from the keyboard into **CX**. Every key is echoed; extra characters are erased, Backspace deletes a digit, `-` is the minus sign, Enter finishes |
| `DEFINE_PRINT_NUM` | `CALL PRINT_NUM` | prints the signed number in **AX**. Also needs `DEFINE_PRINT_NUM_UNS` |
| `DEFINE_PRINT_NUM_UNS` | `CALL PRINT_NUM_UNS` | prints the unsigned number in **AX** (0..65535) |
| `DEFINE_PRINT_STRING` | `CALL PRINT_STRING` | prints the zero-terminated string at **DS:SI** |
| `DEFINE_PTHIS` | `CALL PTHIS` + `DB 'text', 0` | prints the string written right after the `CALL` and continues after it |
| `DEFINE_GET_STRING` | `CALL GET_STRING` | reads a string into the buffer at **DS:DI**; **DX** is the buffer size including the terminating zero |
| `DEFINE_CLEAR_SCREEN` | `CALL CLEAR_SCREEN` | clears the screen and moves the cursor to the top left corner |

## Example

```asm
include 'emu8086.inc'
org 100h
    print 'Enter a number: '
    call scan_num           ; CX = number
    mov ax, cx
    add ax, ax
    printn ''
    print 'Twice: '
    call print_num
    ret
DEFINE_SCAN_NUM
DEFINE_PRINT_NUM
DEFINE_PRINT_NUM_UNS
```

## Looking inside

- Hover over a macro name in the editor to see its description; **⌘-click** opens the code.
- **Help → emu8086.inc library** (or ⌘-click the `include 'emu8086.inc'` line) — all macros and procedures with
  descriptions and code.
- While debugging, **F8** on a line with a macro steps inside: the source window shows the library code and the
  current line (the **step into macros** checkbox), **⇧F11** returns to the call, **F10** runs the whole macro.
