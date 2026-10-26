# RustEmu8086 — аналог Emu8086 для macOS

Цель: полноценный нативный аналог Emu8086 (редактор + ассемблер + эмулятор 8086 + отладчик +
виртуальные устройства) для macOS, написанный на Rust.

## Архитектура

Cargo workspace (корневой `Cargo.toml` — виртуальный манифест):

| Крейт | Назначение |
|---|---|
| `crates/rustemu-core` | Эмулятор: CPU 8086 (`cpu.rs`), шина/память 1 MiB с журналом (`bus.rs`), `Machine` (загрузчики, шаг/run/шаг назад, точки останова — `machine.rs`), BIOS (`bios.rs`: INT 10h/13h/15h/16h/1Ah/33h), DOS (`dos.rs`: INT 21h, файлы в песочнице), видео (`video.rs`), порты и виртуальные устройства (`devices.rs`), шрифт/CP437 (`font.rs`), скан-коды (`keyboard.rs`), дизассемблер (`disasm.rs`). Без зависимостей от GUI. |
| `crates/rustemu-asm` | Ассемблер диалекта Emu8086/MASM: `lexer` → `source` (INCLUDE, MACRO/LOCAL/ENDM, REPT, IF/IFDEF) → `parse` → `asm` (многопроходная сборка до сходимости, сегменты, данные, релокации, листинг) + `encode` (кодирование инструкций) + `expr`. Встроенная `inc/emu8086.inc`. |
| `crates/rustemu-session` | Связка ассемблер ↔ эмулятор: загрузка `Program`/файлов, сопоставление CS:IP ↔ строка исходника, Reload, выражения отладчика (`expr.rs`: регистры, флаги, переменные, `[mem]`, операторы C/ассемблера), стек вызовов (`call_stack` — эвристический обход стека по CALL/INT). |
| `crates/rustemu-cli` | Бинарь `rustemu`: `asm`, `run` (headless, ввод через `--input` или stdin), `disasm`. |
| `crates/rustemu-app` | GUI-бинарь `RustEmu8086` на `eframe/egui` 0.37 (см. этап 5). |

### Ключевые решения
- **Прерывания — «настоящие».** IVT в 0000:0000 указывает на заглушки `F000:1000+4*n`, заглушка —
  опкод `0F FF nn` («BOP») + `IRET`. На BOP `Machine::service` вызывает HLE-обработчик; изменённые
  статусные флаги копируются в FLAGS, сохранённые в стеке. Программы могут перехватывать векторы.
  `Machine::step` выполняет заглушку в том же шаге (шаг через `INT` = шаг через сервис BIOS/DOS).
- **Блокирующий ввод**: обработчик возвращает `Svc::Block`, IP откатывается на BOP, событие
  `Event::WaitingInput`; после `push_key` шаг повторяется. Повторные «пустые» опросы не пишутся в историю.
- **Видеопамять — в адресном пространстве.** Текст: `B800:0000`, режим 13h: `A000:0000`. Состояние видео —
  в BIOS Data Area (0040:0049 режим, 0040:0050 курсоры, …). GUI только рендерит память.
- **Шаг назад**: снимок `Cpu` + журнал перезаписанных байт памяти + очередь клавиш, до 2000 шагов.
- **Загрузка**: PSP в `0700h`; COM в `0700:0100`; EXE — модуль с `0710h`, DS=ES=PSP; BIN — `#load_segment#`
  (по умолчанию `0100h`); BOOT — `0000:7C00`.
- **Ассемблер**: формат — по `#make_xxx#`, иначе COM при `ORG 100h`, EXE при наличии сегментов, иначе COM.
  Переменные (`x db ..`) — память с типом; метки (`x:`, в т.ч. `x: db ..`) — непосредственные адреса.
  Дальний Jcc → автоматически `J!cc +3 / JMP near` (как Emu8086). `shl ax, 3` → 3×`shl ax,1`.
- **Устройства** тикают порциями по 128 инструкций, время — по часам хоста (`DeviceClock`, см. ниже).
  **Порты устройств Emu8086**: 4 — светофор (word), 7 — шаговый двигатель, 9/10/11 — робот,
  125/127 — термометр/нагреватель, 130 — принтер, 199 — LED-дисплей. Плюс VGA DAC 3C8h/3C9h, 3DAh, 60h.

