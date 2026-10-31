# RustEmu8086 — аналог Emu8086 для macOS

Цель: полноценный нативный аналог Emu8086 (редактор + ассемблер + эмулятор 8086 + отладчик +
виртуальные устройства) для macOS, написанный на Rust.

## Архитектура

Cargo workspace (корневой `Cargo.toml` — виртуальный манифест):

| Крейт | Назначение |
|---|---|
| `crates/rustemu-core` | Эмулятор: CPU 8086 (`cpu.rs`), шина/память 1 MiB с журналом (`bus.rs`), `Machine` (загрузчики, шаг/run/шаг назад, точки останова — `machine.rs`), BIOS (`bios.rs`: INT 10h/13h/15h/16h/1Ah/33h), DOS (`dos.rs`: INT 21h, файлы в песочнице), видео (`video.rs`), порты и виртуальные устройства (`devices.rs`), шрифт/CP437 (`font.rs`), скан-коды (`keyboard.rs`), дизассемблер (`disasm.rs`). Без зависимостей от GUI. |
| `crates/rustemu-asm` | Ассемблер диалектов Emu8086/MASM и NASM (`Dialect`, `detect_dialect`, `assemble_dialect`; NASM: `parse_line_nasm`, `tokenize_nasm`, перевод `%`-директив в `Preprocessor::nasm_lines`, локальные метки `rename_local_labels`, секции .text/.data/.bss с выравниванием 4, `times`, строгий размер операнда `encode::set_strict_size`). MASM: `lexer` → `source` (INCLUDE, MACRO/LOCAL/ENDM, REPT, IF/IFDEF) → `parse` → `asm` (многопроходная сборка до сходимости, сегменты, данные, релокации, листинг) + `encode` (кодирование инструкций) + `expr`. Встроенная `inc/emu8086.inc`. |
| `crates/rustemu-session` | Связка ассемблер ↔ эмулятор: загрузка `Program`/файлов, сопоставление CS:IP ↔ строка исходника, Reload, выражения отладчика (`expr.rs`: регистры, флаги, переменные, `[mem]`, операторы C/ассемблера), стек вызовов (`call_stack` — эвристический обход стека по CALL/INT), строки дизассемблера (`code.rs`: `Session::code_row` — данные программы как `DB`/`DW`/`DD` по `LineInfo::is_code`/`data_size`, нули вне кода — `DB n DUP (0)`; инструкция на CS:IP — всегда код). |
| `crates/rustemu-cli` | Бинарь `rustemu`: `asm`, `run` (headless, ввод через `--input` или stdin), `disasm`. |
| `crates/rustemu-ui` | Общая часть GUI на `eframe/egui` 0.37 (рендерер wgpu): `App` (состояние, файлы, сборка, запуск, настройки), редактор, экран, окна отладки/инструментов и диалоги (`windows::shared_windows`), справочник, сервисы ОС — `platform.rs`, `native_options()`; фича `testing` — помощники UI-тестов (`testing.rs`). Своего `eframe::App` нет. |
| `crates/rustemu-desktop` | Интерфейс Emu8086: библиотека (`DesktopApp`, главное окно с меню/тулбаром — `ui.rs`, окно эмулятора и исходника — `emulator.rs`; UI-тесты `ui_tests.rs`) + бинарь `RustEmu8086` (macOS). |
| `crates/rustemu-tablet` | Интерфейс планшета (см. 6.7) — для веб-версии на iPad и сенсорных экранах: `TabletApp` + `TabletState`; бинарь `rustemu-tablet` — окно на Mac для разработки. Нативных сборок iPadOS/Android нет. |
| `crates/rustemu-web` | Веб-версия (см. 6.8): wasm-бинарь для trunk (`index.html`); `layout.rs` — `WebApp` с `DesktopApp` или `TabletApp`, переключение на лету (`App::switch_layout`, выбор в localStorage); раскладка по умолчанию — iPad (`MacIntel` + касания), `pointer: coarse` или узкое окно; `?ui=`, `?example=…&run`, ссылки с программой `#name=…&code=…` (`rustemu_ui::share`). |

