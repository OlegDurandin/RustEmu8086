# For developers

**English** · [Русский](Для-разработчиков)

RustEmu8086 is written in Rust (edition 2024); the interface is [egui / eframe](https://github.com/emilk/egui)
0.37 with the wgpu renderer: Metal on macOS, WebGPU or WebGL 2 in the browser. The licence is MIT.

## Building

Requires macOS 11+ and Rust 1.95+ (`curl https://sh.rustup.rs -sSf | sh`, update with `rustup update`).

```sh
cargo run --release -p rustemu-desktop                       # the app
cargo run --release -p rustemu-desktop -- examples/hello.asm  # open a file
cargo run -p rustemu-tablet                                  # tablet layout in a Mac window
./scripts/bundle_macos.sh                                    # RustEmu8086.app + zip
UNIVERSAL=1 ./scripts/bundle_macos.sh                        # Apple Silicon + Intel
```

The first build takes 1–2 minutes. `UNIVERSAL=1` needs `rustup target add aarch64-apple-darwin
x86_64-apple-darwin`.

### Web version

```sh
rustup target add wasm32-unknown-unknown
brew install trunk               # or: cargo install trunk
cd crates/rustemu-web
trunk serve                      # http://127.0.0.1:8080
trunk build --release            # site in crates/rustemu-web/dist/
```

The build uses the cargo profile `web`: optimised for size with LTO, the emulator core for speed. The emoji fonts
are cut down to the interface's symbols — after adding a new icon to the interface, run
`scripts/subset_emoji_fonts.py` (the `web_fonts_cover_the_ui` test reminds you). For a subfolder:
`trunk build --release --public-url /RustEmu8086/`. Open `?ui=tablet` to see the tablet layout in a desktop
browser.

## How it is organised

| Crate | What is inside |
|---|---|
| `rustemu-target` | the boundary between a processor and the rest: the `Target` trait (steps, step back, breakpoints, memory, disassembly, registers by name). No dependencies |
| `rustemu-core` | the 8086 processor (+ 80186 extensions), 1 MB of memory, `Machine` (loading, steps, step back, breakpoints), BIOS, DOS, video, devices, disassembler, 8087; `Machine` implements `Target`. No GUI dependencies |
| `rustemu-mips` | MIPS32: the integer instruction set, memory and exceptions of MARS, its system services (`syscall`), a text console, files in a sandbox, step back; the assembler of MARS's dialect and a disassembler; `impl Target` |
| `rustemu-asm` | the Emu8086 / MASM and NASM assembler: preprocessor, multi-pass assembly, encoding, listing; the built-in `emu8086.inc` |
| `rustemu-session` | glue between the assembler and the emulator through `Target`: source line ↔ address, debugger expressions, call stack, Step Over / Out; the 8086 part (program formats, floppies) in `x86.rs` |
| `rustemu-cli` | the `rustemu` program |
| `rustemu-ui` | the shared part of the interface: application state, editor, screen, debug windows, reference |
| `rustemu-desktop` | the Emu8086-style interface, the macOS program |
| `rustemu-tablet` | the tablet layout and the PC keyboard |
| `rustemu-web` | the web version: both layouts, switching on the fly |

Key decisions:

- **An architecture boundary.** The session and the shared interface (run loop, breakpoints, watches, call stack,
  disassembly list, memory dump) work with any processor through the `Target` trait; the 8086 windows (Emu8086
  registers, flags, FPU, screen, devices, drives, DEBUG.EXE console) reach the 8086 `Machine` with `x86()`, the
  MIPS ones (`rustemu_ui::mips_ui`) the MIPS machine with `mips()`. A new architecture is its own crate implementing
  `Target`, `match arch` branches in the session and its own windows.
- **A workspace per processor** (`rustemu-ui/src/app/arch.rs`). Each architecture has its own document, session,
  debugger state and windows; `App::switch_arch` parks one and brings back the other. The architecture belongs to
  the document (saved session, browser program, `&arch=` of a link); `App::has_tool` says which windows a processor
  has.
- **Interrupts are "real".** The vector table at 0000:0000 points to ROM stubs (`F000:1000 + 4·n`): a service
  opcode `0F FF nn` and `IRET`. On it the emulator calls a BIOS / DOS handler written in Rust. Programs can hook
  the vectors.
- **Video memory is in the address space** (`B800:0000`, `A000:0000`), the video state in the BIOS Data Area; the
  interface only draws memory.
- **Step back**: for every instruction — a snapshot of the registers and a log of overwritten bytes; devices, DOS
  and queues — only for instructions that may change them.
- **Loading**: PSP at `0700h`; COM at `0700:0100`; EXE from `0710h`; BIN at `#load_segment#` (default `0100h`);
  BOOT at `0000:7C00`.

## Tests

```sh
cargo test --workspace
cargo test -p rustemu-desktop -p rustemu-tablet   # UI tests, PNG snapshots in target/ui-snapshots/
```

- **NASM**: if `nasm` is installed (`brew install nasm`), ~18 000 instruction encodings and whole NASM programs are
  compared with it byte for byte.
- **Emu8086 examples** (the `examples` folder, files from the Emu8086 distribution, unchanged):
  `cargo test -p rustemu-session --test emu8086_corpus -- --nocapture` assembles and runs all 84 examples. Our own
  test programs are in `tests/programs`.
- **MARS** (the MIPS core): random programs run on the core and on the real MARS and are compared — registers,
  HI/LO, memory, exceptions; the assembler's machine code and data against MARS's for every form of every
  instruction: `MARS=<Mars.jar or a folder with Mars.class> cargo test -p rustemu-mips --test mars_compare --
  --nocapture`.
- **binutils** (the MIPS assembler): every basic instruction form with random operands, assembled by RustEmu8086 and
  by `mipsel-linux-gnu-as`, word by word, when the binutils are installed (`brew install
  mipsel-linux-gnu-binutils`).
- **objdump** (the MIPS disassembler): every basic form decodes to the same instruction and operand values as
  `mipsel-linux-gnu-objdump`, and back to the assembler's text.
- **SingleStepTests 8088**: ~2.9 million tests recorded on a real processor. Download `v2/*.json.gz` and
  `v2/metadata.json` from [SingleStepTests/8088](https://github.com/SingleStepTests/8088) into `testdata/8088/`,
  then `cargo test -p rustemu-core --release --test singlestep -- --ignored`. Every processor change is checked
  against them.

## Rules

- `cargo fmt` and `cargo clippy --workspace -- -D warnings` (also for `--target wasm32-unknown-unknown`) — no
  warnings; CI checks this.
- Every public item has a doc comment (`#![warn(missing_docs)]`); `cargo doc` with `RUSTDOCFLAGS=-D warnings` runs
  in CI.
- Code comments are in English. The README and this wiki exist in English and Russian — change both.
- Work happens in branches; `main` gets merges.

## CI and releases

- `.github/workflows/ci.yml` — fmt, clippy, docs, tests on macOS; on `main` — building the `.app` and publishing
  the site to GitHub Pages.
- `audit.yml` — `cargo audit` when dependencies change and once a week.
- `wiki.yml` — publishes the pages from the `wiki/` folder to the GitHub wiki on a push to `main`. Edit the wiki in
  the repository: the next publish overwrites edits made on the GitHub site.
- Releases: the version in `Cargo.toml` (`[workspace.package] version`), then a `vX.Y.Z` tag → `release.yml`
  publishes the release; the notes come from `release-notes/vX.Y.Z.md`.

The work plan and status are in [CLAUDE.md](https://github.com/OlegDurandin/RustEmu8086/blob/main/CLAUDE.md)
(in Russian), known issues and ideas in [TODO.md](https://github.com/OlegDurandin/RustEmu8086/blob/main/TODO.md).
