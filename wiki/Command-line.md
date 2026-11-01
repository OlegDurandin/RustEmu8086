# Command line

**English** · [Русский](Командная-строка)

`rustemu` assembles, runs and disassembles programs without windows — handy for checking assignments and for
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
| `--screen` | print the final text screen instead of the output stream |
| `--trace` | print every executed instruction |
| `--timer-irq` | enable IRQ0 (INT 08h / 1Ch) 18.2 times a second |

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

`rustemu` without arguments prints a short help.
