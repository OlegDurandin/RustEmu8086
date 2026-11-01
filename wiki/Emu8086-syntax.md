# Emu8086 syntax (MASM)

**English** · [Русский](Синтаксис-Emu8086)

The main language of RustEmu8086 is the MASM dialect understood by Emu8086. Programs from textbooks and the
Emu8086 examples assemble unchanged. The second language is [NASM](NASM-syntax); the syntax is detected
automatically.

A full description of every instruction (flags, an example) is in the built-in reference: **F1** on a word in the
editor.

## A program line

```asm
label:  instruction  operands    ; comment
```

Case does not matter: `MOV AX, BX` and `mov ax, bx` are the same.

## Program format

| Format | How to choose it | Where it is loaded |
|---|---|---|
| **COM** | `org 100h` (or `#make_com#`) | `0700:0100`, CS = DS = ES = SS |
| **EXE** | `segment … ends` segments or `.model/.code` (or `#make_exe#`) | the module from segment `0710h`, DS = ES = PSP |
| **BIN** | `#make_bin#` | `#load_segment=#` (default `0100h`) |
| **BOOT** | `#make_boot#` | `0000:7C00`, at most 512 bytes |

Without a `#make_…#` directive: `org 100h` means COM (even with `.data` / `.code`), otherwise EXE if there are
segments, otherwise COM. **new** on the toolbar offers templates of all four formats.

A segment (and a whole COM program) cannot be larger than 64 KB — the assembler points at the line that crosses
the limit.

### COM

```asm
org 100h
    mov ah, 9
    mov dx, offset msg
    int 21h
    ret
msg db 'Hello!$'
```

### EXE

```asm
#make_exe#
data segment
    msg db 'Hello from EXE!', 0Dh, 0Ah, '$'
ends
stack segment
    dw 128 dup(0)
ends
code segment
start:
    mov ax, data            ; DS is not set for you in an EXE
    mov ds, ax
    lea dx, msg
    mov ah, 9
    int 21h
    mov ax, 4C00h           ; terminate
    int 21h
ends
end start                   ; entry point
```

The simplified form: `.model small`, `.stack`, `.data`, `.code`, `.startup` (sets DS) and `.exit`.

## Data

```asm
b   db 5, 'A', 0FFh         ; bytes
w   dw 1234h, -1            ; words
d   dd 12345678h            ; double word
s   db 'text', 13, 10, '$'  ; string
arr dw 10 dup(0)            ; 10 zero words
buf db 80 dup(?)            ; reserved, not initialized
N   equ 10                  ; constant
X = N * 2                   ; redefinable constant
```

Numbers: `10`, `0Ah`, `1010b`, `17o`, characters `'A'`. In expressions: `+ - * / mod`, `shl shr`, `<< >>`,
`and or xor not`, comparisons, `$` — the current address.

## Variable or label

This is the key feature of Emu8086:

| Declaration | What the name is | `mov ax, name` |
|---|---|---|
| `count dw 5` | a **variable** — a typed memory cell | reads the value from memory |
| `count: dw 5` | a **label** (with a colon) — just an address | loads the address |
| `next:` | a label in the code | loads the address |

The address of a variable is `offset count` or `lea dx, count`, its segment is `seg count`. An array element is
`arr[si]`, `arr[bx+2]`, `[arr+4]`.

## Operand size

| Code | Result |
|---|---|
| `mov ax, [bx]` | word — the size comes from the register |
| `mov [bx], 5` | **byte** if the number fits in a byte (as in Emu8086), otherwise word |
| `inc [bx]` | error: the size is unknown |
| `inc byte ptr [bx]`, `inc b.[bx]` | byte |
| `inc word ptr [bx]`, `inc w.[bx]` | word |
| `mov al, var` (`var dw`) | the register wins over the variable's type — a byte is read |

## Procedures and macros

```asm
print_char proc near        ; FAR: RET becomes RETF
    mov ah, 0Eh
    int 10h
    ret
print_char endp

newline macro               ; expanded where it is written
    local skip              ; a unique label in every expansion
    mov al, 13
    call print_char
    mov al, 10
    call print_char
endm
```

Macro parameters: `name macro a, b`; a default value — `a:=1`. Also: `rept n … endm`,
`if / ifdef / ifndef / else / endif`, `include 'file.inc'`.

## Emu8086 directives

| Directive | What it does |
|---|---|
| `#make_com#`, `#make_exe#`, `#make_bin#`, `#make_boot#` | program format |
| `#load_segment=0500#`, `#load_offset=0000#` | where to load a BIN (hex) |
| `#cs=…#`, `#ip=…#`, `#ax=…#`, `#ds=…#` … | registers when a BIN starts |
| `#mem=address,bytes#` | write bytes to memory before the start |
| `#start=robot.exe#` | open a [virtual device](Virtual-devices) |

## Quirks (as in Emu8086)

- A far conditional jump (beyond −128…+127 bytes) is assembled as "inverted short jump + `jmp near`". An
  out-of-range `jcxz` becomes `or cx, cx` + `jz`.
- The 8086 has no `shl ax, 3` — it is assembled as three `shl ax, 1`.
- 80186 extensions work: `pusha`, `popa`, `push 5`, `imul ax, bx, 3`, `leave`.
- Everything after `end` is ignored.

Missing: `STRUC`, `IRP` / `IRPC`, automatic segment prefixes from `ASSUME` — see
[Differences from Emu8086](Differences-from-Emu8086).

## Assembling to a file

**compile** (**⌘B**) writes `.com` / `.exe` / `.bin`, the `.lst` listing and the `.symbol` symbol table next to the
source. In the browser the assembled file is downloaded.
