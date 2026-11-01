# Editor

**English** · [Русский](Редактор)

The editor fills the main window. It highlights Emu8086 and NASM syntax and shows line numbers, assembly errors,
breakpoints and the debugger's current line.

## Hints and the reference

- **Hover** over an instruction, a directive, `int 21h` or an `emu8086.inc` macro to see a short description with
  the flags. For `int 21h` the function number is taken from the `mov ah, …` above it.
- When a program is loaded in the emulator, hovering over a register or a variable shows its **value**.
- **F1** (with the text cursor on a word) or **⌘-click** opens the reference article (in English or Russian — the
  **EN / RU** switch in the reference window, which also translates the hover hints); for a library macro — its
  code.

## Errors

After **F9** or **⌘B** errors appear at the bottom with the line and column (`(4:9): value 300 out of byte
range`). The place of the error is underlined with a wavy line, and clicking the message selects it.

## Search and editing

| Keys | Action |
|---|---|
| **⌘F**, **⌘H** or **⌃H** | find and replace |
| **⌘G** / **F3** | find next |
| **⇧⌘G** / **⇧F3** | find previous |
| **⌘L** or **⌃G** | go to line |
| **⌘/** | comment / uncomment lines |
| **⌃Q** / **⌃W** | comment / uncomment (as in Emu8086) |
| **⌃L** / **⌃U** | lower / upper case |

## Bookmarks

**⌃F2** (or **⌘F2**) sets and clears a bookmark on the line, **F2** / **⇧F2** go to the next / previous one. The
**Bookmarks** menu does the same with the mouse. When you edit, bookmarks move with their lines.

## Debugging from the editor

- Click the margin to the left of the line number, or **⌘F8** — a breakpoint.
- Right-click a line: *Run to line N* (**F4**), add or remove a breakpoint, *Breakpoint condition…*, *Add watch* for
  the word under the cursor.
- The debugger's current line is marked with a ▶ arrow. More in [Debugging](Debugging).

## Tools

- **Math** — an expression calculator in every number base and a number converter (the **calculator** and
  **convertor** buttons on the toolbar).
- **ASCII Codes** — a character table (code page 437) in a separate floating window, as in Emu8086 (also in the
  panels mode): hover over a character to see its code; you can keep working with the emulator without closing
  the table.
- **options** — theme (light / dark / system), assembler syntax, editor font size, step delay, debugging
  checkboxes; on the Mac — which folder is drive C: (see
  [BIOS and DOS interrupts](BIOS-and-DOS-interrupts#where-files-are-stored)), in the browser — the interface layout.
- The **Options** menu (before **Help**) — the most frequently changed settings: windows (classic Emu8086 or
  panels), assembler syntax (auto / Emu8086 / NASM), theme; **all options…** opens the **options** window.

## What is remembered

For each file (the last 30, built-in examples included): breakpoints with conditions, watches, bookmarks, the stop
condition and the cursor line. Open the file again — everything is in place.

The clipboard works in every field (**⌘C** / **⌘V**). In the **emulator screen** window **⌘V** types the text as
keyboard input, and **⌘C** copies the screen text.