### Ключевые решения
- **Прерывания — «настоящие».** IVT в 0000:0000 указывает на заглушки `F000:1000+4*n`, заглушка —
  опкод `0F FF nn` («BOP») + `IRET`. На BOP `Machine::service` вызывает HLE-обработчик; изменённые
  статусные флаги копируются в FLAGS, сохранённые в стеке. Программы могут перехватывать векторы.
  `Machine::step` выполняет заглушку в том же шаге (шаг через `INT` = шаг через сервис BIOS/DOS).
- **Блокирующий ввод**: обработчик возвращает `Svc::Block`, IP откатывается на BOP, событие
  `Event::WaitingInput`; после `push_key` шаг повторяется. Повторные «пустые» опросы не пишутся в историю.
- **Видеопамять — в адресном пространстве.** Текст: `B800:0000`, режим 13h: `A000:0000`. Состояние видео —
  в BIOS Data Area (0040:0049 режим, 0040:0050 курсоры, …). GUI только рендерит память.
- **Шаг назад** (до 2000 шагов): на каждую инструкцию — снимок `Cpu` + журнал перезаписанных байт памяти;
  устройства (журналы портов/палитры/карты робота), DOS, очереди клавиш и IRQ, мышь и дискета — только для
  инструкций, которые могут их менять (`Machine::may_touch_system`; если неожиданно изменили — барьер истории).
  Файловые операции хоста — тоже барьер: их нельзя откатить.
- **Загрузка**: PSP в `0700h`; COM в `0700:0100`; EXE — модуль с `0710h`, DS=ES=PSP; BIN — `#load_segment#`
  (по умолчанию `0100h`); BOOT — `0000:7C00`.
- **Диск C: (INT 21h)**: в GUI на десктопе — своя папка `~/Documents/RustEmu8086/vdrive/C` (`App::drive_dir`,
  как `vdrive\C` Emu8086), опция `Settings::drive_c_source_folder` — папка программы; CLI (`Session::load_path`) —
  папка файла; веб — `MemDrive`. Буква диска в пути игнорируется.
- **Ассемблер**: формат — по `#make_xxx#`, иначе COM при `ORG 100h`, EXE при наличии сегментов, иначе COM.
  Переменные (`x db ..`) — память с типом; метки (`x:`, в т.ч. `x: db ..`) — непосредственные адреса.
  Дальний Jcc → автоматически `J!cc +3 / JMP near` (как Emu8086). `shl ax, 3` → 3×`shl ax,1`.
  Защита от «бомб»: сегмент (и COM) ≤ 64 КБ — ошибка на строке, выходящей за конец (`SEG_LIMIT`); `DUP` больше
  64 КБ отвергается до построения; цикл `INCLUDE` — ошибка (стек открытых файлов, вложенность ≤ 32).
- **Устройства** тикают порциями по 128 инструкций, время — по часам хоста (`DeviceClock`, см. ниже).
  **Порты устройств Emu8086**: 4 — светофор (word), 7 — шаговый двигатель, 9/10/11 — робот,
  125/127 — термометр/нагреватель, 130 — принтер, 199 — LED-дисплей. Плюс VGA DAC 3C8h/3C9h, 3DAh, 60h.

## Совместимость с Emu8086 (сверено с оригиналом)
Установка Emu8086 пользователя (CrossOver): `~/Library/Application Support/CrossOver/Bottles/Assembly/drive_c/emu8086/`
(`examples/`, `inc/emu8086.inc`, `documentation/*.html`, исходники устройств на VB в `devices/DEVELOPER/sources/`).
Используется как справочник. Исключение — примеры: папка `examples/` репозитория — 84 файла из `examples/` Emu8086
без изменений (решение владельца 2026-10-30; `thermometer.asm` перекодирован из cp1252 в UTF-8, CRLF сохранены) +
наши `hello_nasm.asm`/`boot_nasm.asm`. Остальное (документацию, устройства, `emu8086.inc`) не копировать.
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
  сверяются с NASM, если он установлен; `nasm_dialect`: NASM-программы целиком против `nasm -f bin`), сквозные тесты `rustemu-session/tests/run.rs` на наших программах `tests/programs/` (их же грузят UI-тесты:
  `rustemu_ui::testing::PROGRAMS`, `app_with_example(i)`).
- **Корпус Emu8086**: `cargo test -p rustemu-session --test emu8086_corpus -- --nocapture` — собирает и
  прогоняет все примеры `examples/` (84 из Emu8086 + 2 NASM), плюс загрузка micro-os с виртуальной дискеты. Сейчас: всё собирается и выполняется без ошибок
  (кроме `fasm_compatibility.asm` — синтаксис FASM не поддерживается).
