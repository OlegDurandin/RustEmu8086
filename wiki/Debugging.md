# Debugging

**English** · [Русский](Отладка)

**F9** (**▶ emulate**) assembles the program and loads it into the emulator. Then you can run it at full speed or
step by step and watch what happens to the registers, memory and screen.

## Steps

| Key | Button | What it does |
|---|---|---|
| **F8** | Single step | one instruction; on a line with a macro — steps into the macro |
| **F10** | Step over | the whole line: a macro, `CALL` or `INT` runs as one step |
| **⇧F11** | Step out | run until the current procedure or macro returns |
| **⇧F8** or **F6** | Step back | one step back |
| **F4** | Run to cursor | run to the line with the cursor (or right-click → *Run to line N*) |
| **F5** | Run / Stop | start and stop; the **step delay** slider slows execution down |
| **F7** | show current instruction | show the current instruction |

**Step back** restores registers, memory, the screen, and the state of devices, keyboard, DOS and floppies — up
to 2000 instructions back. File operations on the disk (create, write, delete) cannot be undone, so the history
stops at them.

`INT` is a single step: the BIOS or DOS service runs as a whole. If the program waits for a key, the emulator
pauses until you press one.

## Panels and windows

By default everything is as in Emu8086: every tool — emulator, screen, memory, stack, variables, watch,
breakpoints and the rest — is a separate floating window (**options → windows: classic windows (Emu8086)**);
**reset** next to it puts the windows back in their initial places. At startup only the editor is open; the
emulator windows and the tools that were open last time appear on **emulate**.

In **options → windows** you can choose **panels**: tools open as tabs of the workspace and do not overlap. On the
left — the editor (with the source, listing and reference tabs next to it), in the middle — the emulator with
watch, ALU and FPU below it; on the right — the screen, with what the emulator buttons **vars**, **debug**,
**stack** open below it, plus the log and the devices; under the editor — breakpoints and call stack. The
**flags** window (and the *lexical flag analyser*) is a small floating window, as in Emu8086; **memory** floats too,
so the emulator keeps its height. A tab can be dragged
to another group or to the edge of a neighbouring panel, the borders between panels can be moved, and the cross on
a tab closes the tool. The layout is remembered. **Emulator → Reset window layout** (or the **reset** button in
**options**) restores the default layout.

Dialogs (options, about, template choice and so on) and the ASCII table pop up on top in both modes. In the tablet
layout tools always open as windows.

## Where to see the state

- **The current line** — the ▶ arrow in the editor and the highlight in the source (the **source** tab; in the
  classic mode, the **original source code** window). Inside a macro from `emu8086.inc` the source window shows the
  library code, and a banner at the top says where it was called from.
- **Registers** — in the **emulator** window; the ones that changed on the last step are red. A value can be
  edited right in the field. Double-clicking a register name opens the **extended value viewer** — the value in
  every number base.
- **Editor tooltip**: hover over a register or a variable to see its current value.
- **Disassembly** — instructions from CS:IP; jumps and calls show label names (`CALL 0150h ; SCAN_NUM`). Program
  data (`DB`, `DW`, strings from the `print` macro) is shown as data — `DB 'Hello', 0Dh, 0Ah, '$'` with the
  variable name above it — and the zeros after the program as one line, `DB 65219 DUP (0)`. The **data as DB**
  checkbox under the list turns this off: then every byte is decoded as an instruction, as the processor sees it.
  If the program executes bytes from its data, the current instruction is still shown as code.
  The list scrolls with the mouse wheel or the bar to its right — through the whole code segment; the **⌖ CS:IP**
  button under the list, **F7** or the next step bring it back to the current instruction.
  After the program terminates, a banner "The program has terminated (exit code 0)" replaces the list: CS:IP no
  longer points into the program at that moment. Step back returns to its last instructions.

The buttons at the bottom of the emulator window are as in Emu8086:

