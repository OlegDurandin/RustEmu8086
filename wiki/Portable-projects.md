# Portable projects / Переносимые проекты

A `.remu` file and a new share link carry the same version 1 JSON package. No server stores
shared projects: links contain the deflated, base64url-encoded JSON after `#project=`.
The optional `&run` flag starts the program after opening. Old `#name=…&code=…&arch=…` links
remain readable with their original single-source behavior.

## Use

1. Keep dependencies inside the main source folder and use relative include paths.
2. Choose **Share link…** (or the link icon on a tablet). Export assembles the current source
   to collect the files actually used. Fix assembly errors before exporting a new project.
3. Enter prepared console input, including the final newline when the program expects Enter.
4. Copy the link or choose **export project…**. Above 8,000 link characters the dialog recommends
   a file because messengers can truncate links.
5. Open `.remu` with **Open** on desktop or **Upload** in the browser. Imported projects can be
   saved in **my programs**; browser drafts preserve their dependencies. Desktop Save exports
   the project again. Download source still exports just the editor's source.

CLI uses the packaged assembly settings; conflicting assembly options are errors.
`--input` explicitly replaces prepared input, while `--max`, `--regs`, and `--trace` work as usual.

```sh
cargo run -p rustemu-cli -- run examples/projects/mips-input.remu
# 40 (input echo), then 42
cargo run -p rustemu-cli -- run examples/projects/8086-include.remu
# X!
cargo run -p rustemu-cli -- asm examples/projects/mips-input.remu -o /tmp/project-text.bin
```

## Schema

- `version`: `1`; other versions are rejected before changing the current document.
- `architecture`: `8086` or `MIPS32`.
- `entry`: the main source's relative path, present in `files`.
- `files`: map of relative POSIX paths to UTF-8 source text. Includes and enabled exception
  handlers are packaged. MARS directory assembly applies to `.asm` / `.s` files beside `entry`.
- `dialect`: `Masm`, `Nasm`, or `null` for detection (8086).
- `mars`: shared MARS settings, including memory configuration, delayed branching, pseudo
  instructions, start at main, self-modifying code, warnings, program arguments, popup input,
  directory assembly and the internal exception-handler path.
- `input`: prepared console input. LF means Enter; it is restored on assembly/load and Reload.

See the two [example packages](../examples/projects/) for complete JSON.

## Bounds and behavior

Packages allow at most 1,024 files, 4 MiB of source text in total, 4 KiB per path, and 4 KiB
of prepared input. JSON and decompressed link payloads are capped at 32 MiB, including escaping
and metadata. Absolute paths, paths containing `..` components, backslashes, drive prefixes,
NUL characters, noncanonical paths and names differing only by case are rejected. Internal
include references may use `..` only when resolution stays inside the package. No files are
extracted onto the host. Unknown memory configurations and missing entry/handler files fail validation.

Capture verifies that the package assembles again without host fallback. Dependencies outside
the main folder or absolute include references must be moved inside and made relative first.
Only files used by assembly are captured; personal drive C: is never bundled automatically.

Portable programs use empty temporary runtime storage on desktop, browser and CLI. Source
files are for assembly, not runtime file input; runtime writes are not exported or synchronized
to the personal drive. Console input is delivered through bounded keyboard queues. Hardware
keys, MMIO events, dialog answers, time and random state are outside this version's format.
Breakpoints, watches and running machine state are also outside the package.

The package reproduces source and MARS configuration; it does not guarantee deterministic
behavior for programs using clocks, randomness or external device events.

## По-русски

В **Share link…** можно подготовить ввод и передать проект ссылкой либо файлом `.remu`.
Файл открывается через Open / Upload и работает в CLI. Настройки MARS и исходные зависимости
переносятся вместе с программой, старые ссылки поддерживаются. Для экспорта исходник должен
успешно собираться; зависимости нужно держать внутри его папки с относительными include.

Личный диск C: не входит в пакет: программа работает с отдельным временным диском.
Ввод ограничен 4 КиБ, исходники — 4 МиБ и 1 024 файлами. Часы, случайность, события устройств
и состояние отладчика не переносятся.