- **Курс** (`course/` — только локально, в .gitignore, на GitHub не отправлять; без папки тест пропускается; уроки по-русски, программы `course/programs/*.asm` со строками проверки `;! input/output/screen/endless`):
  `cargo test -p rustemu-ui --test course` запускает программы и сверяет ссылки «▶ Открыть в эмуляторе» (`share::fragment`)
  и тексты программ в уроках (после `<!-- program: имя -->`); `COURSE_UPDATE_LINKS=1` — обновить их.
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
cargo run -p rustemu-cli -- run examples/hello.asm --input x
cargo run -p rustemu-cli -- run examples/calc.asm --input '12\n30\nx'
cargo run -p rustemu-cli -- asm examples/exetest.asm -l -s
cargo run -p rustemu-desktop        # GUI (можно передать путь к .asm)
cargo run -p rustemu-tablet         # интерфейс планшета в окне на Mac
cargo test -p rustemu-desktop -p rustemu-tablet  # UI-тесты + PNG-снимки в target/ui-snapshots/
./scripts/bundle_macos.sh        # RustEmu8086.app
cd crates/rustemu-web && trunk serve   # веб-версия на http://127.0.0.1:8080 (wasm32-unknown-unknown + trunk)
```

## План работ (статус: [ ] — не начато, [~] — в работе, [x] — готово)
Найденные проблемы, недочёты и идеи фич — [TODO.md](TODO.md).

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
- [x] 2.7 Сообщения об ошибках: строка и место в ней (`AsmError::col`/`len`, символы). `errpos.rs`: по сообщению и
      исходной строке (`Preprocessor::sources`) — имя в кавычках, первый/последний/все операнды, иначе оператор;
      в развёртке макроса — вызов целиком; синтаксические — токен из лексера, если строка не изменена препроцессором.
      GUI: `(строка:колонка)` в сообщениях, волнистое подчёркивание (`App::error_spans`), клик выделяет
      (`App::jump_to_message`)
- [x] 2.9 Диалект NASM (`-f bin`): автоопределение (`detect_dialect`; не собралось — `assemble_dialect` пробует другой
      диалект, ошибки — определённого) / `#nasm#` / `.nasm` / настройка; COM / BOOT / BIN по ORG;
      тест `nasm_dialect` сверяет программы с настоящим `nasm -f bin` байт в байт. Есть `%define` с параметрами
      (`source.rs`: `function_define`/`expand_functions`, раскрытие при чтении строки), `struc`/`istruc`/`at`/`iend` (→ `equ`
      полей и `times … db 0`), операторы с порядком NASM (`expr.rs`: `nasm_cond` — `?:`, `|| ^^ &&`, сравнения = 1, `| ^ &`).
      Нет: EXE, `%0`/`%rotate`/greedy-параметры, `%strlen`, `incbin`, float, контекстные макросы; дальний Jcc — пара 8086
- [ ] 2.8 Недостающее: `STRUC`, `IRP/IRPC`, `ASSUME`-автопрефиксы, `.binf`-файл — проверить,
      что поддерживает Emu8086

### Этап 3. BIOS/DOS HLE и загрузка программ
- [x] 3.1 IVT + ROM-заглушки (BOP), BIOS Data Area
- [x] 3.2 Загрузчики COM/EXE/BIN/BOOT, PSP
- [x] 3.3 INT 10h: 00,01,02,03,05,06,07,08,09,0A,0C,0D,0E,0F,10h(DAC),13h; режимы 0-3, 7, 13h
- [x] 3.4 INT 16h, 1Ah, 15h, 11h, 12h, 13h (дискеты FLOPPY_0..3 в `Machine::floppies`, DL = номер дисковода; INT 11h и INT 13h/08h сообщают их число;
      «create new floppy drive» — `Machine::add_floppy`; образы в памяти, загружаются/сохраняются явно; запись на дискету —
      файла или загруженной программы (`Session::loaded_bytes`, работает в вебе); `Session::boot_from_floppy` оставляет
      `program`, если сектор совпадает с собранной BOOT-программой (`App::boot_from_floppy` не сбрасывает исходник))