| Button | Window |
|---|---|
| **screen** | the emulator screen |
| **source** | the source with the current line |
| **reset** | load the program again |
| **aux** | memory, ALU, FPU (8087 coprocessor), stop on condition, symbol table, listing, breakpoints, watch, call stack, devices, log |
| **vars** | variables and constants (hex / dec / signed / bin / ascii) |
| **debug** | a state log after every stop and a DEBUG.EXE-style console |
| **stack** | the stack from SS:FFFE, the current SP highlighted |
| **flags** | flags as 0/1; **analyse** shows which conditional jumps would be taken now |

## Breakpoints

- **In the editor**: click the margin to the left of the line number, or **⌘F8**. A red dot is a plain breakpoint,
  orange — one with a condition, grey — disabled. When you edit the text, breakpoints move with their lines.
- **In the disassembly**: double-click an instruction.
- The **breakpoints** window (*aux* → *breakpoints*, or right-click a line → *Breakpoint condition…*):
  - a **condition** for each breakpoint: `cx == 0`, `[bx] > 10`, `al == 'A'`;
  - a **hit counter**, enable and disable.
- Breakpoints, conditions, watches and bookmarks are remembered for each file and survive a restart.

## Stop on condition

As in Emu8086: the expression is checked after every instruction, and the emulator stops when it **becomes true**
or **changes its value**. For example, `[counter] == 0`, or `sp < 0FFF0h` to catch a stack overflow. The window
is *aux* → *stop on condition*, or the **debug** menu of the emulator window.

## Watch, call stack, ALU, FPU

- **watch** — expressions recalculated after every step; the ones that changed are red. To add a variable from the
  editor: right-click → *Add watch*.
- **call stack** — the chain of calls from the `CALL` / `INT` return addresses on the stack, plus the macros of the
  current line. Clicking a row opens it in the editor.
- **FPU** — the 8087 coprocessor: registers ST(0)…ST(7), bits C3–C0, exceptions, the control word (see
  [8087 coprocessor](8087-coprocessor)).
- **ALU** — the last arithmetic or logic operation: operands and the result in hex, binary and decimal, flags
  before and after.

## Expressions

Conditions, watches and stop on condition accept:

| What | Examples |
|---|---|
| registers | `ax`, `al`, `ds`, `ip`, `sp` |
| flags | `zf`, `cf`, `sf`, `of` … |
| variables | `value`, `table[si]`, `offset msg` |
| memory | `[bx]`, `word [si+2]`, `es:[di]` |
| numbers | `10`, `0Ah`, `1010b`, `'c'` |
| C operators | `+ - * / % << >> & \| ^ ~ ! == != < <= > >= && \|\|` |
| assembler operators | `and or xor not shl shr mod` |

In NASM programs a name is an address and the value is in brackets: `count` is the address, `[count]` the value
(see [NASM syntax](NASM-syntax)).

## DEBUG.EXE console

The **debug** button opens the log and a DEBUG.EXE-style command line. Numbers are hex; addresses are `seg:off`,
linear, or `register:off`.

| Command | What it does |
|---|---|
| `r [reg [value]]` | show the registers / change a register |
| `d [addr] [len]` | memory dump (without an address — continue) |
| `e addr b1 b2 …` | write bytes (or `'text'`) |
| `f addr len byte` | fill memory |
| `u [addr] [count]` | disassemble |
| `t [n]` | execute n instructions |
| `p` | step over |
| `g [addr]` | run (to an address) |
| `h a b` | sum and difference in hex |
| `cls` | clear the console |

## The emulator window menu

- **file** — load an executable, reload, examples, reset the emulator and memory, the program's command line
  (PSP:80h), **save the emulator's state** / **load from previous state** (the snapshot lives until the app is
  closed).
- **debug** — steps, stop on condition, **run until** (**⌃F8**) and **set CS:IP** (**⌃⇧F7**) for the instruction
  selected by a click in the disassembly.
- **view** — all windows; **virtual devices** — [devices](Virtual-devices); **virtual drive** — floppies for
  INT 13h and drive C: (on the Mac — *show drive C: folder*, in the browser — *drive C: files…*).

## When the code has changed

If you edit the text after loading, the emulator and source windows warn that the program is out of date. **F9**
assembles and loads it again; breakpoints stay on their lines.
