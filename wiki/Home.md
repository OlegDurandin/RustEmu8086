# RustEmu8086

**English** · [Русский](Главная)

**Learn x86 assembly by watching your code execute.** RustEmu8086 is an Intel 8086 assembler, emulator and
debugger in the spirit of Emu8086. Write a program, press F9 and run it step by step: registers, flags, memory,
the stack and the screen are all in view, and you can step back when you go too far.

- **Web version** — runs in the browser on a computer or a tablet, nothing to install:
  **https://olegdurandin.github.io/RustEmu8086/**
- **macOS app** (Apple Silicon and Intel) — on the
  [Releases](https://github.com/OlegDurandin/RustEmu8086/releases) page.

## Getting started

1. [Installation](Installation) — the web version or the Mac app.
2. [Your first program](First-program) — from "Hello, World" to step-by-step debugging in 10 minutes.
3. [Debugging](Debugging) — stepping forwards and backwards, breakpoints, watches, stop conditions.

## Reference

| Topic | What it covers |
|---|---|
| [Emu8086 syntax](Emu8086-syntax) | directives, COM / EXE / BIN / BOOT formats, operand sizes |
| [NASM syntax](NASM-syntax) | the second assembly language, as in `nasm -f bin` |
| [8087 coprocessor](8087-coprocessor) | floating point: `FLD`, `FADD`, `FSQRT`, the FPU window |
| [emu8086.inc library](Emu8086-inc-library) | `PRINT`, `SCAN_NUM`, `PRINT_NUM` and other macros |
| [BIOS and DOS interrupts](BIOS-and-DOS-interrupts) | which INT 10h, 16h, 21h, 13h… functions are supported |
| [Virtual devices](Virtual-devices) | traffic lights, robot, thermometer, LED display, stepper motor, printer |
| [Editor](Editor) | highlighting, hints, search, bookmarks |
| [Keyboard shortcuts](Keyboard-shortcuts) | everything on one page |

## Web and tablet

- [Web version](Web-version) — "my programs", drive C: in the browser, share links.
- [Tablet layout](Tablet-layout) — iPad and touch screens, the PC keyboard.

## More

- [Differences from Emu8086](Differences-from-Emu8086) — what works differently and what is missing.
- [Command line](Command-line) — `rustemu asm / run / disasm` without windows.
- [Troubleshooting](Troubleshooting) — when something does not work.
- [For developers](For-developers) — building from source, how the project is organised, tests.

Found a bug or missing a feature? [Open an issue](https://github.com/OlegDurandin/RustEmu8086/issues).