- [x] 3.5 INT 21h: консоль, 0Ah, векторы, дата/время, файлы (3Ch-42h, 39h-3Bh, 41h, 43h, 47h, 4Eh/4Fh, 56h),
      память 48h-4Ah, 4Ch; INT 20h; INT 33h (базово)
- [x] 3.6 Таймер: счётчик тиков BIOS 0040:006C идёт по часам хоста 18.2 Гц (или `instructions_per_tick` —
      детерминированно, для тестов); опционально IRQ0 → ROM-обработчик INT 08h (F000:E000) → INT 1Ch
      (галочка «timer IRQ» в эмуляторе, `--timer-irq` в CLI). Весь сегмент F000 выполняется «атомарно» в шаге.
- [ ] 3.7 Графические режимы CGA 4/5/6 и VGA 12h (если используются примерами Emu8086)

### Этап 4. CLI (`rustemu-cli`)
- [x] 4.1 `rustemu asm` (-l листинг, -s символы), `rustemu run`, `rustemu disasm`
- [x] 4.2 Интеграционные тесты на примерах

### Этап 5. GUI (eframe/egui 0.37)
`rustemu-ui`: `app.rs` (состояние `App`, `Settings`; команды — в `app/`: `files`, `session`, `build` — compile/emulate,
`exec` — шаги и цикл выполнения, `emulator_cmds`, `shortcuts`, `storage` — persist), `windows.rs`
(общие диалоги и инструменты), `workspace.rs` (панели / классические окна), `editor.rs` + `highlight.rs` (`Highlighter` — подсветка по строкам с кешем между кадрами), `screen.rs`, `debug.rs` (+ начало дизассемблера
`disasm_start`), `devices_ui.rs`, `examples.rs` (встроенные `examples/*.asm`, меню `MENU` — пункты Examples Emu8086), `help.rs`, `reference.rs` +
`reference_ui.rs`, `debugger.rs`, `fonts.rs`. `rustemu-desktop`: `ui.rs` (меню, тулбар, сообщения, статус),
`emulator.rs` (окно эмулятора и исходника), `ui_tests.rs` (kittest, PNG-снимки в `target/ui-snapshots/`).
`rustemu-tablet` — см. 6.7.
- [x] 5.1 Редактор: подсветка, номера строк, отметки ошибок, переход к строке по клику на ошибку,
      открыть/сохранить/недавние, примеры, compile (⌘B, пишет .com/.exe + .lst + .symbol), emulate (F9)
- [x] 5.2 Окно эмулятора: редактируемые регистры (изменённые — красным), дизассемблер от CS:IP,
      Load/Reload/Step back/Single step/Step over/Run/Stop, задержка шага, точки останова (по строкам,
      переживают перекомпиляцию), окно исходника с текущей строкой (внутри BIOS — строка вызывающего INT)
- [x] 5.3 Экран: текст 80x25/40x25 с CGA-цветами и курсором (растровый шрифт 8×16 CP437), 13h через текстуру и палитру DAC,
      клавиатура (символы, Enter/Esc/Backspace/Tab/стрелки/F1-F10/Ctrl+буква), мышь для INT 33h
- [x] 5.4 Окна: память (hex-дамп, переход по адресу/регистру), стек, флаги (редактируемые + анализ условий
      переходов), переменные (hex/dec/signed/bin/ascii), таблица символов, листинг, журнал
- [x] 5.5 Устройства (вкладки, выбор по `#start=..#`): светофор, шаговый двигатель, LED, термометр, принтер,
      робот (карта редактируется кликом)
- [x] 5.6 Примеры (все из Emu8086: меню Examples с подписями оригинала + NASM, *more examples...* — окно со списком
      `EXAMPLES` и описаниями `examples::description`, `windows::examples_menu`; `examples::index` — без учёта регистра,
      старые имена → `FORMER`; ключ сессии примера — `EXAMPLE_KEY` = `emu8086-example:`, старые `example:` не применяются) и справочник на английском и русском (`reference::Lang`, `Settings::reference_lang`, переключатель EN / RU;
      русские тексты — `reference/ru_{instructions,directives,interrupts}.rs` по имени / номеру статьи, `ru.rs` — поиск
      с запасным английским, `help::HELP_RU`; тест: перевод есть у каждой статьи, код русских примеров = английскому) (`reference.rs` — поиск и темы, данные — `reference/{instructions,directives,interrupts}.rs`, `reference_ui.rs` — окно): все инструкции
      (синтаксис, флаги ODITSZAPC, пример), директивы, прерывания BIOS/DOS по функциям, обзор синтаксиса NASM
      (`NASM_GUIDE`, вкладка *NASM syntax*, `Topic::Nasm`; примеры распознаются как NASM — тест); из редактора —
      подсказка при наведении, F1/⌘-клик открывает статью (для `INT nn` функция берётся из `mov ah, ..` выше).
      Тест: все примеры справочника собираются, все мнемоники ассемблера описаны
