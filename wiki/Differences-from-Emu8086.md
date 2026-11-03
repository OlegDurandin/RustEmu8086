# Differences from Emu8086

**English** · [Русский](Отличия-от-Emu8086)

RustEmu8086 follows Emu8086 in the assembly language, directives, BIOS and DOS services, virtual devices and the
window layout. All 84 examples shipped with Emu8086 assemble and run the same way, except one example written in
FASM syntax. Below is where programs still diverge.

## What is missing

- **FASM syntax** (`#fasm#`) — which is why the `fasm_compatibility.asm` example does not assemble.
- **Assembler:** no `STRUC` and `IRP` / `IRPC`; `ASSUME` is accepted, but segment prefixes are not inserted from
  it; the processor executes `ENTER`, `BOUND` and `INS` / `OUTS`, but the assembler does not accept them.
- **The `.binf` file** is neither written nor read. The load address and registers of a BIN file are set in the
  source: `#load_segment=#`, `#load_offset=#`, `#cs=#`, `#ip=#` and so on.
- **Video modes**: only text modes 0–3 and 7 and graphics mode 13h (320×200, 256 colours). No CGA 4/5/6 or
  VGA 12h.
- **The external menu** (debug.exe, an external DOS) — there is no DOS on macOS.
- **Custom devices** through `c:\emu8086.io` and `c:\emu8086.hw` cannot be attached; the devices are built in.
- **The Emu8086 documentation and tutorials** are not included — instead there is the built-in reference (F1)
  and this wiki.
- **No auto-update**: *help → check for an update* opens the releases page.

## What works differently

- **Keyboard shortcuts** are partly our own — see [Keyboard shortcuts](Keyboard-shortcuts#differences-from-emu8086).
- **Windows.** By default — floating windows, as in Emu8086; in **options → windows** you can switch to panel tabs
  in the main window that do not overlap (see [Debugging](Debugging#panels-and-windows)).
- **Floppies** FLOPPY_0…FLOPPY_3 are kept in memory, not as files; images are loaded and saved through *virtual
  drive*, and Emu8086 `FLOPPY_n` images work.
- **Save the emulator's state** keeps a snapshot in memory until the app is closed.
- **DOS files are sandboxed.** On the Mac drive C: is the folder `~/Documents/RustEmu8086/vdrive/C` (like
  `vdrive\C` in Emu8086; **options** can choose the program's folder instead), in the browser — a drive in browser
  storage. There are no other drives for INT 21h: the drive letter in a path is ignored, `A:\DATA.TXT` is the same
  file as `C:\DATA.TXT`. On the Mac a run may write up to 16 MB and create up to 1024 files and folders, so that a
  loop cannot fill the computer's disk.
- **Step back** does not go through file operations on the disk.
- **`emu8086.inc`** is our own implementation with the same visible behaviour.
- **VGA_STATE is not needed**: the vertical retrace bit in port 3DAh always toggles.
- **Timer.** The BIOS tick counter always runs; IRQ0 (INT 08h → INT 1Ch) is behind the *timer IRQ* checkbox.

## What Emu8086 does not have

- **Source-level debugging**: breakpoints and the current line in the editor, conditional breakpoints, watches,
  call stack, Run to cursor, Step out, stepping into macros, values on hover.
- **Step back** over 2000 instructions, including devices, keyboard, DOS and floppies.
- **A reference** for every instruction and interrupt function, in English and Russian (**EN / RU**), with editor
  hints and F1.
- **Remembered debugging setup** for each file.
- **[NASM syntax](NASM-syntax)**.
- **A [web version](Web-version)** for any computer and tablet, and share links.
- **The [command line](Command-line)** tool `rustemu`.
- **Verified accuracy**: the processor is checked against 2.9 million tests recorded on a real 8088, and
  instruction encodings against NASM.
