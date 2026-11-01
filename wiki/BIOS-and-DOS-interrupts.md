# BIOS and DOS interrupts

**English** · [Русский](Прерывания-BIOS-и-DOS)

RustEmu8086 emulates the BIOS and DOS services that teaching programs use. The interrupts are "real": the vector
table is at 0000:0000, and a program can install its own handler (INT 21h, function 25h).

The input and output registers of every function, with an example, are in the built-in reference: hover over
`int 21h` in the editor (the function number is taken from the `mov ah, …` above it) or press **F1**.

## INT 10h — video (function in AH)

| AH | Function |
|---|---|
| 00h | set the video mode (text 0–3 and 7, graphics 13h) |
| 01h | cursor shape |
| 02h | set the cursor position |
| 03h | cursor position and shape |
| 05h | active page |
| 06h / 07h | scroll a window up / down |
| 08h | read the character and attribute at the cursor |
| 09h | write a character with an attribute |
| 0Ah | write a character, keeping the attribute |
| 0Ch / 0Dh | write / read a pixel |
| 0Eh | teletype: a character, advancing the cursor |
| 0Fh | current video mode |
| 10h | palette, blinking (AL = 03h), DAC registers |
| 13h | write a string |

Video memory is in the address space: text at `B800:0000` (2 bytes per character: code and attribute), mode 13h
(320×200, 256 colours) at `A000:0000`. Programs can write to it directly.

## INT 16h — keyboard (AH)

| AH | Function |
|---|---|
| 00h | wait for a key: AL = character, AH = scan code |
| 01h | is a key available (ZF = 1 — no), without taking it |
| 02h | Shift, Ctrl, Alt state |

## INT 21h — DOS (AH)

**Console**

| AH | Function |
|---|---|
| 01h | read a character with echo |
| 02h | write the character in DL |
| 05h | a character to the printer |
| 06h | direct console I/O |
| 07h / 08h | read a character without echo |
| 09h | write the string at DS:DX up to `$` |
| 0Ah | read a line into a buffer |
| 0Bh | is a character available |
| 0Ch | flush the keyboard buffer and call the function in AL |

**Files and directories**

| AH | Function |
|---|---|
| 3Ch / 5Bh | create a file / create only if it does not exist |
| 3Dh / 3Eh | open / close a file |
| 3Fh / 40h | read / write (handles 0–2 are the keyboard and screen, 4 is the printer) |
| 41h | delete a file |
| 42h | move the file pointer |
| 43h | file attributes |
| 56h | rename |
| 4Eh / 4Fh | find the first / next file |
| 39h / 3Ah / 3Bh | create / remove / change a directory |
| 47h | current directory |
| 19h | current drive |

**Other**

| AH | Function |
|---|---|
| 25h / 35h | set / get an interrupt vector |
| 2Ah / 2Ch | date / time (from the computer's clock) |
| 30h | DOS version |
| 48h | allocate memory |
| 4Ch | terminate the program, AL is the exit code |

0Dh, 0Eh, 1Ah / 2Fh (DTA), 2Bh / 2Dh, 31h, 33h, 36h, 44h, 49h, 4Ah, 4Dh, 50h are there in a simplified form.
Running other programs (4Bh) is not supported.

### Where files are stored

File functions work in a sandbox (drive C:):

- **in the Mac app** — the folder `~/Documents/RustEmu8086/vdrive/C`, like `vdrive\C` in Emu8086. To open it:
  *Emulator → Virtual drive → show drive C: folder*. In the **options** window (*drive C:*) you can choose the
  program's folder instead: then the program sees the files next to its source, but can also change and delete
  them. A program cannot get outside the drive, including through symbolic links;
- **in the browser** — a drive in browser storage, up to 2 MB, the **drive C:** window — see
  [Web version](Web-version).

## Other interrupts

| INT | What it does |
|---|---|
| 20h | terminate the program |
| 11h | equipment list (number of floppy drives) |
| 12h | memory size |
| 13h | floppies: 00h reset, 02h read, 03h write sectors, 08h parameters; DL = 0..3 is FLOPPY_0…FLOPPY_3 |
| 15h | 86h — wait CX:DX microseconds (real time) |
| 19h | reboot — terminates the program |
| 1Ah | 00h tick counter, 02h time, 04h date (BCD) |
| 33h | mouse: 00h reset, 01h / 02h show / hide the cursor, 03h position and buttons, 04h set the cursor |

**Timer.** The BIOS tick counter (0040:006C) runs from the computer's clock, 18.2 times a second. The hardware
interrupt IRQ0 (INT 08h → INT 1Ch) is turned on with the **timer IRQ** checkbox in the emulator window, on the
command line with `--timer-irq`.

**Floppies.** *virtual drive* in the emulator window: up to four 1.44 MB floppies, loading and saving images
(Emu8086 `FLOPPY_n` images work), writing the boot sector, booting from a floppy. Floppies are kept in memory —
use **save floppy image** to keep one.

A boot loader and a kernel on one floppy:

1. The kernel (`#make_bin#`, for example `#load_segment=0800#`): **emulate**, then *virtual drive → write '.bin'
   file to floppy…* → *the program loaded in the emulator*, cylinder 0, head 0, **sector 2** → **write**. The
   program loaded in the emulator can be written in the browser too; a `.bin` file from the disk only on the Mac.
2. The boot loader (`#make_boot#`, `org 7C00h`, reads the kernel with `INT 13h`/02h): **emulate**, then *write 512
   bytes at 0000:7c00 to boot sector → FLOPPY_0*.
3. *boot from virtual floppy → FLOPPY_0* — like switching the computer on: sector 1 goes to `0000:7C00`, DL = 0. If
   the floppy holds the same boot loader that is assembled in the editor, debugging follows its source
   (breakpoints, the current line); the code read from the floppy (the kernel) is in the disassembly.

The Emu8086 *micro-os* example (**more examples...**: *micro-os_loader*, *micro-os_kernel*) works the same way.
