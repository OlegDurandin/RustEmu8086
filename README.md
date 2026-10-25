# RustEmu8086

Ассемблер, эмулятор и отладчик процессора Intel 8086 для macOS — аналог Emu8086.

## Что нужно

- macOS 11 или новее.
- Rust 1.95 или новее. Установка: `curl https://sh.rustup.rs -sSf | sh`, обновление: `rustup update`.

## Запуск графического приложения

Из корня репозитория:

```sh
cargo run --release -p rustemu-app
```

Сразу открыть файл:

```sh
cargo run --release -p rustemu-app -- examples/hello.asm
```

Первая сборка занимает 1–2 минуты, дальше запуск почти мгновенный.

### Как приложение `.app`

```sh
./scripts/bundle_macos.sh
open target/release/bundle/RustEmu8086.app
```

Скрипт собирает `target/release/bundle/RustEmu8086.app`. Его можно перетащить в «Программы».

## Работа в приложении

1. Откройте файл (**File → Open**, ⌘O) или выберите пример в меню **Examples**.
2. Нажмите **▶ emulate** (F9). Программа соберётся и загрузится в эмулятор.
   При ошибках сборки они появятся внизу; клик по ошибке переводит к строке.
3. В окне **emulator**:
   - **Single step** (F8) — одна инструкция;
   - **Step back** (⇧F8) — откат на шаг назад;
   - **Step over** (F10) — выполнить текущую строку целиком (макрос, `CALL`, `INT`);
   - **Step out** (⇧F11) — выйти из текущего макроса или процедуры к месту вызова;
   - **Run / Stop** (F5) — запуск/остановка, ползунок **step delay** задаёт паузу между шагами.
4. Если программа ждёт ввода, щёлкните по окну **emulator screen** и печатайте.
5. Точка останова — клик по кружку слева от строки в окне **original source code**
   или двойной клик по инструкции в дизассемблере.
6. Кнопки внизу окна эмулятора открывают память, стек, флаги, переменные, листинг,
   виртуальные устройства и консоль в стиле DEBUG.EXE.

### Библиотека emu8086.inc

`include 'emu8086.inc'` подключает макросы (`PRINT`, `PUTC`, `GOTOXY`…) и процедуры
(`SCAN_NUM`, `PRINT_NUM`…). Чтобы понимать, что они делают:

- наведите курсор на имя в редакторе — появится описание; ⌘-клик открывает код;
- кнопка **📖 emu8086.inc** (или **Help → emu8086.inc library**) показывает все макросы и процедуры
  с описанием и исходным кодом;
- при пошаговой отладке на строке с макросом (`print '...'`) F10 выполняет его целиком, а F8
  заходит внутрь: окно исходника показывает код `emu8086.inc` и подсвечивает текущую строку
  (галочка **step into macros**), плашка сверху — в чём вы находитесь и откуда вызвано;
  **Step out** (⇧F11) возвращает к вызвавшей строке;
- в дизассемблере у переходов и вызовов подписаны имена меток (`CALL 0150h ; SCAN_NUM`).

### Тема

По умолчанию светлая. Переключатель ☀/🌙 — справа в панели инструментов,
выбор Light/Dark/System — в меню **View**.

### Ещё как в Emu8086

- **File → New from template** — шаблоны COM / EXE / BIN / boot sector.
- **Emulator → Set command line parameters…** — параметры программы (PSP:80h).
- **Emulator → Virtual drive** — образ дискеты для INT 13h: загрузить/сохранить (можно взять `FLOPPY_0`
  из Emu8086), записать 512 байт с 0000:7C00 в загрузочный сектор, загрузиться с дискеты.
- Директива `#start=robot.exe#` (и другие устройства) открывает нужную вкладку окна устройств.
- Синтаксис FASM (`#fasm#`) не поддерживается.

**⌘B** (compile) записывает рядом с исходником исполняемый файл (`.com`/`.exe`/`.bin`),
листинг `.lst` и таблицу символов `.symbol`.

Файловые функции DOS (INT 21h) работают с папкой, где лежит исходник; для несохранённого
файла — с `~/Documents/RustEmu8086/vdrive`.

## Командная строка

```sh
cargo build --release -p rustemu-cli        # бинарь: target/release/rustemu

target/release/rustemu run examples/hello.asm                     # собрать и выполнить
target/release/rustemu run examples/calc.asm --input '12\n30\n'   # с вводом с клавиатуры
target/release/rustemu run prog.asm --args "p1 p2"              # параметры командной строки (PSP:80h)
target/release/rustemu run examples/colors.asm --input 'x' --screen   # показать итоговый экран
target/release/rustemu asm examples/hello_exe.asm -l -s           # собрать .exe + листинг + символы
target/release/rustemu asm examples/hello.asm                     # → examples/hello.com
target/release/rustemu disasm examples/hello.com                  # дизассемблировать
```

Без `--input` ввод берётся из stdin. Остальные опции: `rustemu` без аргументов.

## Тесты

```sh
cargo test --workspace
```

Дополнительно:

- сверка кодировок с NASM выполняется автоматически, если установлен `nasm` (`brew install nasm`);
- совместимость с примерами Emu8086: если Emu8086 установлен (переменная `EMU8086_DIR` или стандартный путь
  CrossOver), `cargo test -p rustemu-session --test emu8086_corpus -- --nocapture` собирает и запускает все
  его примеры;
- проверка процессора на тестах с реального 8088 (SingleStepTests): скачать `v2/*.json.gz`
  и `v2/metadata.json` из https://github.com/SingleStepTests/8088 в `testdata/8088/`, затем
  `cargo test -p rustemu-core --release --test singlestep -- --ignored`.
