# A86 syntax

**English** · [Русский](Синтаксис-A86)

RustEmu8086 supports an A86 source dialect for 8086/80186 programs. A86 has its own rules for numbers,
typed operands, local labels and macros; selecting the right dialect matters even for short programs.
This includes support for real-mode listings from the book “Ассемблер и программная модель x86/64”,
within the compatibility limits below.

## Choosing A86

Choose **Options → assembler syntax → A86**, or leave **auto** selected. Files ending in `.a86`, `.8`,
`.08` or `.8x`, and sources with `#a86#`, select A86 automatically. A detected A86 source is never retried
in MASM/NASM on error: that could silently change a constant such as `0100`.

```sh
rustemu asm program --syntax a86 -o program.com
rustemu run examples/hello_a86.a86 --max 1000
```

`--syntax` is an assembly option. `run` detects A86 from the extension or source marker; portable projects
can store `"dialect": "A86"` explicitly.

## Try the example

Open **examples → Hello, world (A86)**, then press **F9** and **F5**, or step with **F8**.
The source is `examples/hello_a86.a86`. It prints `Hello from A86!`, then exits with code 0.
The `clear` macro expands one call into three `XOR` instructions:

```asm
#a86#
clear macro
#rx1l
    xor #x, #x
#em

    clear ax, bx, dx
    mov dx, offset message
    mov ah, 09
    int 021
    mov ax, 04c00
    int 021
message db 'Hello from A86!', 0d, 0a, '$'
```

## Supported constructs

- Numbers with a leading zero are hexadecimal: `0100` = 256, `021` = 33, `04c00` = 19456.
  Explicit `xB`/`xD`/`xH`/`xO`/`xQ` bases and the `K` multiplier are also supported.
- Typed memory `B/W/D/Q/T`, `BIT`/`BY`, implicit addition in address expressions, `THIS`, redefinable `EQU`,
  empty data declarations and `DUP` without parentheses.
- Repeated local labels such as `L1`, forward references `>L1`, and local-label scopes in macro calls.
- Positional macros ending in `#EM`, parameters `#1`…`#9`, `#L`, `#N`, `#S`, `#V`, numeric `#R`/`#Q` loops,
  and `##IF`/`##ELSE`/`##ENDIF`; includes inherit the dialect. Expansion is bounded and cancellable.
- `STRUC` typed overlays, optionally at an explicit base: fields describe memory without allocating it.
  The debugger resolves absolute typed fields through the current DS and accepts original local-label names.
- A86 instruction shorthand: `ENTER`, unary `TEST`, three-operand `MOV`, repeated `PUSH/POP/INC/DEC`,
  counted string instructions and segment-register loads. `BOUND` uses the existing 80186 support.
- FPU stack operands `0`…`7`, inline floating constants and constant floating arithmetic in x87 precision.
- Flat programs default to COM; `ORG 0` selects BIN.

## Compatibility limits

A386 registers, address/operand-size prefixes and 32-bit instructions require a 386 target, which this
emulator does not implement. Real mode alone does not make those programs compatible with an 8086.
Object-file linking, character-loop macros and some uncommon A86 extensions are not implemented.
Byte-for-byte equivalence to the original A86 assembler is not claimed.

Of 161 textbook listings checked, automatic assembly increased from 20 to 117 (95/128 separate source
files and 22/33 extracted exercises). Assembly or process termination does not prove an exercise's result:
some listings are fragments, data, input/IRQ waits or deliberately invalid examples.
Results and reproduction commands are in `reports/a86-dialect-2026-11-15.txt`.

Developers can run `cargo test -p rustemu-asm --test a86_dialect` and
`python3 scripts/check_book_corpus.py <arch-folder> --out <new-report-folder>`.
