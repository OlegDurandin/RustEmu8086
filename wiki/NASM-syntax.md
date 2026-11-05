# NASM syntax

**English** · [Русский](Синтаксис-NASM)

Besides the Emu8086 syntax, RustEmu8086 assembles NASM programs the way `nasm -f bin` does: COM programs, boot
sectors and flat binaries. The output is checked byte for byte against the real NASM.

```asm
bits 16
org 100h
    mov ah, 9
    mov dx, msg             ; in NASM a name is an address
    int 21h
    mov al, [count]         ; memory is read in brackets
    ret
msg   db 'Hello from NASM!', 13, 10, '$'
count db 3
```

## How the syntax is chosen

Automatically. NASM is selected if the file has any of these:

- `bits 16` / `[bits 16]`, `[org …]`, `section .text`, `cpu`, `global`;
- `%define`, `%macro`, `%include`, `%assign`, `%rep`, `%if`;
- `times`, `resb` / `resw` / `resd`, `$$`;
- a size without `ptr`: `byte [..]`, `word [..]`.

The `.nasm` file extension and a `#nasm#` line also select NASM. If a program does not assemble in the syntax
detected this way but does in the other one, the other one is used (for example, `push word 5` or `` `a` ``
without other signs of NASM). To set it explicitly: the **Options** menu or the **options** window → *assembler
syntax* (auto / Emu8086 / NASM); on the command line — `rustemu asm file --syntax nasm`. The build message says
which syntax the program was assembled with.

Templates: **new** → the *use NASM syntax* checkbox (COM, BIN, boot sector); the `hello_nasm.asm` and
`boot_nasm.asm` examples in **examples**.

## The main difference from Emu8086

In NASM a name is an address, and memory is read only in square brackets:

| Code | Emu8086 | NASM |
|---|---|---|
| `mov ax, value` | reads the variable | loads the address of `value` |
| `mov ax, [value]` | reads the variable | reads the variable |
| `mov byte [bx], 5` | error (needs `byte ptr`) | written just like this |
| `mov [bx], 5` | byte if the number fits | error: size not specified |
| `mov ax, 'ab'` | AH = 'a', AL = 'b' | AL = 'a', AH = 'b' |

The debugger takes this into account: in watches, conditions and tooltips `count` is the address and `[count]`
the value.

## Program format

| `org` | Format |
|---|---|
| `org 100h` | COM |
| `org 0x7c00` | boot sector (512 bytes, started at 0000:7C00) |
| anything else or none | BIN |

Sections `.text`, `.data`, `.bss`: as in `nasm -f bin`, `.data` follows the code aligned to 4 bytes, and `.bss`
takes the addresses after it but is not written to the file.

## What is supported

- `bits 16`, `use16`, `org`; `cpu`, `global`, `extern` are accepted and ignored.
- Labels with and without a colon, local labels `.loop` (in the debugger — `PREVIOUS_LABEL.LOOP`).
- `db dw dd dq dt`, `times`, `resb … rest`, `align`, `alignb`, `equ`, `$`, `$$`.
- Numbers `0x1F`, `1Fh`, `$1F`, `0b1010`, `1010b`, `0o17`, `17q`, `0d99`; strings `'…'`, `"…"` and `` `…` `` with
  `\n \t \r \0 \xHH`.
- Operands `byte` / `word` / `dword` without `ptr`, `[es:di]`, `short` / `near` / `far`, `seg:off`.
- Operators as in NASM, with its precedence: `?:`, logical `|| ^^ &&`, comparisons (true is 1), then `| ^ &`,
  shifts, `+ -`, `* / %`.
- Preprocessor: `%define` (including with parameters: `%define SQ(x) ((x)*(x))`), `%xdefine`, `%undef`,
  `%assign`, `%macro … %endmacro` with `%1…%9` and `%%label`, `%rep`, `%if / %elif / %else / %endif`, `%ifdef`,
  `%ifndef`, `%include`, `%error`. A `%define` may use others (`%define B (A+A)`): as in NASM, its text is expanded
  where it is used, with the definitions in force there, while `%xdefine` expands it at once; a name is not
  expanded inside its own text.
- Structures: `struc … endstruc` (fields `.x resw 1` → offset `name.x`, size `name_size`) and
  `istruc … at … iend`.

```nasm
struc point
    .x: resw 1
    .y: resw 1
endstruc
    mov ax, [p + point.y]     ; 20
    mov cx, point_size        ; 4
p:  istruc point
        at point.x, dw 10
        at point.y, dw 20
    iend
```

## What is missing

- Only `-f bin`. A DOS EXE is built in NASM through an object format and a linker — that is not available; for
  EXE use the [Emu8086 syntax](Emu8086-syntax).
- `%macro m 1+`, `%0`, `%rotate`, `%strlen` / `%substr`, `incbin`, floating-point numbers in data, arbitrary
  section names, `%push` / `%$label`.
- Far conditional jumps are assembled, as in Emu8086, as "inverted short jump + `jmp near`" (NASM without
  `cpu 8086` emits an 80386 instruction the 8086 does not have), so the bytes differ from NASM here.

More in the *NASM syntax* tab of the built-in reference (**Help → NASM syntax**).
