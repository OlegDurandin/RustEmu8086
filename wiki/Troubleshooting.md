# Troubleshooting

**English** · [Русский](Вопросы-и-проблемы)

## Installing and starting

### macOS says it cannot verify the developer

The app is not signed with an Apple certificate. Allow it to run once — see
[Installation](Installation#first-launch).

### The site loads slowly

- The first visit downloads about 3 MB. After every site update the file is different and is downloaded again;
  otherwise the page opens from the cache in a fraction of a second.
- If it is slow every time, GitHub Pages servers are most likely slow on your network. To check: in Chrome open
  DevTools → Network, reload the page and look at the time of the `…_bg.wasm` row. If it is almost the whole load
  time, the network is the cause, not the app.
- The progress bar shows how much has been downloaded. "Starting…" after 100% lasts a fraction of a second.

### My programs disappeared from the browser

"My programs" and drive C: are kept in this browser on this device. Clearing the site data deletes them, and in a
private window they are not saved at all. Download important programs as files (**File → Download**).

## Assembler

### `operand size not specified (use BYTE PTR or WORD PTR)`

`inc [bx]` does not say whether it is a byte or a word. Give the size: `inc byte ptr [bx]` or `inc w.[bx]`. See
[Emu8086 syntax](Emu8086-syntax#operand-size).

### `value 300 out of byte range`

The number does not fit in an 8-bit register or a byte. Use a 16-bit register (`mov ax, 300`) or check the number.

### `mov ax, msg` reads memory, but I need the address

`msg db …` is a variable; its name means the value. The address is `mov ax, offset msg` or `lea ax, msg`. In NASM
it is the other way round: the name is the address, the value is `[msg]`.

### `unknown instruction` on a macro name

A macro has to be declared above the place where it is used.

### An Emu8086 example does not assemble

All 84 examples shipped with Emu8086 are in **examples → more examples...** and assemble, except
`fasm_compatibility.asm` (FASM syntax). In your own program check for FASM, `STRUC` or `IRP` in the Emu8086
syntax — they are not supported, see [Differences from Emu8086](Differences-from-Emu8086) (in
[NASM](NASM-syntax) `struc` is available). If it is something else, send the example to
[issues](https://github.com/OlegDurandin/RustEmu8086/issues).

## Running

### The program does nothing — it is waiting

If the program waits for input (INT 21h / INT 16h), click the **emulator screen** window and type. On a tablet the
PC keyboard opens. **⌘V** pastes text as keyboard input.

### An EXE program prints garbage

In an EXE the DS register points to the PSP, not to your data. At the start of the program:

```asm
mov ax, data      ; with .model/.data: mov ax, @data
mov ds, ax
```

### Long garbage instead of a string

INT 21h / 09h prints up to the `$` character. Check that the string ends with one.

### The program is stuck in a loop

**F5** (Stop) stops it. Then **⇧F8** (step back) shows how it got there, and stop on condition catches the moment
when, for example, `sp` goes too low. See [Debugging](Debugging).

### Where are the files the program created

On the Mac — in the folder `~/Documents/RustEmu8086/vdrive/C` (*Emulator → Virtual drive → show drive C:
folder*); if **options** makes the program's folder drive C: — next to the source. In the browser — in the
**drive C:** window (*Emulator → Virtual drive → drive C: files…*), where you can download them.

### A program from a link does not see my files

That is by design: until a program from a link is saved, it works with an empty temporary drive C:. Save it and
run it again. See [Web version](Web-version#sharing-a-program).

### It runs too fast to see anything

The **step delay** slider in the emulator window adds a pause between instructions. Or set breakpoints and step
(**F8**).

## Found a bug

[Open an issue](https://github.com/OlegDurandin/RustEmu8086/issues): attach the program, describe what you
expected and what happened, and where you ran it (the Mac app or a browser, and which one).
