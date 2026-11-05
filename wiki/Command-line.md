# Command line

**English** · [Русский](Командная-строка)

`rustemu` assembles, runs and disassembles programs — for the 8086 and for [MIPS32](MIPS32) — without windows — handy for checking assignments and for
scripts.

Where to get it:

- in the Mac app: `/Applications/RustEmu8086.app/Contents/MacOS/rustemu`;
- from source: `cargo build --release -p rustemu-cli` → `target/release/rustemu`.

## Assembling — `rustemu asm`

```sh
rustemu asm hello.asm                 # -> hello.com
rustemu asm hello_exe.asm -l -s       # .exe + listing (.lst) + symbols (.symbol)
rustemu asm prog.asm -o out.bin       # output file name
rustemu asm boot.asm --syntax nasm    # nasm or masm; detected by default
```

## Running — `rustemu run`

Runs an `.asm` (assembling it first), `.com`, `.exe`, `.bin` or `.boot`. The program's output is printed to the
terminal.

```sh
rustemu run hello.asm
rustemu run calc.asm --input '12\n30\n'      # keyboard input, \n = Enter
echo 21 | rustemu run twice.asm              # without --input: stdin
rustemu run prog.asm --args "p1 p2"          # DOS command line (PSP:80h)
rustemu run colors.asm --screen              # print the final text screen
rustemu run sum.asm --trace                  # every instruction with registers
```

| Option | What it does |
|---|---|
| `--input <text>` | keyboard input (`\n` is Enter); without it — stdin |
| `--args <text>` | DOS command-line parameters (PSP:80h) |
| `--max <n>` | instruction limit (default 100 000 000) |
| `--regs` | print the registers at the end (to stderr) |
| `--screen` | print the final text screen instead of the output stream |
| `--trace` | print every executed instruction |
| `--timer-irq` | enable IRQ0 (INT 08h / 1Ch) 18.2 times a second |
| `--keys <text>` | MIPS: keys of MARS's MMIO keyboard (`\n` is Enter) |
| `--delayed-branching`, `--self-modifying`, `--start-at-main`, `--no-pseudo`, `--warnings-are-errors` | MIPS (also `asm`): MARS's settings, its options `db`, `smc`, `sm`, `np`, `we` |
| `--memory <config>` | MIPS (also `asm`): MARS's memory configuration: `Default`, `CompactDataAtZero`, `CompactTextAtZero` (its `mc`) |
| `--project` | MIPS (also `asm`): assemble the `.asm` and `.s` files of the program's folder with it (MARS's `p`) |
| `--exception-handler <file>` | MIPS (also `asm`): assemble this file first (MARS's exception handler setting) |
| `--program-args <text>` | MIPS: arguments for the program, separated by spaces (MARS's `pa`): `$a0` their number, `$a1` the array of their addresses |

The exit code of `rustemu run` is the program's exit code (AL in INT 21h, function 4Ch), so scripts can check the
result:

```sh
rustemu run task.asm --input '5\n' > out.txt && diff out.txt expected.txt
```

A trace line looks like this:

```
0700:010D  RET                          AX=000F BX=0006 CX=0000 DX=0000 SP=FFFE F=F206
```

## Disassembler — `rustemu disasm`

```sh
rustemu disasm hello.com
rustemu disasm boot.bin --org 7C00
```

## MIPS32 programs

`.s` files, and sources written for MIPS (registers like `$t0`, `syscall`), are assembled and run as MIPS32 programs
in the MARS dialect; `--arch mips` or `--arch 8086` says it explicitly.

```sh
rustemu asm sort.s -l -s                     # check; listing (.lst, MARS's text segment) + symbols
rustemu asm sort.s -o sort.bin               # the text segment as binary (MARS's dump .text)
rustemu run sum.s --input '100\n'            # Run I/O input; \n = Enter
rustemu run keyboard_display.s --keys 'hi\n' --max 500000   # keys of the MMIO keyboard
rustemu run sort.s --delayed-branching     # MARS's settings, as its db (see MIPS32)
rustemu asm sort.s --memory CompactTextAtZero -o sort.bin   # the text segment from address 0
rustemu run fibonacci.s --regs               # print the registers at the end
rustemu run hello.s --trace                  # every instruction with $v0 $a0 $t0 $t1 $sp
rustemu disasm sort.bin --arch mips          # from 0x00400000; --org for another address
rustemu disasm sort.bin --arch mips --mars   # MARS's basic notation: $8, branch offsets
```

The exit code is the code of `exit2` (syscall 17), 0 after `exit` (syscall 10) or when the program runs past its
last instruction; an exception without a handler prints the error and returns 5. File services (13–16) work in
the program's folder. `--keys` types on MARS's MMIO keyboard (one key each time the program has read the
previous one); what the program sent to the MMIO display is printed at the end after `[MMIO display]`. The
dialogs (syscall 50–59) have no windows here: a message is printed, a question or an input takes a line of
input. A trace line:

```
00400004  3c011001  lui $at, 0x1001              v0=00000004 a0=00000000 t0=00000000 t1=00000000 sp=7fffeffc
```

`rustemu` without arguments prints a short help.