## Совместимость с Emu8086 (сверено с оригиналом)
Установка Emu8086 пользователя (CrossOver): `~/Library/Application Support/CrossOver/Bottles/Assembly/drive_c/emu8086/`
(`examples/`, `inc/emu8086.inc`, `documentation/*.html`, исходники устройств на VB в `devices/DEVELOPER/sources/`).
Используется только как справочник и корпус тестов — **в репозиторий не копировать** (чужие файлы).
- Ассемблер: размер `[mem], imm` без типа — байт, если значение влезает в 8 бит; регистр важнее типа
  переменной/`ptr`; `msg: db` (с двоеточием) — метка-адрес; формат: `ORG 100h` → COM (даже с `.data/.code`,
  код раньше данных), иначе сегменты → EXE; `JCXZ` далеко → `OR CX,CX`+`JZ`; `<<`/`>>`; текстовые `EQU`;
  всё после `END` игнорируется; `#AL=..#`, `#MEM=..#`; BIN без директив грузится по CS:IP или 0100:0000.
- CPU в машине — 8086 + расширения 80186 (`CpuModel::I80186`: PUSHA/POPA, PUSH imm, IMUL imm, shift imm,
  ENTER/LEAVE, INS/OUTS, BOUND). SingleStepTests гоняются в строгом `I8086`.
- Устройства — в реальном времени (`DeviceClock::RealTime`), как в оригинале; для тестов `Instructions(n)`.
  Шаговый двигатель — автомат оригинала (ротор 0..3, ±1/±2), термометр (+1°/200 мс, к воздуху 1°/с,
  −65..120), робот (поле 9×6, комната по умолчанию, ошибки как в оригинале, шаг/поворот 250 мс).
- INT 15h/86h — реальное ожидание (`Event::Sleeping`), INT 19h завершает программу.
- `emu8086.inc` — своя реализация с тем же видимым поведением (SCAN_NUM эхо каждой клавиши и т.д.).

## Тестирование
- `cargo test --workspace` — юнит-тесты CPU, ассемблера (в т.ч. `nasm_compare`: ~18 000 кодировок
  сверяются с NASM, если он установлен), сквозные тесты `rustemu-session/tests/run.rs` на `examples/`.
- **Корпус Emu8086**: `cargo test -p rustemu-session --test emu8086_corpus -- --nocapture` — собирает и
  прогоняет все 84 примера из установки Emu8086 (`$EMU8086_DIR` или путь CrossOver; без установки — пропуск),
  плюс загрузка micro-os с виртуальной дискеты. Сейчас: всё собирается и выполняется без ошибок
  (кроме `fasm_compatibility.asm` — синтаксис FASM не поддерживается).
- **SingleStepTests 8088** (эталон с реального железа, ~2.9 млн тестов): данные в `testdata/8088/`
  (в .gitignore; скачать `v2/*.json.gz` + `metadata.json` из github.com/SingleStepTests/8088), запуск:
  `cargo test -p rustemu-core --release --test singlestep -- --ignored` (`SST_FILTER=F6` — подмножество).
  Сейчас проходят все 2 927 000.

## Соглашения
- Rust edition 2024, `cargo fmt`, `cargo clippy --workspace` без предупреждений.
- Комментарии в коде — на английском, документация проекта — на русском.
- Любое изменение CPU — прогонять SingleStepTests; изменение кодировок — `nasm_compare`.

## Команды
```sh
cargo build --workspace
cargo test --workspace
cargo run -p rustemu-cli -- run examples/hello.asm
cargo run -p rustemu-cli -- run examples/calc.asm --input '12\n30\n'
cargo run -p rustemu-cli -- asm examples/hello_exe.asm -l -s
cargo run -p rustemu-app            # GUI (можно передать путь к .asm)
cargo test -p rustemu-app           # UI-тесты + PNG-снимки окон в target/ui-snapshots/
./scripts/bundle_macos.sh        # RustEmu8086.app
```

## План работ (статус: [ ] — не начато, [~] — в работе, [x] — готово)

### Этап 1. Ядро CPU (`rustemu-core`)
- [x] 1.1 Workspace, крейты
- [x] 1.2 Память 1 MiB, адресация seg:off, порты I/O (трейт `Bus`)
- [x] 1.3 Регистры, флаги, ModR/M, эффективный адрес, префиксы
- [x] 1.4 Все инструкции 8086, включая недокументированные (SALC, SETMO, алиасы 60-6F/C0/C1/C8/C9/D6),
      квирки (DAA/DAS с AF, REP IDIV, AAM 0, PUSH SP), исключение деления, trap-флаг
- [x] 1.5 Дизассемблер
- [x] 1.6 Журнал «шага назад»
- [x] 1.7 Валидация на SingleStepTests/8088

### Этап 2. Ассемблер (`rustemu-asm`)
- [x] 2.1 Лексер
- [x] 2.2 Операнды и выражения (`[bx+si+d]`, `var[bx]`, `b.`/`w.`, `ptr`, `seg:`, `offset`, `seg`, `type`,
      `length`, `size`, `low/high`, `$`, `dup`, сравнения)
- [x] 2.3 Кодогенерация всех инструкций 8086 и используемых Emu8086 расширений 80186
      (PUSHA/POPA, PUSH imm, IMUL imm; сверка с NASM)
