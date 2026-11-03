# Tablet layout

**English** · [Русский](Интерфейс-планшета)

On an iPad and other tablets RustEmu8086 runs through the [web version](Web-version): in the browser it picks the
touch interface by itself. There is no separate app for iPadOS or Android — the browser gives the same without
installing anything. You can add the site to the Home Screen: Safari → Share → Add to Home Screen.

Dark theme, large finger-sized controls.

## Top bar

The file name, the processor badge and the **Run / Pause / Step / Reset** buttons. Run, Step and Reset assemble the
program themselves if the text has changed. The badge (*8086 REAL MODE* or *MIPS32*) switches the processor: each
has its own program and debugging session, see [MIPS32](MIPS32).

Tapping the file name opens the **project menu**:

- **New ASM file** — a new file: empty or from a template;
- **Open from Files…** — a source (`.asm`, `.inc`) or a ready program (`.com`, `.exe`, `.bin`) from the iPad's Files
  or from the computer's disk;
- **Example library** — a catalogue of examples with search and descriptions;
- **Save draft** — save the program in this browser (only on this device and in this browser, not in Files);
  **My drafts** — the saved programs;
- **Export .asm** — download the source as a file;
- **Share source** — send the `.asm` through the system Share menu (AirDrop, Messages, mail…). If the browser
  cannot share files, the source is downloaded, and the status bar says so.

Unsaved changes are not lost on New / Open: you are asked Save / Discard / Cancel.

## Tabs

Each tab has one main area and its context.

- **Editor** — the source (about 60% of the width in landscape), with **Output** on the right (the emulator screen
  or the assembler messages) and a compact **CPU**: AX, BX, CX, DX together with their bytes (AH | AL …), the flags
  CF ZF SF OF, the segment registers, and an *Open debugger* link.
- **Debugger** — what one step did:
  - **Last step** — the last executed instruction and its line, what it changed (`BL 0Fh → 0Eh`,
    `BX 000Fh → 000Eh`, `CF 0 → 1`, bytes written to memory), and on the right the line of the next instruction.
    *Why did this happen?* explains the flags (`SUB 0Fh − 01h = 0Eh`, `ZF = 0 (result is not 0)` …),
    *Open ALU details* opens the ALU window over the tab;
  - **Code** — *Source* (the source, the default), *Mixed* (every source line above its instructions),
    *Assembly*. The last executed line has a blue bar, the next one (CS:IP) an amber arrow; tapping the left margin
    sets a breakpoint;
  - **CPU** — AX–DX in one cell each: the word on top (AX 001F), its bytes below (AH 00 │ AL 1F); SP BP SI DI,
    CS DS ES SS and IP, all nine flags;
  - the **Watch, Breakpoints, Call stack, Trace, Debug log** buttons open one tool under the code (tapping
    again closes it); **ALU** opens the ALU window over the tab. If Output does not fit next to the CPU (a 10–11″ iPad in landscape, a narrow window), it
    becomes such a button too.
- **Memory** — a hex editor (segment, go to address, search for bytes or text, input in HEX or ASCII, 8/16/32
  bytes per row), a byte inspector, the segment registers, the stack, a map of the 1 MB of memory.

In landscape there are two columns; in portrait and in a narrow window (Split View) the main area is on top and the
context below it.

### Registers and flags

A value that changed on the last step (or during the last Run) is green with a dot in the corner. Tapping a
register's word (the top row of the cell), one of its bytes or a flag shows the value in HEX, decimal and binary
(a flag — with a description). While the program is paused the value can be changed: a byte changes only its half
of the register (AL = AA with AX = 1234 gives AX = 12AA). While running and while the program waits for a key —
view only.

### Waiting for a key

When the program waits for input (INT 16h, INT 21h), Output says exactly where: *Waiting for a key — INT 21h at
echo.asm:10*, with an *Open keyboard* button. Code shows the program line with that `INT`, not the BIOS code; the
BIOS internals are behind the *BIOS internals* checkbox. Step is unavailable while waiting: a key is needed first.
If the program stopped on a step that waits for a key, pressing a key on the PC keyboard completes that step.

## PC keyboard

The keyboard of the emulated computer: key presses go to the program (into the BIOS keyboard buffer), not to the
editor. Code in the editor is typed with the tablet's own keyboard. The panel is docked at the bottom and takes
about a third of the screen height; Output stays above it. It opens with the keyboard icon next to ⋮, the *Open
keyboard* button, a tap on the emulator screen, and by itself when the program starts waiting for input (once —
after *Hide* it does not come back until the next wait).

- US layout plus the keys DOS programs need: Esc, Tab, Ctrl, arrows, and behind **Fn keys** — F1–F10,
  Ins / Del / Home / End / PgUp / PgDn.
- Shift and Ctrl apply to the next key only.
- *Hide* removes the panel but does not cancel the wait.
- On the Memory tab a small emulator screen is shown next to the keys.

## The ⋮ menu

- examples (the Examples menu, as in Emu8086) and **Share link…** — a link to the program in the web version;
- debugging: Assemble and load, Step back, Step over, Step out, Run to cursor;
- **Virtual drive** — drive C: (in the browser — its files, in a Mac window — the folder) and the floppies: booting
  from a floppy, writing the boot sector and a `.bin` file, a new floppy — as in the *virtual drive* menu of the
  Emu8086 interface;
- windows: **Drive C: files** (in the browser), Virtual devices, Variables, Symbol table, Listing, ALU, Stop on
  condition; **Debug log** — the DEBUG.EXE-style console on the Debugger tab (in the MIPS workspace the menu has
  only its windows: no drive, devices, FPU, debug log or reference);
- tools: Reference, Calculator, Base converter, ASCII codes;
- *Desktop layout* — switch to the Emu8086 interface; **Options** — the assembler syntax (auto / Emu8086 / NASM)
  and *all options…*; About.
