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
| registers | in groups, one at a time, with **PC** always under their tabs: **GPR** — all 32 general purpose registers in pairs (`$zero $at`, `$v0 $v1`…; hover a name for its number and use), **Special** — PC, HI, LO, **CP0** — BadVAddr, Status, Cause, EPC (read only; the last exception's code is shown), **FPU** — `$f0..$f31` in pairs, each pair also as a double, and the 8 condition flags (a click changes one); DEC shows the numbers, HEX their bits. Changed ones are red; *HEX / DEC* switches the notation; a click changes a value while the program is paused (`$zero` is always 0) |
| Run I/O | what the program prints, and a line to type its input: **Enter** (or *send*) gives the line to the program |
| ALU | the last arithmetic, logic, comparison, shift, multiplication or division instruction: its operands and result (or HI and LO) in hex, binary, unsigned and signed, an overflow; loads, stores, branches and writes to `$zero` (`nop`) are not shown |
| memory | words, 16 bytes a row; buttons *.text*, *.data*, *heap*, *$gp*, *$sp*, *PC*, *.kdata*; the address field takes hex (`0x10010000`) or an expression (`$sp`, `array + 8`). Addresses outside every segment are shown as `········` — reading them is an error, not a zero. While the program is paused a click on a word changes it (hex `0x…` or decimal, Enter) |
| variables | the data labels, in the size of their data (`.byte` / `.asciiz` — bytes, `.half`, `.word`) |
| Keyboard and Display MMIO Simulator, Bitmap Display, Digital Lab Sim | MARS's tools for memory-mapped devices (the *tools* menu of the emulator window; see below) |
| source, listing, symbol table, watch, breakpoints, call stack, stop on condition | as for the 8086; the listing is MARS's text segment: address, code, basic instruction, source |

The 8086's own windows — the screen, flags, FPU, the stack window, virtual devices, drive C:, the DEBUG.EXE
console and the emu8086.inc library — are not shown in the MIPS workspace.

In the debugger and the watch window registers are written as in the source: `$t0`, `$8`, `$sp + 4`, `$f12` (its bits), `fcc`
(the condition flags as a byte); labels give
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
- **registers & memory** — the 32 registers with their numbers and conventions, PC / HI / LO, the `$f`
  registers and condition flags of coprocessor 1, MARS's memory map.

In the editor, hovering over an instruction, a directive or a `syscall` shows a summary (the service of a
`syscall` is taken from the `li $v0, n` before it); F1 or ⌘-click opens the article.

## Command line

`rustemu asm / run / disasm` work with MIPS programs too: `.s` files and MIPS sources are recognized, `--arch mips`
forces it. See [Command line](Command-line).

## What is supported

- The instructions of MARS — 77 integer and those of coprocessor 1 — and its pseudo-instructions, directives
  `.text .data .ktext .kdata .word .half .byte .float .double .ascii .asciiz .space .align .extern .globl .eqv .set`, `%hi` / `%lo`.
- System calls 1–17, 30–36, 40–44: printing (floats and doubles too), reading, `sbrk`, exit, files (on drive C:, shared with DOS
  programs: its folder on the Mac, the **drive C:** window in the browser, where they are kept and can be downloaded), time, `sleep`, random numbers.
- Exceptions (overflow, unaligned access, addresses outside the segments, traps) and a handler at `0x80000180`.
- Floating point as in MARS: `float` in one register, `double` in an even register and the next one (an odd
  one stops the program, as in MARS), roundings, conversions, comparisons into 8 condition flags. Checked against
  MARS on random programs: registers, flags, memory and errors are the same.
- Macros and `.include`, by MARS's rules (see below).
- MARS's memory-mapped devices: the Keyboard and Display MMIO Simulator, the Bitmap Display and the Digital Lab
  Sim (see below).

Not yet: dialogs (50–59), MARS's settings (delayed branching…).

## Macros and .include

```asm
.include "print_macros.s"        # print_int, print_str, done

.macro for (%reg, %from, %to, %body)
        add  %reg, $zero, %from
loop:
        %body ()
        add  %reg, %reg, 1
        ble  %reg, %to, loop
.end_macro
```

- `.macro name (%a, %b)` … `.end_macro`; a call is `name (x, y)` or `name x y`. Parameters start with `%` (SPIM's
  `$a` also works); an argument is one token — a register, a number, a label, a string.
- A macro is defined before it is used. Macros with the same name and a different number of parameters are
  different; of two with the same number the later one is used.
- A label in the body becomes `label_M0`, `label_M1`… in each expansion, so a loop can be expanded many times.
- `.include "file"` replaces its line with the file; a relative name is taken from the folder of the including file.
  In the app the file is also looked up on drive C: and among the files of the built-in examples
  (`print_macros.s` — see the *macros and .include* example). Including the same file twice is an error, as in MARS.
- The code list shows each line of an expansion as MARS does: `21: <13> la $a0, text_M0` — line 21 calls the
  macro, 13 is the line of its body. Stepping enters the body (in the included file too); errors in an expansion
  are reported as `line 21->13`.

## Memory-mapped devices

The *tools* menu of the emulator window (and *Windows* in the Emulator menu, ⋮ on the tablet) opens MARS's
tools for input and output through memory. They are always connected: a program can use them without a click on
"Connect to MIPS". A program that reaches the devices (its code builds an address with `lui …, 0xffff`) opens the
keyboard and display, or the Digital Lab Sim, at **emulate**.

**Keyboard and Display MMIO Simulator** — four words at `0xffff0000`:

| address | register |
|---|---|
| `0xffff0000` | receiver control: bit 0 — a key is ready, bit 1 — interrupt enable |
| `0xffff0004` | receiver data: the key (reading it clears the ready bit) |
| `0xffff0008` | transmitter control: bit 0 — the display is ready, bit 1 — interrupt enable |
| `0xffff000c` | transmitter data: the character to display |

Click the KEYBOARD area and type: each key goes to the receiver. A character stored to the transmitter while the
display is ready appears after the delay (*Delay length*, 5 instructions by default; without *DAD* at once), then
the display is ready again. *Delay* chooses a fixed delay or, as in MARS, a random one: uniform from 1 to the
length, or `floor(abs(N(0,1) * length) + 1)`. Character 12 clears the display; character 7 makes it a terminal of
the window's size and puts the cursor at the column in bits 20..31 and the row in bits 8..19 of the stored word.
With bit 1 of a control register set (and Status bit 0, as after the start), a key, or the display becoming ready,
interrupts the program: the handler at `0x80000180` gets code 0 in Cause with bit 8 (keyboard) or 9 (display), and
`eret` returns. *Reset* clears the display and makes the transmitter ready. See the *keyboard and display (MMIO)*
example.

As in MARS, the device holds one key: a key typed before the program has read the previous one replaces it. When
several keys arrive at once (fast typing, a paste) the window gives them to the program one by one, each after
the previous one has been read.

**Bitmap Display** — words of memory as pixels, row by row from the base address (`0x10010000`, static data, by
default), each `0x00RRGGBB`. The window sets the unit (one word) and the display size in pixels: 512 x 256 with
1 x 1 units shows 512 x 256 words. It shows memory as it is now — after a step back too; MARS paints a unit when
the program writes it. See the *bitmap display* example.

**Digital Lab Sim** — bytes from `0xffff0010`: two seven-segment displays, a 4 x 4 hexadecimal keyboard and a
counter.

| address | |
|---|---|
| `0xffff0010`, `0xffff0011` | the right and the left display: bit 0 — segment a … bit 6 — g, bit 7 — the point |
| `0xffff0012` | the row to scan (1, 2, 4, 8); bits 4..7 enable the keyboard interrupt |
| `0xffff0013` | non-zero: the counter's interrupt |
| `0xffff0014` | the key found in that row: row bit + column bit << 4 (key 2 — `0x41`), 0 if none |

A click presses a key; the next click (on any key) releases it. The counter interrupts after every 31 of MARS's
notices: instructions fetched from `.text` (not the handler's in `.ktext`) and loads and stores at
`0xffff0010..13`. Its interrupt (Cause bit 10) and the keyboard's (bit 11) come whenever the program is not in the
handler — Status's interrupt enable does not matter — and these Cause bits stay set, as in MARS. A load at
`0xffff0010` lights the display too; a word or halfword stored there is seen at its first byte only. See the
*digital lab sim* example.

The keyboard and display and the Digital Lab Sim are checked against MARS's own tools: the same display text,
segments, registers and interrupts for programs that poll, take interrupts, use the terminal and get keys or
clicks at given instructions.