- [x] 2.4 Директивы: ORG, DB/DW/DD/DQ/DT, числовые и текстовые EQU/=, SEGMENT/ENDS, PROC/ENDP (FAR → RETF), LABEL, END,
      .MODEL/.STACK/.DATA/.CODE, INCLUDE, MACRO/ENDM/LOCAL/EXITM, REPT, IF/IFE/IFDEF/IFNDEF/ELSE/ENDIF, EVEN/ALIGN
- [x] 2.5 Директивы Emu8086: `#make_com/exe/bin/boot#`, `#load_segment/offset=#`, `#AX=..#`, `#start=..#`
- [x] 2.6 Вывод COM/EXE(MZ)/BIN/BOOT, листинг, таблица символов, карта адрес → строка
- [~] 2.7 Сообщения об ошибках: строка есть, колонка — только для синтаксических ошибок
- [ ] 2.8 Недостающее: `STRUC`, `IRP/IRPC`, `ASSUME`-автопрефиксы, `.binf`-файл — проверить,
      что поддерживает Emu8086

### Этап 3. BIOS/DOS HLE и загрузка программ
- [x] 3.1 IVT + ROM-заглушки (BOP), BIOS Data Area
- [x] 3.2 Загрузчики COM/EXE/BIN/BOOT, PSP
- [x] 3.3 INT 10h: 00,01,02,03,05,06,07,08,09,0A,0C,0D,0E,0F,10h(DAC),13h; режимы 0-3, 13h
- [x] 3.4 INT 16h, 1Ah, 15h, 11h, 12h, 13h (образ дискеты в `Machine::floppy`)
- [x] 3.5 INT 21h: консоль, 0Ah, векторы, дата/время, файлы (3Ch-42h, 39h-3Bh, 41h, 43h, 47h, 4Eh/4Fh, 56h),
      память 48h-4Ah, 4Ch; INT 20h; INT 33h (базово)
- [x] 3.6 Таймер: счётчик тиков BIOS 0040:006C идёт по часам хоста 18.2 Гц (или `instructions_per_tick` —
      детерминированно, для тестов); опционально IRQ0 → ROM-обработчик INT 08h (F000:E000) → INT 1Ch
      (галочка «timer IRQ» в эмуляторе, `--timer-irq` в CLI). Весь сегмент F000 выполняется «атомарно» в шаге.
- [ ] 3.7 Графические режимы CGA 4/5/6 и VGA 12h (если используются примерами Emu8086)

### Этап 4. CLI (`rustemu-cli`)
- [x] 4.1 `rustemu asm` (-l листинг, -s символы), `rustemu run`, `rustemu disasm`
- [x] 4.2 Интеграционные тесты на примерах

### Этап 5. GUI (`rustemu-app`, eframe/egui 0.37)
Модули: `app.rs` (состояние, файлы, compile/emulate, цикл выполнения, горячие клавиши), `ui.rs` (меню,
тулбар, сообщения, диалоги, инструменты), `editor.rs` + `highlight.rs`, `emulator.rs` (окно эмулятора и
исходника), `screen.rs`, `debug.rs`, `devices_ui.rs`, `examples.rs` (встроенные `examples/*.asm`),
`help.rs`, `reference.rs` + `reference_ui.rs`, `debugger.rs`, `fonts.rs`, `ui_tests.rs` (kittest, PNG-снимки в `target/ui-snapshots/`).
- [x] 5.1 Редактор: подсветка, номера строк, отметки ошибок, переход к строке по клику на ошибку,
      открыть/сохранить/недавние, примеры, compile (⌘B, пишет .com/.exe + .lst + .symbol), emulate (F9)
- [x] 5.2 Окно эмулятора: редактируемые регистры (изменённые — красным), дизассемблер от CS:IP,
      Load/Reload/Step back/Single step/Step over/Run/Stop, задержка шага, точки останова (по строкам,
      переживают перекомпиляцию), окно исходника с текущей строкой (внутри BIOS — строка вызывающего INT)
- [x] 5.3 Экран: текст 80x25/40x25 с CGA-цветами и курсором, 13h через текстуру и палитру DAC,
      клавиатура (символы, Enter/Esc/Backspace/Tab/стрелки/F1-F10/Ctrl+буква), мышь для INT 33h
- [x] 5.4 Окна: память (hex-дамп, переход по адресу/регистру), стек, флаги (редактируемые + анализ условий
      переходов), переменные (hex/dec/signed/bin/ascii), таблица символов, листинг, журнал
- [x] 5.5 Устройства (вкладки, выбор по `#start=..#`): светофор, шаговый двигатель, LED, термометр, принтер,
      робот (карта редактируется кликом)