- [x] 5.7 `scripts/bundle_macos.sh` → `target/release/bundle/RustEmu8086.app` + `RustEmu8086-<версия>-macos.zip`,
      ad-hoc подпись; `UNIVERSAL=1` — arm64 + x86_64 через `lipo` (иконка: `cargo run -p rustemu-ui --example make_icon`)
- [x] 5.8 Редактор: поиск/замена (⌘F, ⌘G/⇧⌘G), переход к строке (⌘L), комментирование блока (⌘/) — `find.rs`
- [x] 5.10 Прозрачность макросов: `Line/LineInfo.origin` — строка в определении макроса; окно исходника
      показывает код `emu8086.inc` при шаге внутри макроса/процедуры (плашка + строка вызова), подсказки
      и ⌘-клик в редакторе (`rustemu_asm::library::index`), окно библиотеки (`library_ui.rs`), имена меток
      в дизассемблере (`Session::symbol_at`; LOCAL-метки называются `name??NNNN`)
- [x] 5.12 Отладка на уровне исходника: `Session::view_at` (ленивый вход в макрос: на первой инструкции
      развёртки показывается строка вызова; `LineInfo.chain` — вся цепочка вложенности), Step Over/Out
      (`Session::step_goal`/`run_step_goal`, глубина по CALL/INT↔RET/IRET), кнопка Step out (⇧F11)
- [x] 5.13 Интерфейс как в Emu8086: меню File/Edit/Bookmarks/Assembler/Emulator/Math/ASCII Codes/Options/Help (Options — `windows::options_menu`:
      окна, синтаксис, тема, all options…; и в меню ⋮ планшета),
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
- [x] 5.18 Два режима окон (`workspace.rs`, `Settings::layout`, options → windows): **классические окна Emu8086** (по умолчанию — минимум отличий от оригинала) и **панели** (`egui_tiles` —
      `egui_dock` для egui 0.37 нет). Флаги `App::windows` (+ `library.open`, `reference.open`)
      — единственный источник «что открыто»: в панелях `workspace::sync` добавляет вкладку в группу инструмента (`Group`:
      Text/Debug слева, Emulator/Inspect (memory, watch, ALU) посередине, Screen/Aside справа — под экраном vars, stack,
      debug log, log, устройства, диск; нет группы — новая колонка или половина колонки
      по раскладке по умолчанию), крестик снимает флаг. Окна-инструменты: `xxx_window` не рисует окно при `App::panels()`,
      тело — `xxx_ui(app, ui)`, вызывается из `workspace::tool_ui` (эмулятор и исходник — из `rustemu-desktop`, `emulator::tool_ui`).
      Диалоги, flags (+ flag analyser) и таблица ASCII всплывают в обоих режимах; планшет — всегда окна.
      Дизассемблер листается колесом и полосой справа от списка (`App::scroll_disasm`/`show_disasm_at`, в пределах CS;
      назад — декодирование с 256 байт раньше), до следующего шага или ⌖ CS:IP / F7. Раскладка — `App::panels`, в настройках строкой RON
      (`Settings::panels`, нечитаемая отбрасывается); `workspace::reset_layout` (Emulator → Reset window layout)
- [x] 5.17 Дизассемблер различает данные (`Session::code_row`, настройка `Settings::disasm_data`, галочка **data as DB**):
      строки и переменные — `DB 'Hello', 0Dh, 0Ah, '$'` с именем переменной (`symbol_at` знает и переменные), нули после
      программы — `DUP`; после завершения программы — плашка вместо дизассемблера (CS:IP уже в PSP). Консоль `u` — как DEBUG, без этого
