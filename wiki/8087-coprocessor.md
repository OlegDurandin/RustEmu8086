# 8087 coprocessor

**English** · [Русский](Сопроцессор-8087)

RustEmu8086 emulates the **8087** math coprocessor (FPU): a second processor "attached" to the 8086 that works
with real numbers. Its instructions start with `F` and work in both syntaxes — Emu8086 and NASM.

```asm
; hypotenuse: sqrt(3^2 + 4^2)
org 100h
    finit           ; start working with the 8087
    fild a          ; ST = 3
    fmul st, st     ; ST = 9
    fild b          ; ST = 4, ST(1) = 9
    fmul st, st     ; ST = 16
    faddp st(1), st ; ST = 25
    fsqrt           ; ST = 5
    fistp r         ; r = 5, the stack is empty
    ret
a dw 3
b dw 4
r dw ?
```

## The register stack

The 8087 has eight 80-bit registers that form a **stack**: `ST` (also `ST(0)`) is the top, `ST(1)` is below it,
and so on. `FLD` / `FILD` push a value onto the top, `FSTP` / `FISTP` pop it. Most operations work with `ST(0)`:
`FADD x` means `ST(0) = ST(0) + x`. The `P` forms (`FADDP`, `FSUBP`, …) pop the top after the operation.

In NASM the registers are written `st0` … `st7`, in Emu8086 / MASM — `st`, `st(1)` … `st(7)` (both spellings are
understood in both syntaxes). Outside coprocessor instructions `st` stays an ordinary name.

## Numbers in memory

| Directive | Size | In an instruction |
|---|---|---|
| `dd 1.5` | 32 bits (float) | `dword ptr` / `dword` |
| `dq 1.5` | 64 bits (double) | `qword ptr` / `qword` |
| `dt 1.5` | 80 bits (extended, as inside the 8087) | `tbyte ptr` / `tword` |
| `dw 7`, `dd 7`, `dq 7` | 16 / 32 / 64-bit integers | for `FILD`, `FIST`, `FISTP` |

A fraction is written with a point: `1.5`, `0.25`, `-3.0e-2`. Without a point (`dd 2`) it is an integer. A
variable `x dq 1.5` sets the size itself: `fld x`; for `[bx]` the size is written explicitly
(`fld qword ptr [bx]`), without it — 4 bytes, as in NASM. The disassembly shows such data as numbers:
`DD 2.0, 0.5`.

## Comparing and jumping

`FCOM`, `FICOM`, `FTST` put the result into bits **C3 C2 C0** of the status word. To jump on it, go through
`SAHF`:

```asm
    fld x
    ficomp n        ; compare with n
    fstsw sw        ; status word -> memory
    mov ax, sw
    sahf            ; C3 -> ZF, C0 -> CF
    ja bigger       ; x > n (unsigned jumps: JA JAE JB JBE JE)
```

`FSTSW AX` is an 80287 instruction; it works too.

## The FPU window

The **aux** button at the bottom of the emulator window → **FPU** (or the emulator window menu **view → floating
point unit**; in the panels mode — a tab next to watch) shows:

- registers `ST(0)` … `ST(7)` — the value, the tag (valid, zero, special, empty) and the 80-bit hex;
- the stack top pointer `TOP`, bits C3 C2 C1 C0 (a tooltip reminds how they get into ZF / PF / CF through `SAHF`);
- exception flags IE DE ZE OE UE PE (raised ones in red, each with a tooltip) and the control word: precision,
  rounding, masks;
- **raw bytes (as in Emu8086)** — the same as the FPU window of Emu8086: *control register* — the 14-byte
  environment (as `FSTENV` writes it: control word, status word, tags, addresses of the last instruction and
  operand) and *FPU registers* — eight 10-byte registers, low byte first (as `FSAVE` writes them).

What changed on the step is red. **Step back** restores the coprocessor state too.

## What is supported

- All 8087 instructions: load and store (`FLD`, `FST`, `FILD`, `FIST`, `FBLD`, `FBSTP`), arithmetic (`FADD`,
  `FSUB(R)`, `FMUL`, `FDIV(R)` and the `P` and `I` forms), comparisons, constants (`FLDPI`, `FLD1`, …), `FSQRT`,
  `FABS`, `FCHS`, `FRNDINT`, `FSCALE`, `FXTRACT`, `FPREM`, transcendental (`FPTAN`, `FPATAN`, `F2XM1`, `FYL2X`,
  `FYL2XP1`), control (`FINIT`, `FCLEX`, `FSTSW`, `FSTCW`, `FLDCW`, `FSTENV`, `FLDENV`, `FSAVE`, `FRSTOR`,
  `FFREE`, …).
- A few 80287 / 80387 instructions found in textbooks: `FSTSW AX`, `FSIN`, `FCOS`, `FSINCOS`, `FUCOM`, `FUCOMP`,
  `FUCOMPP`, `FPREM1`.
- Arithmetic is exact 80-bit, with the rounding and precision modes of the control word. The encoding of the
  instructions and of floating-point data is checked byte for byte against the real NASM.

## Differences from the real 8087

- Transcendental functions (`FPTAN`, `FPATAN`, `F2XM1`, `FYL2X`, `FSIN`…) are computed in double precision
  (53 bits) rather than 64 bits, and accept any argument (the 8087 only a limited range).
- Exceptions behave as masked: the result is a NaN or an infinity, and a flag is set in the status word. If an
  exception is unmasked, the error summary (ES) is also set, but no interrupt is raised.
- The assembler does not insert `FWAIT` before instructions by itself (as NASM); it is only in `FINIT`, `FSTSW`,
  `FSTCW`, `FSTENV`, `FSAVE`, `FCLEX`. In the emulator the coprocessor runs synchronously, so there is nothing to
  wait for.

Every instruction is described in detail in the built-in reference (**F1**, the "8087 coprocessor" group).