- [x] 5.6 Примеры (15 шт.) и справочник (`reference.rs` — данные, `reference_ui.rs` — окно): все инструкции
      (синтаксис, флаги ODITSZAPC, пример), директивы, прерывания BIOS/DOS по функциям; из редактора —
      подсказка при наведении, F1/⌘-клик открывает статью (для `INT nn` функция берётся из `mov ah, ..` выше).
      Тест: все примеры справочника собираются, все мнемоники ассемблера описаны
- [x] 5.7 `scripts/bundle_macos.sh` → `target/release/bundle/RustEmu8086.app` (иконка генерируется
      `cargo run -p rustemu-app --example make_icon`)
- [x] 5.8 Редактор: поиск/замена (⌘F, ⌘G/⇧⌘G), переход к строке (⌘L), комментирование блока (⌘/) — `find.rs`
- [x] 5.10 Прозрачность макросов: `Line/LineInfo.origin` — строка в определении макроса; окно исходника
      показывает код `emu8086.inc` при шаге внутри макроса/процедуры (плашка + строка вызова), подсказки
      и ⌘-клик в редакторе (`rustemu_asm::library::index`), окно библиотеки (`library_ui.rs`), имена меток
      в дизассемблере (`Session::symbol_at`; LOCAL-метки называются `name??NNNN`)
- [x] 5.12 Отладка на уровне исходника: `Session::view_at` (ленивый вход в макрос: на первой инструкции
      развёртки показывается строка вызова; `LineInfo.chain` — вся цепочка вложенности), Step Over/Out
      (`Session::step_goal`/`run_step_goal`, глубина по CALL/INT↔RET/IRET), кнопка Step out (⇧F11)
- [x] 5.13 Интерфейс как в Emu8086: меню File/Edit/Bookmarks/Assembler/Emulator/Math/ASCII Codes/Help,
      панель New/Open/Examples/Save/Compile/Emulate/Calculator/Convertor/Options/Help/About, окно
      «choose code template» при New, закладки (`editcmd.rs`), калькулятор (`calc.rs`, `rustemu_asm::evaluate`),
      окно options, горячие клавиши редактора Emu8086 (⌃Q/⌃W/⌃L/⌃U/⌃G/⌃H, F2/F3)
- [x] 5.11 Темы Light (по умолчанию)/Dark/System, акцентные цвета — `colors.rs`; меню Examples в строке меню
- [x] 5.9 Debug-консоль в стиле DEBUG.EXE (`console.rs`: r/d/e/f/u/t/p/g/h)
- [x] 5.14 Отладчик уровня IDE (`debugger.rs`): точки останова и текущая строка в редакторе (клик по полю,
      ⌘F8; при правке точки и закладки сдвигаются со строками), Run to cursor (F4, контекстное меню),
      условные точки останова со счётчиком попаданий, «stop on condition» (стало истинным / изменилось),
      окна watch, call stack, ALU (`Cpu::alu_trace` — последняя операция АЛУ, ~3% к скорости),
      значение регистра/переменной при наведении, предупреждение об устаревшей программе (`App::is_stale`)
- [x] 5.15 Окно эмулятора как в Emu8086: регистры AX BX CX DX CS IP SS SP BP SI DI DS ES; кнопки screen / source /
      reset / aux (memory, ALU, FPU — нет, stop on condition, symbol table, listing + наши окна) / vars (с константами) /
      debug (журнал состояния в стиле DEBUG.EXE после каждой остановки) / stack (от SS:FFFE, SP выделен) /
      flags (0/1, кнопка analyse → «lexical flag analyser»); подсказки кнопок — тексты оригинала

### Этап 6. Полировка
- [x] 6.1 Сравнение с Emu8086: корпус из 84 оригинальных примеров (сборка + выполнение), устройства по
      исходникам оригинала, параметры командной строки, виртуальная дискета, шаблоны New
- [ ] 6.6 Синтаксис FASM (`#fasm#`) — не поддерживается
- [x] 6.2 Настройки и сессия (`Settings` через eframe persistence): тема, шрифт, масштаб экрана, задержка,
      открытые окна (положения хранит egui), галочки отладки; сессия файла (`FileSession`, последние 30, ключ —
      путь или `example:имя`): точки останова с условиями (в т.ч. в `emu8086.inc` — по имени файла, `pending_breakpoints`),
      watch, закладки, stop on condition, строка курсора. Буфер обмена: фича eframe `clipboard` (⌘C/⌘V во всех
      полях), экран эмулятора — вставка как ввод с клавиатуры и копирование текста экрана
- [~] 6.3 Производительность: ~60 млн инстр/с без истории (release, M-серия); с историей ~в 2 раза медленнее
- [x] 6.4 `.startup`/`.exit` (MASM 6), корпус примеров Emu8086
- [x] 6.5 Git: ветка `main`, CI — `.github/workflows/ci.yml` (macOS: fmt, clippy -D warnings, тесты с NASM)