- [x] 5.16 Меню окна эмулятора (Emu8086): file (load executable, reload, examples, reset emulator and ram —
      `Session::unload`, command line, save/load the emulator's state — `Machine/Session::save_state` в памяти, без
      открытых файлов хоста, недавние файлы, close), math, debug (шаги, stop on condition, run until / set break point /
      set CS:IP по выделенной строке дизассемблера, show current break point / instruction). Клавиши — наши; из Emu8086
      добавлены не конфликтующие F6 (step back), F7 (current instruction), ⌃F8 (run until), ⌃⇧F7 (set CS:IP)
      Далее: view (окна; extended value viewer — регистр/слово памяти во всех системах, правка выражением,
      двойной клик по имени регистра; F11 — журнал), virtual devices (имена exe Emu8086 + вкладка VGA state:
      порт 3DAh и палитра DAC), virtual drive (общий с главным меню; запись .bin с цилиндра/головки/сектора —
      `Session::write_floppy`), help (справочник, страница релизов, about). Меню external не делаем

### Этап 6. Полировка
- [x] 6.1 Сравнение с Emu8086: корпус из 84 оригинальных примеров (сборка + выполнение), устройства по
      исходникам оригинала, параметры командной строки, виртуальная дискета, шаблоны New
- [x] 6.7 Интерфейс планшета — крейт `rustemu-tablet`, работает в веб-версии (iPad, сенсорные экраны, узкое
      окно) и окном на Mac для разработки. **Нативные iPadOS/Android убраны** (решение 2026-10-28: без Apple
      Developer ID; код — в локальной ветке `archive/native-tablet`, на GitHub не отправлять). Модули: `lib.rs`
      (`TabletApp`, `TabletState`, `tablet_ui`), `theme.rs` (тёмная тема через `App::fixed_theme`), `widgets.rs`
      (иконки, кнопки, карточки), `bars.rs` (верхняя панель Run/Pause/Step/Reset, вкладки, меню ⋮, статус),
      `tabs.rs` (раскладка Editor / Debugger / Memory: альбомная — три колонки на всю высоту, `three_columns`;
      портретная — две колонки и нижний ряд; узкое окно — стопкой, `left_width`), `cards.rs`, `memory.rs`
      (`MemoryView`: hex-редактор, инспектор, сегменты, стек, карта), `keypad.rs` (экранная клавиатура: US +
      Esc/Tab/Ctrl/стрелки/F1–F10/Ins…PgDn, Shift/Ctrl на одну клавишу, открывается при `is_waiting_input`;
      рядом экран `screen::Fit::Cursor`), `tests.rs` (снимки `30_tablet_*`, `31_tablet_keyboard*`). Трасса —
      `Machine::history_cpus`. Платформы в `rustemu_ui::platform` — только desktop (`rfd`) и wasm
