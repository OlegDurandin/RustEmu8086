# MIPS32

**English** · [Русский](Процессор-MIPS32)

Besides the 8086, RustEmu8086 runs MIPS32 programs the way [MARS](https://github.com/dpetersanderson/MARS) does:
the same assembler syntax, memory layout and system calls. It is a separate **workspace**: switching the processor
does not touch the 8086 program, and the other way round.

## Switching the processor

- **Desktop and web (large screen)** — the list at the left of the toolbar: *Intel 8086* / *MIPS32*.
- **Tablet** — the badge next to the file name in the top bar (*8086 REAL MODE* / *MIPS32*).

Each processor keeps its own program, unsaved changes, breakpoints, watches, the loaded machine and the open
windows. Switching pauses a running program; when you come back everything is as you left it. The application always
starts in the 8086 workspace; MIPS opens when you switch to it or open a MIPS file, example or link.

A file remembers its processor. Opening a file, a program saved in the browser or a share link switches to its
workspace. A file opened for the first time goes to the current workspace, unless its text clearly is the other
processor's code (MIPS registers such as `$t0` and `syscall`, or 8086 registers such as `ax`, `dl`) — then you are
asked which processor it is for.

## A first program

File → Examples → *Hello, MIPS* (in the MIPS workspace the Examples menu lists the MIPS examples; the 8086 ones are
in a submenu, and the other way round). Or File → New:

```asm
        .data
msg:    .asciiz "Hello!\n"

        .text
main:   li   $v0, 4          # print the string at $a0
        la   $a0, msg
        syscall
        li   $v0, 10         # exit
        syscall
```

**emulate** (F9) assembles the program and loads it; then Step (F8), Step back (⇧F8), Run (F5) work as for the
8086. **compile** only checks the program: MIPS programs have no executable file, as in MARS.

## Why the code differs from the source

Like MARS, the code list shows the machine instructions (MARS's *Basic* column), not the source. The source may use
**pseudo-instructions** — `li`, `la`, `move`, `blt`, `mul` with a constant… — which the processor does not have;
the assembler replaces each with one or more real instructions:

| Source | Machine instructions |
|---|---|
| `li $v0, 4` | `addiu $v0, $zero, 4` |
| `la $a0, msg` | `lui $at, 0x1001` + `ori $a0, $at, 0x0` (the label's address in two halves, through `$at`) |
| `move $t0, $v0` | `addu $t0, $zero, $v0` |

The source column shows a line at its first instruction; the next ones of the same line are marked `↳`.

## Windows

| Window | What it shows |
|---|---|
| emulator | the run buttons, the registers and the code: address, machine word, **basic** instruction, source line; click ○ to set a breakpoint |
| registers | in groups, one at a time, with **PC** always under their tabs: **GPR** — all 32 general purpose registers in pairs (`$zero $at`, `$v0 $v1`…; hover a name for its number and use), **Special** — PC, HI, LO, **CP0** — BadVAddr, Status, Cause, EPC (read only; the last exception's code is shown), **FPU** — not emulated yet. Changed ones are red; *HEX / DEC* switches the notation; a click changes a value while the program is paused (`$zero` is always 0) |
| Run I/O | what the program prints, and a line to type its input: **Enter** (or *send*) gives the line to the program |
| ALU | the last arithmetic, logic, comparison, shift, multiplication or division instruction: its operands and result (or HI and LO) in hex, binary, unsigned and signed, an overflow; loads, stores, branches and writes to `$zero` (`nop`) are not shown |
| memory | words, 16 bytes a row; buttons *.text*, *.data*, *heap*, *$gp*, *$sp*, *PC*, *.kdata*; the address field takes hex (`0x10010000`) or an expression (`$sp`, `array + 8`). Addresses outside every segment are shown as `········` — reading them is an error, not a zero |
| variables | the data labels, in the size of their data (`.byte` / `.asciiz` — bytes, `.half`, `.word`) |
| source, listing, symbol table, watch, breakpoints, call stack, stop on condition | as for the 8086; the listing is MARS's text segment: address, code, basic instruction, source |

The 8086's own windows — the screen, flags, FPU, the stack window, virtual devices, drive C:, the DEBUG.EXE
console and the emu8086.inc library — are not shown in the MIPS workspace.

In the debugger and the watch window registers are written as in the source: `$t0`, `$8`, `$sp + 4`; labels give
their address, `[label]` and `dword [label]` the value in memory.

## Tablet

The same three tabs: **Editor** (the source, the output, the registers), **Debugger** (the last step — the
instruction and the registers and bytes it changed; the code; the registers; watch, breakpoints and call stack) and
**Memory** (the memory and the variables). The output card shows **Run I/O** with its input line; the PC keyboard
is not used by MIPS programs.

## Reference

**Help** (or the *help* button, F1) opens the MIPS32 reference; the switch at its top shows the 8086's or the MIPS
one, and *EN / RU* the language:

- **instructions** — every machine instruction and pseudo-instruction MARS accepts, by group, in articles for a
  family (`lb / lbu`, `blt / ble / bgt / bge…`): what it does, its exceptions, an example; and its **forms** as
  MARS's table writes them, each pseudo-instruction with the machine instructions it becomes
  (`li $t1,100000 → lui $at, 0x1; ori $t1, $at, 0x86a0`);
- **system calls** — every `syscall` service of MARS by number: arguments, results, notes, an example; the ones not
  supported yet are grey;
- **directives** — `.text`, `.data`, `.word`, `.asciiz`, `.align`, `.eqv`…;
- **registers & memory** — the 32 registers with their numbers and conventions, PC / HI / LO, MARS's memory map.

In the editor, hovering over an instruction, a directive or a `syscall` shows a summary (the service of a
`syscall` is taken from the `li $v0, n` before it); F1 or ⌘-click opens the article.

## Command line

`rustemu asm / run / disasm` work with MIPS programs too: `.s` files and MIPS sources are recognized, `--arch mips`
forces it. See [Command line](Command-line).

## What is supported

- The integer instructions of MARS (77) and its pseudo-instructions, directives `.text .data .ktext .kdata .word
  .half .byte .ascii .asciiz .space .align .extern .globl .eqv .set`, `%hi` / `%lo`.
- System calls 1, 4, 5, 8–17, 30–36, 40–42: printing, reading, `sbrk`, exit, files (in drive C:'s folder on the Mac,
  in memory in the browser), time, `sleep`, random numbers.
- Exceptions (overflow, unaligned access, addresses outside the segments, traps) and a handler at `0x80000180`.

Not yet: floating point (coprocessor 1) and its system calls, dialogs (50–59), macros (`.macro`) and `.include`,
MARS's memory-mapped devices.