- [~] 6.8 Веб-версия (`rustemu-web`, trunk → GitHub Pages, задания `web`/`deploy-web` в CI): WebGPU с откатом на
      WebGL 2 (`rustemu_ui::wgpu_options`); время — `web-time` (std `Instant` в браузере паникует), часовой пояс —
      `js_sys::Date`; файлы — `platform::request_upload`/`take_uploads` (rfd `AsyncFileDialog`) и `download`;
      `App::doc_name` — имя файла без пути, `Session::load_named` — загрузка из байтов. Диск C: в браузере —
      `rustemu_core::memdrive::MemDrive` (`Machine::set_dos_memory_drive`; в `dos.rs` диск — `DosDrive::Host`/`Memory`,
      открытый файл — `OpenFile`), хранится в eframe storage (`App::persist`/`restore_drive`, base64), окно
      `drive_ui.rs` (загрузка, просмотр, скачивание). «Мои программы» — `programs.rs` (`App::browser` вместо
      `is_web()`, чтобы тестировать нативно; Save → `save_program`/диалог имени `SaveName`, Open → окно, загрузка
      исходника → `import_program`; черновик редактора `Draft` восстанавливается при следующем визите). `App::persist`
      не пишет неизменённые значения (`App::put`; записанное читается обратно — при переполнении квоты
      ключ в `unsaved_keys`, повтор при следующем сохранении, одно предупреждение); веб сохраняет каждые 10 с и сам — на `visibilitychange`/`pagehide`
      через `WebRunner::app_mut` (eframe 0.37 слушает несуществующее `onbeforeunload`); без localStorage — notice и
      вопрос перед закрытием. Экран текстового режима — растровый шрифт 8×16 (`font::glyph8x16`, Unscii 16, public
      domain; 4 глифа нарисованы) через атлас с «sharp bilinear» (`screen.rs`). Не сделано: PWA/офлайн, ☻☼⌂ⁿ в
      окнах с текстом (таблица ASCII, дамп памяти). Ссылки «поделиться» — `share.rs`: `#name=…&code=<deflate
      (miniz_oxide), base64url>[&run]`, окно *share link* (File → Share link…, и на Mac — ведёт на `WEB_URL`);
      веб открывает ссылку при запуске и на `hashchange` (`App::open_link` → `PendingAction::Shared`, спрашивает про
      несохранённое) и убирает её из адреса (`history.replaceState`). Программа из ссылки, пока не сохранена
      (`App::from_link`, флаг в `Draft`), работает с пустым временным диском C: (`App::use_temporary_drive`; свой —
      в `App::own_drive`, его и сохраняет `persist`), уведомление при первом запуске и в окне drive C:. Диск в памяти —
      не больше `memdrive::CAPACITY` (2 МБ; `MemDrive::write_at` — неполная запись, как DOS на полном диске) и 1024 записей. Размер загрузки: профиль cargo `web` (`opt-level="z"`, fat LTO,
      `codegen-units=1`; `rustemu-core` — `opt-level=3`: с "z" ядро на ~40% медленнее, с LTO и 3 — быстрее прежнего),
      `data-cargo-profile="web"` в `index.html`; эмодзи-шрифты egui в вебе — подмножества (`rustemu-ui/fonts/*-subset.ttf`,
      `scripts/subset_emoji_fonts.py`, тест `fonts::tests::web_fonts_cover_the_ui`; на десктопе `monochrome_emoji_fonts`
      включает `rustemu-ui` только для нативной цели). Итого wasm 10,3 → 7,2 МБ, gzip 4,0 → 2,7 МБ. Прогресс загрузки —
      initializer trunk `progress.mjs`; trunk передаёт размер wasm до wasm-opt, хук `wasm_size.sh` (Trunk.toml) ставит настоящий
- [ ] 6.6 Синтаксис FASM (`#fasm#`) — не поддерживается
- [x] 6.2 Настройки и сессия (`Settings` через eframe persistence): тема, шрифт, задержка (размер экрана — размер окна screen: `Fit::Whole`, по центру; зума нет),
      открытые окна (положения хранит egui; окна программы — эмулятор, экран, память… — при запуске закрыты, как в
      Emu8086, и открываются при первом emulate: `App::deferred_tools`, `Windows::take_program_tools`), галочки отладки; сессия файла (`FileSession`, последние 30, ключ —
      путь или `example:имя`): точки останова с условиями (в т.ч. в `emu8086.inc` — по имени файла, `pending_breakpoints`),
      watch, закладки, stop on condition, строка курсора. Буфер обмена: фича eframe `clipboard` (⌘C/⌘V во всех
      полях), экран эмулятора — вставка как ввод с клавиатуры и копирование текста экрана
- [~] 6.3 Производительность (release, M-серия): ~50–55 млн инстр/с без истории, ~32 млн с историей шага назад
      (лёгкий снимок CPU+память на каждую инструкцию; устройства, DOS, очереди и дискета — только для IN/OUT, INT,
      исключений, IRQ и кода BIOS, см. `Machine::may_touch_system`)
- [x] 6.4 `.startup`/`.exit` (MASM 6), корпус примеров Emu8086
- [x] 6.5 Git: ветка `main`, CI — `.github/workflows/ci.yml` (macOS: fmt, clippy -D warnings, тесты с NASM; задание
      `bundle` на `main` — универсальный .app в артефактах). `audit.yml` — `cargo audit` (база RustSec) при изменении
      `Cargo.toml`/`Cargo.lock` и раз в неделю. Вики — страницы в `wiki/` (русский, для пользователей), `wiki.yml`
      копирует их в вики GitHub при push в `main` (вики должна быть создана один раз вручную); меняя поведение — правьте и вики. `release.yml`: тег `vX.Y.Z` (= версии в Cargo.toml) →
      GitHub Release с zip, описание — `release-notes/vX.Y.Z.md` (если есть). Без Developer ID и нотаризации (в README — как открыть при первом запуске)
