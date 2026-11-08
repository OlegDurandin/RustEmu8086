# RustEmu8086 — аналог Emu8086 для macOS

Цель: полноценный нативный аналог Emu8086 (редактор + ассемблер + эмулятор 8086 + отладчик +
виртуальные устройства) для macOS, написанный на Rust.

## Архитектура

Cargo workspace (корневой `Cargo.toml` — виртуальный манифест):

| Крейт | Назначение |
|---|---|
| `crates/rustemu-target` | Граница архитектуры, без зависимостей: трейт `Target` (шаг/run/шаг назад, точки останова, память по физическим адресам, дизассемблер, `flow_at`, `call_stack`, `system_caller`, таблица регистров `registers()` для выражений, `data_address`, `code_segment`, снимки `State`), `Arch`, `Event`, `Flow`, `Frame`, `OutputBuffer` (вывод программы: последний ~1 МБ, длина — всё записанное). Через него session и общий UI работают с любой машиной; специфичное — downcast `as_any`. |
| `crates/rustemu-core` | Эмулятор: CPU 8086 (`cpu.rs`), шина/память 1 MiB с журналом (`bus.rs`), `Machine` (загрузчики, шаг/run/шаг назад, точки останова — `machine.rs`), BIOS (`bios.rs`: INT 10h/13h/15h/16h/1Ah/33h), DOS (`dos.rs`: INT 21h, файлы в песочнице), видео (`video.rs`), порты и виртуальные устройства (`devices.rs`), шрифт/CP437 (`font.rs`), скан-коды (`keyboard.rs`), дизассемблер (`disasm.rs`), сопроцессор 8087 (`fpu.rs`), журнал эмулятора (`log.rs`: повторы считаются, ≤ 1000 записей, шаг назад по `Mark`). `target.rs` — `impl Target for Machine` (+ `bios_caller`, `call_frames`, `flow_at`, `Checkpoint`, `AsMachine::x86()`). Без зависимостей от GUI. |
| `crates/rustemu-mips` | Ядро MIPS32 по эталону MARS (этап 7): `settings.rs` — настройки MARS (`Settings`: отложенные переходы, самомодифицирующийся код, старт с `main`, псевдоинструкции, предупреждения как ошибки, `MemoryConfig`: Default / CompactDataAtZero / CompactTextAtZero; `mars_options` — ключи MARS `db smc sm np we mc`) и `Layout` — адреса конфигурации с действующими границами сегментов, посчитанными как `Memory.initialize` MARS (знаковые int Java, таблицы 4 МБ и 64 КБ MMIO; MMIO — до 0xffffffff не включительно); ассемблер пишет настройки в `Image::settings`, `Machine::load` настраивает по ним память (`Memory::layout`, `self_modifying`) и процессор (`Cpu::delayed_branching`), так что программа идёт с теми, с которыми собрана; `memory.rs` — разреженная память страницами 4 КБ (двухуровневая таблица: блок 4 МБ → страница, слово — один поиск), сегменты и проверки доступа по `Layout` (по умолчанию text 0x00400000, data 0x10000000, стек до 0x7ffffffc, ktext/kdata, MMIO), при самомодифицирующемся коде text — словами, как в MARS (запись байта/полуслова заменяет слово и делает его инструкцией — участок кода, откат шагом назад — `Memory::truncate_code`; не по адресу слова — ошибка загрузки «fetch address for text segment not aligned…»), сообщения ошибок MARS (kernel text — его «DEVELOPER: You must use getStatement()/setStatement()…»), журнал для шага назад, `fetch` (нет кода — `Step::DroppedOff`, как «dropped off bottom»); `cpu.rs` — 77 целочисленных инструкций MARS, отложенные переходы (`Delay` — автомат `DelayedBranch` MARS: переход регистрирует цель, `Cpu::end_step` в конце шага её взводит, после слота — переходит; переход в слоте сохраняет первую цель и выполняет её шагом позже; исключение в слоте с обработчиком — переход всё равно срабатывает; адрес возврата — за слотом; `jalr rd, rs` — переход по `rs` после записи `rd`, как в MARS; конец шага `syscall` — в машине, после сервиса), HI/LO, CP0 (8/12/13/14), последняя операция АЛУ (`Cpu::alu`: `AluTrace` — операнды до, результат после,
выбор инструкции по слову в `Cpu::step`, запись в `$zero` не считается; окно ALU — `mips_ui::alu_ui`), исключения как в MARS (Cause/EPC/BadVAddr/EXL; обработчик 0x80000180, если там есть код), `syscall` → `Step::Syscall`. `fpu.rs` — сопроцессор 1 как в MARS: `Cpu::fpr` (32 слова) и `Cpu::fcc` (8 флагов условий), double — чётный регистр и следующий (младшее слово в чётном), NaN каноничны (`f32_bits`/`f64_bits`, как `Float.floatToIntBits`), `round.w` — половина к чётному, `round/trunc/ceil/floor.w` вне диапазона — 0x7fffffff, `cvt.w` — приведение Java, корень из отрицательного — NaN; нечётный регистр — ошибка MARS без кода исключения (`ExcCode::Runtime`: CP0 не меняется, обработчик 0x80000180 вызывается), `ldc1/sdc1` — выравнивание на 8. `machine.rs` — `Machine`: загрузка `Image` (куски кода/данных, проверка сегментов), шаг/run (`rustemu_target::Event`), блокирующий ввод как в 8086 (PC назад на `syscall`, пустые повторы не в истории), `sleep` (`Event::Sleeping`, `instant_waits`), история шага назад (2000 шагов и ≤ 64 МБ учтённых снимков и журналов — один буфер на все шаги, снимок хранит число своих записей; для `syscall` — длина консоли, только потреблённые байты ввода, куча и прежнее состояние одного генератора (до 4096 потоков); файлы — барьер; устройство MMIO — `DeviceState` и откат текста дисплея, только у шагов, которые их изменили); `mmio.rs` — Keyboard and Display MMIO Simulator MARS (`KeyboardDisplay`, всегда подключён: `0xffff0000..0c`; память отмечает чтение данных приёмника и запись в данные передатчика — `Memory::mmio`, после шага `after_instruction`: задержка передатчика в инструкциях — такт на каждую выбранную инструкцию, как наблюдатель text-сегмента MARS; DAD; form feed, bell — терминал строкой с переводами строк и кареткой, с причудами JTextArea; прерывания `ExcCode::KeyboardInterrupt/DisplayInterrupt` — Cause с битом 8/9, перед следующей инструкцией вместо неё; `Machine::mmio_key` пишет регистры мимо истории; задержка — `DelayTechnique` Fixed/Uniform/Normal, генератор `JavaRandom` с `next_gaussian` в состоянии устройства; `DigitalLab` — Digital Lab Sim: индикаторы `0xffff0010/11`, опрос клавиатуры `0x12` → `0x14`, счётчик `0x13`; как в MARS, видит каждое чтение и запись `0x10..0x13` (журнал `MmioTouch::lab`, значение — `char` Java) и выборки из `.text` (не `.ktext`): счётчик — 31 уведомление на прерывание, прерывания `TimerInterrupt`/`HexKeyboardInterrupt` (Cause бит 10/11, остаются установленными) — только при EXL = 0; `Machine::lab_click`); `dialog.rs` — диалоги 50–59 (`Dialog`/`DialogKind`/`DialogAnswer`): сервис без ответа ставит `Machine::dialog`, PC — назад на `syscall`, `Event::WaitingInput`; интерфейс показывает окно и зовёт `answer_dialog`, повторный шаг берёт ответ и пишет регистры/буфер как сервисы MARS (целое — `Integer.parseInt` без trim, строка — его цикл с `\n` и нулём, статусы 0/−1/−2/−3/−4, подтверждение 0/1/2/−1); `dialogs_in_console` (CLI) — сообщения в консоль, ответы — строки ввода; настройка `popup_input` — сервисы 5–8, 12 спрашивают в окне «MIPS Keyboard Input» (в консоль `**** user input : …`, закрытое окно — строка `uninitializedValue`, как у Swing); `Machine::program_arguments` — аргументы программы, при загрузке под стек как `ProgramArgumentList` MARS (`$a0` argc, `$a1` argv, `$sp`); `syscall.rs` — сервисы MARS 1–17, 30–36, 40–44, 50–59 с его сообщениями ошибок (ошибки файлов 13–15, как в MARS 4.6, — −1 в `$v0` без остановки, причина — `Machine::file_error`, её показывает Run I/O), ввод как в панели Run I/O (эхо, конец по Enter или по длине; `read char` — без Enter); печать float/double — `java_float`/`java_double` (как `Float/Double.toString`); диалоги 50–59 — ошибка; `console.rs` — текстовая консоль вместо B800 (байты, ≤ 1 МБ, показ UTF-8 / Latin-1); `files.rs` — песочница (папка или память, без `..`/абсолютных/ссылок наружу, чтение порциями до 64 КиБ, запись до 16 МиБ за syscall и квота 16 МБ записи за запуск (до сборки буфера), до 1024 новых файлов за запуск (существующие открываются и после лимита), память — до 2 МиБ и 1024 файлов/папок с учётом пустых папок C:, дескрипторы 3..31, флаги 0/1/9); `random.rs` — `java.util.Random` (последовательности как в MARS, и `nextFloat`/`nextDouble`). `asm/` — ассемблер диалекта MARS: `lexer.rs` (правила токенизатора MARS: унарный `+`/`-`, символьные литералы, `Integer.decode`; `%hi(x)`/`%lo(x)` — расширение, как в GNU as), `basic.rs` (155 базовых форм MARS — 87 целочисленных и 68 сопроцессора 1: пример + маска кодирования, сгенерировано из `InstructionSet.java`), `table.rs` (`data/PseudoOps.txt` MARS как есть — все формы псевдоинструкций, и FPU (`l.s`, `s.d`, `mfc1.d`…); выбор формы — первая подходящая по классам операндов, как `OperandFormat`; подстановка шаблонов — как `makeTemplateSubstitutions`; исправлен шаблон `swr $t1,100`, в MARS там `nop`), `mod.rs` (два прохода, директивы `.text/.data/.ktext/.kdata/.word/.half/.byte/.float/.double/.ascii/.asciiz/.space/.align/.extern/.globl/.eqv/.set`, автовыравнивание с переносом метки, ссылки вперёд; ошибки с текстами MARS; строки — UTF-8; результат `Program`: `Image`, `statements` со столбцом Basic, `data`, `symbols`, `warnings`). Макросы и `.include` — по `Assembler.parseLine`/`Macro`/`MacroPool`/`Tokenizer.processIncludes` MARS: строки программы после `.include` (`include_lines`: строка с `.include "файл"` заменяется целиком, путь — от папки включающего файла, повторное включение — «Recursive include»), макрос — номера строк тела (`Macro`), выбор по имени и числу аргументов (последний определённый), подстановка текстом как `getSubstitutedLine` (токены с конца, первая вхождение), метки тела `label_M<n>`, петля — по индексу строки вызова; `.eqv` в теле — при определении; место строки — `Place` (файл, строка, вызовы): у `Statement`/`DataLine` — `file`, `origin`, `chain`, `expanded` (`<13> текст`, как колонка Source MARS), у `AsmError` — `file`, `expansion` (`line 21->13`); общие бюджеты: `MAX_EXPANDED_LINES` 250 000 на исходные и генерируемые строки, `MAX_SOURCE_BYTES` 4 МиБ, `MAX_EXPANDED_BYTES` 16 МиБ, до 1024 файлов, путь до 4 КиБ, строка до 64 КиБ / 4096 токенов, до 1 млн обработанных токенов, `MAX_NESTING` 64; исходники проверяются перед копированием/удержанием включений, макроподстановки и `.eqv` — до роста. `MAX_DIAGNOSTICS` 100 (включая итоговую ошибку усечения), UI ограничивает отображение независимо. `read_source_file` — ограниченное чтение загрузчиками CLI/UI; `assemble_files_cancellable` — атомарный флаг отмены для вызывающего worker (GUI ещё синхронный). `assemble_with(name, source, read)` → `AsmErrors { errors, files }`; `Program::files` — `SourceFile`, `listing()` по ним; `assemble_settings` — с настройками; `assemble_files(program, exception_handler, others, …)` — несколько файлов, как MARS (обработчик, программа, остальные; у каждого файла свои метки, `.eqv`, макросы, сегмент; `.globl` переносит метку в общую таблицу в конце файла — «declared global label but not defined», «already defined as global in a different file»; `.extern` — сразу в общую; ссылки вперёд в данных — сначала свой файл и общая, после всех файлов — общая; `.text`/`.data` продолжаются; `Program::files[0]` — всегда программа). `disasm.rs` — дизассемблер по той же таблице `BASIC`: первая форма, чьи фиксированные биты совпали (как MARS: 0 — `nop`), поля `f/s/t` → `Decoded { mnemonic, args: [Arg::Reg/Cp0/Fpr/Imm/Branch/Jump] }`; `Style::Mars` — колонка Basic (`addi $8,$9,-100`, ветвление — смещением), `Style::Named` — `addi $t0, $t1, -100`, цели адресами, логические константы в hex; `clz/clo` — с любым rt; не инструкция — `.word 0x…`. `target.rs` — `impl Target` (регистры `$zero..$ra`, `$0..$31`, PC/HI/LO, `$f0..$f31` с `FPR` и флаги условий байтом `FCC`; `flow` — `jal/jalr/bgezal/bltzal` вызов, `jr $ra` возврат; стек вызовов по `$ra` и стеку; снимки `Snapshot` без файлов; `Checkpoint { cpu }`; `AsMips` — `mips()`/`mips_mut()` у `dyn Target`). |
| `crates/rustemu-asm` | Ассемблер диалектов Emu8086/MASM и NASM (`Dialect`, `detect_dialect`, `assemble_dialect`; NASM: `parse_line_nasm`, `tokenize_nasm`, перевод `%`-директив в `Preprocessor::nasm_lines`, локальные метки `rename_local_labels`, секции .text/.data/.bss с выравниванием 4, `times`, строгий размер операнда `encode::set_strict_size`). MASM: `lexer` → `source` (INCLUDE, MACRO/LOCAL/ENDM, REPT, IF/IFDEF) → `parse` → `asm` (многопроходная сборка до сходимости, сегменты, данные, релокации, листинг) + `encode` (кодирование инструкций) + `expr`. Встроенная `inc/emu8086.inc`. |
| `crates/rustemu-session` | Связка ассемблер ↔ эмулятор через `pub target: Box<dyn Target>` (8086-машина — `Session::x86()`/`x86_mut()`, MIPS — `mips()`/`Session::new_mips`/`load_mips` в `mips.rs`: отладочная информация MIPS переводится в общий `rustemu_asm::Program` — строки и символы, размер переменной — единица `.byte/.half/.word`, исходная программа — `mips_program`; зависящее от процессора — `match target.arch()`; 8086-часть — `x86.rs`: форматы COM/EXE/BIN/BOOT, дискеты, символы seg:off, диск C: в памяти): загрузка `Program`/файлов, сопоставление CS:IP ↔ строка исходника, Reload, выражения отладчика (`expr.rs`: регистры, флаги, переменные, `[mem]`, операторы C/ассемблера), стек вызовов (`call_stack` — эвристический обход стека по CALL/INT), строки дизассемблера (`code.rs`: `Session::code_row` — данные программы как `DB`/`DW`/`DD` по `LineInfo::is_code`/`data_size`, нули вне кода — `DB n DUP (0)`; инструкция на CS:IP — всегда код). |
| `crates/rustemu-cli` | Бинарь `rustemu`: `asm`, `run` (headless, ввод через `--input` или stdin, `--regs` — регистры в конце), `disasm`. Процессор — `--arch`, иначе MIPS для `.s` и исходников для MIPS (`rustemu_session::detect_arch`); MIPS: `asm` — проверка, `-o` — дамп сегмента кода (`Program::text_bytes`), `-l` — `Program::listing`, `-s` — `symbols_text`; `run` — файлы в папке программы, ввод с `\n`, `--keys` — клавиатура MMIO, в конце — `[MMIO display]`; настройки MARS в `asm` и `run` — `--delayed-branching --self-modifying --start-at-main --no-pseudo --warnings-are-errors --memory <config> --project --exception-handler <file>`, в `run` — `--program-args <text>`, диалоги 50–59 — в консоль (`dialogs_in_console`); `disasm --arch mips [--mars]`. |
| `crates/rustemu-ui` | Общая часть GUI на `eframe/egui` 0.37 (рендерер wgpu): `App` (состояние, файлы, сборка, запуск, настройки; рабочие места процессоров — `app/arch.rs`), окна MIPS (`mips_ui.rs`; адреса — по `mips_ui::layout` загруженной программы), настройки MARS (`mars_settings.rs`: `Settings::mars`, меню settings окна эмулятора MIPS, Options → MARS settings, all options; меню не закрывается по щелчку — `menu_config`; программа собрана с другими — `changed_since_load` и плашка; аргументы программы, ввод в окне, файл обработчика и все файлы папки — `App::mips_project_files`, не в браузере), окно диалогов 50–59 и ввода — `mips_ui::dialog_window` (ответ на паузе завершает шаг), редактор, экран, инструменты (`tools.rs` — одна таблица всех окон, `Host` — свои инструменты интерфейса) и диалоги (`rustemu_ui::windows` раз в кадр), справочник, сервисы ОС — `platform.rs`, `native_options()`; фича `testing` — помощники UI-тестов (`testing.rs`). Своего `eframe::App` нет. |
| `crates/rustemu-desktop` | Интерфейс Emu8086: библиотека (`DesktopApp`, главное окно с меню/тулбаром — `ui.rs`, окно эмулятора и исходника — `emulator.rs`; UI-тесты `ui_tests.rs`) + бинарь `RustEmu8086` (macOS). |
| `crates/rustemu-tablet` | Интерфейс планшета (см. 6.7) — для веб-версии на iPad и сенсорных экранах: `TabletApp` + `TabletState`; бинарь `rustemu-tablet` — окно на Mac для разработки. Нативных сборок iPadOS/Android нет. |
| `crates/rustemu-web` | Веб-версия (см. 6.8): wasm-бинарь для trunk (`index.html`); `layout.rs` — `WebApp` с `DesktopApp` или `TabletApp`, переключение на лету (`FrontEnd::switch_layout`, выбор в localStorage); раскладка по умолчанию — iPad (`MacIntel` + касания), `pointer: coarse` или узкое окно; `?ui=`, `?example=…&run`, ссылки с программой `#name=…&code=…` (`rustemu_ui::share`). |

### Ключевые решения
- **Граница архитектуры** (подготовка ко второму процессору — MIPS). Session и общий UI (цикл выполнения, точки
  останова, watch/условия, стек вызовов, Step Over/Out, список дизассемблера, дамп памяти, переменные) обращаются
  к машине только через `Target`; окна 8086 (регистры Emu8086, флаги, FPU/ALU, экран B800/A000, устройства, диск и
  дискеты, консоль DEBUG.EXE, value viewer, CPU и Memory планшета) берут `Machine` через `x86()` и ничего не
  показывают для другого процессора. Новая архитектура = свой крейт с `impl Target` + ветки `match arch` в session
  + свои окна. Состояние «до шага» — `Exec::prev` (`Target::checkpoint`, у 8086 — `Checkpoint { cpu, fpu }`).
- **Рабочие места процессоров** (`rustemu-ui/src/app/arch.rs`). У каждой архитектуры своё рабочее место
  (`Workspace`: session, документ, редактор, сборка, отладчик, виды, окна и панели, статус); `App::switch_arch`
  останавливает выполнение, паркует текущее (`App::parked`) и возвращает/создаёт другое (первый раз — его
  `last_file` или первый пример). Настройки: 8086 — прежние поля, MIPS — `Settings::mips` (`ArchSettings`);
  запуск — всегда в 8086 (MIPS — переключателем, файлом, примером или ссылкой). Архитектура принадлежит документу: `FileSession::arch`, `StoredProgram::arch`,
  `&arch=MIPS32` в ссылке; без неё — текущая, если `detect_arch` (`rustemu-session/src/detect.rs`; регистры `$t0`/`ax`, `syscall`) не уверен в
  другой (тогда диалог «Which processor?», `Dialogs::open_as`); `enter_workspace_for` — общий вход для open,
  примеров, программ браузера, загрузок и ссылок. Окна — `App::has_tool` (только 8086: экран, устройства, диск,
  FPU, флаги, стек, DEBUG.EXE, журнал, value viewer, emu8086.inc; только MIPS — `Tool::RunIo` и устройства MMIO `Tool::MmioDisplay/Bitmap/DigitalLab`). Справочник — одно
  окно, показывает процессор рабочего места (`RefState::mips`, переключатель 8086 / MIPS32 в окне).
  Черновики браузера — `draft` / `draft_mips`; диск C: DOS — всегда в 8086-сессии (`x86_session`); программа MIPS в браузере
  получает его файлы при запуске, записанное возвращается туда (`App::sync_mips_files`, `Files::generation`).
- **Прерывания — «настоящие».** IVT в 0000:0000 указывает на заглушки `F000:1000+4*n`, заглушка —
  опкод `0F FF nn` («BOP») + `IRET`. На BOP `Machine::service` вызывает HLE-обработчик; изменённые
  статусные флаги копируются в FLAGS, сохранённые в стеке. Программы могут перехватывать векторы.
  `Machine::step` выполняет заглушку в том же шаге (шаг через `INT` = шаг через сервис BIOS/DOS).
- **Блокирующий ввод**: обработчик возвращает `Svc::Block`, IP откатывается на BOP, событие
  `Event::WaitingInput`; после `push_key` шаг повторяется. Повторные «пустые» опросы не пишутся в историю.
- **Видеопамять — в адресном пространстве.** Текст: `B800:0000`, режим 13h: `A000:0000`. Состояние видео —
  в BIOS Data Area (0040:0049 режим, 0040:0050 курсоры, …). GUI только рендерит память.
- **Шаг назад** (до 2000 шагов и `HISTORY_BYTES` = 64 МБ учтённых снимков и журналов — старые шаги уходят первыми): на каждую инструкцию — снимок `Cpu` + журнал перезаписанных байт памяти;
  устройства (журналы портов/палитры/карты робота), DOS, очереди клавиш и IRQ, мышь и дискета — только для
  инструкций, которые могут их менять (`Machine::may_touch_system`; если неожиданно изменили — барьер истории).
  Файловые операции хоста — тоже барьер: их нельзя откатить.
- **Загрузка**: PSP в `0700h`; COM в `0700:0100`; EXE — модуль с `0710h`, DS=ES=PSP; BIN — `#load_segment#`
  (по умолчанию `0100h`); BOOT — `0000:7C00`.
- **Диск C: (INT 21h)**: в GUI на десктопе — своя папка `~/Documents/RustEmu8086/vdrive/C` (`App::drive_dir`,
  как `vdrive\C` Emu8086), опция `Settings::drive_c_source_folder` — папка программы; CLI (`Session::load_path`) —
  папка файла; веб — `MemDrive`. Буква диска в пути игнорируется. Папка хоста — с пределами на запуск
  (`dos::HOST_WRITE_QUOTA` 16 МБ записи, файл до 16 МБ, `HOST_MAX_CREATED` 1024 созданных файла и папки; дальше —
  короткая запись / ошибка 5 и строка в log).
- **Ассемблер**: формат — по `#make_xxx#`, иначе COM при `ORG 100h`, EXE при наличии сегментов, иначе COM.
  Переменные (`x db ..`) — память с типом; метки (`x:`, в т.ч. `x: db ..`) — непосредственные адреса.
  Дальний Jcc → автоматически `J!cc +3 / JMP near` (как Emu8086). `shl ax, 3` → 3×`shl ax,1`.
  Защита от «бомб»: сегмент (и COM) ≤ 64 КБ — ошибка на строке, выходящей за конец (`SEG_LIMIT`); `DUP` больше
  64 КБ отвергается до построения; цикл `INCLUDE` — ошибка (стек открытых файлов, вложенность ≤ 32); развёртка
  препроцессора (REPT, `%rep`, макросы) — не больше `source::MAX_EXPANDED_LINES` = 250 000 строк, ошибка на внешней
  строке (тест `bombs`); выражения — глубина ≤ `expr::MAX_NESTING` = 64 и ≤ `MAX_OPERANDS` = 256 операндов в строке
  (и в выражениях отладчика), иначе ошибка, а не переполнение стека. Текстовые подстановки (`%define`, текстовый `EQU`)
  просматриваются повторно (`source::expand_text`, имя внутри своего текста не раскрывается, как в NASM): ≤ 64
  вложенных имён и ≤ 64 КБ на строку; `%xdefine` раскрывает значение при определении.
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
  сверяются с NASM, если он установлен; `nasm_dialect`: NASM-программы целиком против `nasm -f bin`; `no_panics`:
  ≈64 000 коротких строк из «опасных» токенов в обоих диалектах — ошибка или код, но не паника), сквозные тесты `rustemu-session/tests/run.rs` на наших программах `tests/programs/` (их же грузят UI-тесты:
  `rustemu_ui::testing::PROGRAMS`, `app_with_example(i)`).
- **Корпус Emu8086**: `cargo test -p rustemu-session --test emu8086_corpus -- --nocapture` — собирает и
  прогоняет все примеры `examples/` (84 из Emu8086 + 2 NASM), плюс загрузка micro-os с виртуальной дискеты. Сейчас: всё собирается и выполняется без ошибок
  (кроме `fasm_compatibility.asm` — синтаксис FASM не поддерживается).
- **Курс** — отдельный репозиторий github.com/OlegDurandin/rustemu-course (с 2026-11-05), клон — в `course/` (в .gitignore этого репозитория) или по пути `COURSE_DIR`; без него тест пропускается; уроки по-русски, программы `course/programs/*.asm` (8086, строки проверки `;! input/output/screen/endless`) и `*.s` (MIPS32, `#!`, `#! mars: db …` — настройки MARS):
  `cargo test -p rustemu-ui --test course` запускает программы и сверяет ссылки «▶ Открыть в эмуляторе» (`share::fragment`)
  и тексты программ в уроках (после `<!-- program: имя -->`); `COURSE_UPDATE_LINKS=1` — обновить их.
- **Сверка MIPS с MARS** (`rustemu-mips/tests/mars_compare.rs`): случайные программы (арифметика с переполнением,
  HI/LO, сдвиги, загрузки/сохранения с невыровненными адресами, `lwl/lwr/swl/swr`, переходы вперёд, ловушки) на ядре
  и в настоящем MARS — регистры, HI/LO, 256 байт `.data`, сообщение об исключении. Запуск:
  `MARS=<Mars.jar или папка со скомпилированным Mars.class> cargo test -p rustemu-mips --test mars_compare -- --nocapture`
  (`MARS_PROGRAMS`, `MARS_SEED`); без `MARS` тест пропускается. Сейчас совпадают все 500. Второй тест — сервисы
  без ввода (форматы печати, `sbrk`, `random` с зерном, код `exit2`) против вывода и кода возврата MARS; случайные
  программы — также с `j/jal/jalr/bgezal/bltzal/ll/sc`, и наш ассемблер должен собрать их в тот же код. Третий —
  ассемблер: каждая форма каждой инструкции со случайными операндами, машинный код и `.data` против `dump` MARS
  (`MARS_PROGRAMS` раундов; сейчас 60 раундов ≈ 19 000 кодирований совпадают). Любое изменение ядра, сервисов или
  ассемблера MIPS — прогонять. Четвёртый (`float_programs_match_mars`) — сопроцессор 1: регистры из особых значений (±0, ±∞, NaN, денормальные, половины, края 32-битного диапазона) и случайных битов, арифметика s/d, корни, преобразования, округления, сравнения в 8 флагов, условные пересылки и переходы, `mfc1/mtc1`, `swc1/sdc1`, изредка нечётный регистр — `$f0..$f31`, флаги, память и ошибка против MARS (сейчас 300 из 300). Пятый (`macros_match_mars`) — 22 программы с макросами и `.include` (причуды подстановки MARS, перегрузка, параметры SPIM, `.data` в теле, вложенные включения из подпапки и все сообщения об ошибках): код и `.data` или ошибки (файл, `line 21->13`, столбец, текст). Шестой — `tests/mmio_compare.rs`: настоящий класс `KeyboardAndDisplaySimulator` MARS через `tests/mmio/MmioHarness.java` (компилируется `javac`, окно не показывается; клавиши — после заданного числа шагов, размер терминала берётся у MARS): 20 программ (опрос, задержки, DAD, bell/form feed, прерывания дисплея и клавиатуры, без обработчика, Status без IE) — текст дисплея, регистры, Status/Cause/EPC, регистры устройства, ошибка (PC после ошибки не сравнивается: мы оставляем его на инструкции). Там же `digital_lab_matches_mars`: класс `DigitalLabSim` через `tests/mmio/LabHarness.java` (щелчки — после заданного числа шагов), 9 программ (опрос клавиатуры, прерывания клавиатуры и счётчика, обращения в обработчике, чтения и запись слова, пример `digital_lab.s`) — индикаторы, нажатая клавиша, регистры, CP0, байты устройства.
- **Настройки MARS** (`rustemu-mips/tests/settings.rs`): 17 программ — отложенные переходы (слот, `jal`, переход и исключение в слоте, `syscall` в слоте, `nop` псевдоинструкций), самомодифицирующийся код (запись и чтение text, байт не по адресу слова, код в данных), старт с `main`, запрет псевдоинструкций, предупреждения как ошибки, обе компактные конфигурации — с ожиданиями по исходникам MARS; `settings_match_mars` гоняет их же в MARS с ключами настроек, `mars_compare::delayed_branching_matches_mars` — случайные программы с `db` (сейчас совпадают 17 из 17 и 500 из 500); `program_arguments_match_mars` — аргументы `pa` (длинные, компактная память). `tests/projects.rs` — программы из нескольких файлов, `projects_match_mars`: код, данные и ошибки против `Mars a main.s lib.s …` (7 из 7). `mars_compare::input_services_match_mars` — сервисы 5–8 против MARS с stdin (`nc`, строка на чтение; 15 наборов, в т.ч. hex-ввод, субнормальные числа и округление вывода): там, где режимы MARS совпадают (символ и строка длиннее буфера в `nc` и в Run I/O читаются по-разному, у нас — как Run I/O). `tests/dialogs.rs` — диалоги 50–59 по исходникам MARS (окна Swing тестом не управляются).
- **Корпус QtSpim ffcabbar** (`rustemu-mips/tests/ffcabbar_corpus.rs`): исходники из
  `https://github.com/ffcabbar/MIPS-Assembly-Language-Examples`, commit
  `de9c21ea0429f9d0eb840b6806f15acc24d42b21`, в `testdata/mips-ffcabbar/` (не в git) или `FFCABBAR_DIR`.
  Запуск: `MARS=<Mars.jar или папка Mars.class> cargo test -p rustemu-mips --test ffcabbar_corpus -- --nocapture`.
  Без корпуса пропускается; без MARS проверяются ожидаемые результаты. 18 файлов + фрагмент README: без изменений
  12 завершаются, 2 требуют входа с `main`, 4 не собираются в диалекте MARS, в README пропущена точка перед `data`.
  Отдельно — 52 сценария после минимальной адаптации синтаксиса и входа (`.globl main`, `sm`), оригиналы не меняются.
  Вывод без эха ввода, все 32 регистра и 320 байт данных совпали с MARS
  `ac3d34ea65c3b8349c4f6c00306bade9e3fec5c9` (2026-11-06).
  Ошибки самих примеров сохраняются: №4 дважды печатает остаток; №12 неизвестная операция попадает в сложение;
  №14 уменьшает стек дважды; №18 сортирует шесть слов вместо пяти, затягивая ноль из-за границы массива.
- **Сверка MIPS с binutils** (`rustemu-mips/tests/gnu_compare.rs`, как `nasm_compare`): все 155 базовых форм MARS (и сопроцессора 1: флаги условий — `$fccN`, double — чётные регистры) со
  случайными операндами — наш ассемблер против `mipsel-linux-gnu-as` (+ `ld` по адресу 0x00400000, `objcopy`), слово в
  слово; запускается, если binutils в PATH (`brew install mipsel-linux-gnu-binutils`, есть в CI), `GNU_ITERATIONS` —
  экземпляров на форму (сейчас 400 ≈ 35 000 инструкций совпадают). Отличия MARS от GNU проверяются как MARS: `clz/clo`
  с rt = 0, `break n` — 20-битный код (в GNU это `break n>>10, n&1023`); в исходнике для GNU `div rs,rt` пишется как
  `div $zero,rs,rt`, регистр CP0 — номером; непредсказуемые по MIPS32 комбинации (`jalr rd,rd`, `jalr $ra`,
  `bgezal $ra`) GNU не кодирует — генератор их обходит. Выполнение сверяется с MARS (выше): `qemu-mipsel` (user mode)
  на macOS нет.
- **Дизассемблер MIPS** (`rustemu-mips/tests/disasm.rs`): для всех 155 форм со случайными операндами текст `Style::Mars`
  = колонке Basic ассемблера (синонимы: 0 — `nop`, `jalr $31,…`, `break 0`); с `mipsel-linux-gnu-objdump` (`-z -M
  no-aliases,gpr-names=numeric,cp0-names=numeric`) — мнемоника и значения всех операндов (≈ 3500 инструкций), с
  поправками на запись objdump (`neg` = `sub` от `$0`, `div $0,rs,rt`, `break a,b` = 20-битный код, `jalr rs`).
- **SingleStepTests 8088** (эталон с реального железа, ~2.9 млн тестов): данные в `testdata/8088/`
  (в .gitignore; скачать `v2/*.json.gz` + `metadata.json` из github.com/SingleStepTests/8088), запуск:
  `cargo test -p rustemu-core --release --test singlestep -- --ignored` (`SST_FILTER=F6` — подмножество).
  Сейчас проходят все 2 927 000.

## Соглашения
- Rust edition 2024, `cargo fmt`, `cargo clippy --workspace` без предупреждений.
- Комментарии в коде — на английском. README и вики — на двух языках: основной английский (`README.md`, страницы
  `wiki/` с английскими именами, `Home`), русская копия (`README.ru.md`, страницы с русскими именами, `Главная`);
  под заголовком каждой страницы — переключатель на парную, в `_Sidebar.md` русские — под `<details>`. Меняя одну,
  правьте и другую. CLAUDE.md, TODO.md, курс — на русском.
- Doc-комментарии обязательны: в библиотечных крейтах `#![warn(missing_docs)]` (с `clippy -D warnings` — ошибка),
  у каждого модуля `//!`. Описывать смысл и единицы (строка 1-based, физический адрес, байты), а не повторять имя;
  для очевидных групп (варианты-мнемоники) — описание на типе и `#[allow(missing_docs)]` с причиной. Проверка
  ссылок: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items` (есть в CI).
- Любое изменение CPU — прогонять SingleStepTests; изменение кодировок — `nasm_compare`.

## Команды
```sh
cargo build --workspace
cargo test --workspace
cargo run -p rustemu-cli -- run examples/hello.asm --input x
cargo run -p rustemu-cli -- run examples/calc.asm --input '12\n30\nx'
cargo run -p rustemu-cli -- asm examples/exetest.asm -l -s
cargo run -p rustemu-cli -- run examples/mips/sum.s --input '100\n' --regs   # MIPS32 (MARS)
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
- [x] 1.8 Сопроцессор 8087 (`fpu.rs`; состояние — `System::fpu`, процессор получает его через `Bus::fpu()`: у тестовой
      шины SingleStepTests его нет — ESC декодируется и пропускается; в историю шага назад попадает только на D8..DF
      через системный снимок, обращение ставит `touched`). 80-битная арифметика — `rustc_apfloat` (X87DoubleExtended, округление и
      точность из управляющего слова), `FSQRT` — точный (Ньютон в 80 битах); трансцендентные — в f64; исключения как
      замаскированные (+ES, без прерывания); FSTSW AX/FSIN/FCOS/FSINCOS/FUCOM*/FPREM1 тоже. Ассемблер: `Tok::Float`
      (`1.5`, `2.0e-3`) только в DD/DQ/DT (`DataItem::Float`, `LineInfo::data_float` → дизассемблер `DD 2.0`),
      `Reg::St` (`st`, `st(i)`, `stN` — только в операндах FPU-мнемоник, `parse::fpu_reg`), `QWORD`/`TBYTE PTR`,
      `encode::fpu` (без размера — dword, WAIT только в FINIT/FSTSW/…, как NASM; тест `fpu_8087` — байт в байт с NASM).
      Дизассемблер `fpu_text`, окно FPU `debugger::fpu_ui` (aux → FPU, вкладка `Tool::Fpu`), справочник — группа Fpu

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
      GUI: `(строка:колонка)` в сообщениях, волнистое подчёркивание (`Build::error_spans`), клик выделяет
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
`rustemu-ui`: `app.rs` — `App` из групп состояния: `doc` (`Document`: путь, текст, сохранённый текст), `editor`
(`EditorState`: курсор, выделение, поиск, закладки), `build` (сообщения, ошибки), `program` (исходники загруженной программы,
`generation`), `exec` (цикл выполнения, CPU/FPU до шага), `debugger` (точки останова, watch, stop on condition), `view`
(прокрутка памяти/дизассемблера/исходника), `dialogs`, `web` (браузер: программы, диск в памяти), `front` (что задаёт интерфейс:
браузер, планшет, тема), `store` (запись в хранилище); плюс `settings`, `session`, `run`, `status`, `windows` и состояния
инструментов (`screen`, `console`, `library`, `reference`, `calculator`). Настройки не дублируются в `App` — читаются из
`settings`. Команды — в `app/`: `files`, `session`, `build` — compile/emulate, `exec` — шаги и цикл выполнения,
`emulator_cmds`, `shortcuts`, `storage` — persist. **`tools.rs`** — реестр инструментов: `Tool` (все окна, кроме диалогов),
`Tool::spec` (ключ в настройках, заголовок вкладки, группа панелей или плавающее, своя прокрутка, открывается с программой),
`Tool::window` (заголовок, место, размер), `tool_ui` (тело), `windows` — один цикл рисует плавающие окна; `Windows` —
флаги по `Tool` (индекс `windows[Tool::Memory]`), сохраняются под именами полей прежней структуры. Новый инструмент —
вариант `Tool` + строки в этих `match`. `Host` — инструменты интерфейса (десктоп: `emulator::Desktop` — эмулятор и
исходник; планшет не показывает окнами экран/эмулятор/исходник). `workspace.rs` — только раскладка панелей.
Тела инструментов: `inspect.rs` (память, стек, флаги, переменные, символы, листинг, журнал, `disasm_start`),
`debugger.rs` (точки останова, stop on condition, watch, call stack, value viewer), `units.rs` (ALU, FPU), `devices_ui.rs`,
`drive_ui.rs` (диск C:, меню virtual drive, запись на дискету), `console.rs`, `library_ui.rs`, `reference_ui.rs`, `programs.rs`,
`utility.rs` (ASCII, конвертер, About), `calc.rs`; `dialogs.rs` (New, options, командная строка, несохранённое, уведомления),
`examples_ui.rs` (меню Examples, more examples), `editor.rs` + `highlight.rs` (`Highlighter` — подсветка по строкам с кешем
между кадрами), `screen.rs`, `examples.rs` (встроенные `examples/*.asm`, меню `MENU` — пункты Examples Emu8086), `help.rs`,
`reference.rs`, `fonts.rs`. `rustemu-desktop`: `ui.rs` (меню, тулбар, сообщения, статус),
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
      `EXAMPLES` и описаниями `examples::description`, `examples_ui::examples_menu`; `examples::index` — без учёта регистра,
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
- [x] 5.13 Интерфейс как в Emu8086: меню File/Edit/Bookmarks/Assembler/Emulator/Math/ASCII Codes/Options/Help (Options — `dialogs::options_menu`:
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
      reset / aux (memory, ALU, FPU, stop on condition, symbol table, listing + наши окна) / vars (с константами) /
      debug (журнал состояния в стиле DEBUG.EXE после каждой остановки) / stack (от SS:FFFE, SP выделен) /
      flags (0/1, кнопка analyse → «lexical flag analyser»); подсказки кнопок — тексты оригинала
- [x] 5.18 Два режима окон (`workspace.rs`, `Settings::layout`, options → windows): **классические окна Emu8086** (по умолчанию — минимум отличий от оригинала) и **панели** (`egui_tiles` —
      `egui_dock` для egui 0.37 нет). Флаги `App::windows` (`tools::Windows`)
      — единственный источник «что открыто» (`RefState`/`LibraryState` только просят показать — `raise`): в панелях `workspace::sync` добавляет вкладку в группу инструмента (`Group`:
      Text/Debug слева, Emulator/Inspect (watch, ALU, FPU) посередине, Screen/Aside справа — под экраном vars, stack,
      debug log, log, устройства, диск; нет группы — новая колонка или половина колонки
      по раскладке по умолчанию), крестик снимает флаг. Окна рисует `tools::windows` (в панелях — только инструменты без группы),
      тело — `tools::tool_ui` (эмулятор и исходник — `Host` из `rustemu-desktop`).
      Диалоги, memory (в панелях вкладкой отнимала высоту у эмулятора), flags (+ flag analyser), stop on condition, value viewer, ASCII, конвертер, калькулятор всплывают в обоих режимах; планшет — всегда окна.
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

### Этап 7. Вторая архитектура (MIPS)
- [x] 7.0 Граница архитектуры: `rustemu-target` (`Target`), `impl Target for Machine`, session и общий UI через
      трейт, окна 8086 через `x86()`. Поведение не менялось: все тесты, корпус Emu8086 и 68 UI-снимков побайтно
      совпадают с прежними
- [x] 7.1 Ядро MIPS32 (MARS): крейт `rustemu-mips` — 77 целочисленных инструкций, HI/LO, CP0, исключения
      (переполнение, невыровненный доступ, адрес вне сегментов, ловушки, `break`, reserved), обработчик исключений,
      разреженная память с сегментами MARS и журналом; сверка с MARS — 500 случайных программ совпадают
- [x] 7.1b Машина MIPS: syscall MARS (печать, ввод как в Run I/O, `sbrk`, выход, файлы в песочнице, время, `sleep`,
      случайные числа как `java.util.Random`), текстовая консоль вместо B800, загрузка `Image`, история шага назад
- [x] 7.1c Дизассемблер MIPS (`rustemu_mips::disasm`): нотации MARS и читаемая; сверка с ассемблером и GNU objdump
- [x] 7.1d `impl Target for rustemu_mips::Machine` (регистры, `flow_at`, стек вызовов, снимки); session: загрузка,
      строки ↔ адреса, выражения с `$t0`/`$8`, Step Over/Out, Reload (`rustemu-session/tests/mips.rs`).
      Диалоги 50–59 — позже
- [x] 7.2 Ассемблер MARS (`rustemu_mips::asm`): базовые и псевдоинструкции по таблицам MARS, директивы данных,
      `%hi/%lo`, сообщения ошибок MARS; сверка с MARS побайтно по всем формам. Макросы и `.include` — 7.6
- [x] 7.2b Сверка с эталонами: кодировки базовых инструкций — с binutils (`gnu_compare`, в CI), выполнение — с MARS
      (`mars_compare`); `qemu-mipsel` на macOS недоступен
- [x] 7.3 Интерфейс нескольких архитектур (спецификация CLAUDE-MULTIARCH-UX-SPEC): переключатель процессора
      (десктоп — список в тулбаре, планшет — значок в верхней панели), свои рабочие места (см. ключевые решения).
      `rustemu_ui::mips_ui`: регистры переключаемыми группами (`RegGroup`: GPR — 32 регистра парами в два столбца,
      Special — PC/HI/LO, PC ещё и строкой под вкладками групп, CP0 — BadVAddr/Status/Cause/EPC только чтение, FPU — `$f0..$f31` парами со значением пары как double, 8 флагов условий (клик меняет); HEX/DEC, изменённые —
      красным, правка на паузе, `$zero` не правится; на планшете столбцы во всю ширину карточки), Run I/O (вывод + строка ввода, Enter),
      плоская память словами с быстрыми ссылками (.text/.data/heap/$gp/$sp/PC/.kdata; вне сегментов — `········`,
      колесо листает), список кода (адрес, слово, basic, строка — с пробелами в один; следующие инструкции псевдоинструкции —
      `↳`; точка останова кликом), листинг как text
      segment MARS, «последний шаг» (`last_step`). Десктоп: окно эмулятора MIPS (`rustemu-desktop/src/mips.rs`,
      те же кнопки — `emulator::controls`; свои id окон эмулятора и исходника — шире, Run I/O и исходник справа; в панелях ширина колонок —
      `workspace::column_widths` по процессору: у MIPS колонка эмулятора шире, Run I/O уже; ALU — кнопкой внизу и в view), меню 8086
      скрыты. Планшет: CPU-карточка MIPS, Output = Run I/O, без
      PC-клавиатуры, Memory = память + переменные, Last step по истории MIPS; меню ⋮ — только окна, у которых `App::has_tool`,
      Example library (`more_examples_window`) — примеры процессора рабочего места. Редактор: подсветка MARS
      (`Highlighter::set_mips`), подсказки форм инструкций. Примеры — `examples/mips/*.s` (`MIPS_EXAMPLES`, меню
      Examples, `?example=mips/hello.s`); New — `MIPS_TEMPLATE`. Устройства в памяти — 7.7
- [x] 7.4 Справочник MIPS32 (EN, RU) и CLI: `rustemu_ui::reference::mips` — статьи инструкций семействами
      (`MIPS_INSTRUCTIONS`, `MipsGroup`; формы и развёртка псевдоинструкций — не текстом, а из `asm::Table` и сборки
      формы в окне, `reference_ui::expansion`), директивы (`MIPS_DIRECTIVES`), сервисы syscall 1–59 (`SYSCALLS`,
      `supported`), роли регистров; русский текст — `reference/ru_mips.rs` (по имени / номеру). `Topic::MipsInstr/
      MipsDirective/Syscall`, вкладки окна System calls и Registers & memory; редактор: `mips_topic_at` (мнемоника,
      директива, `syscall` с сервисом из `li $v0, n` выше), подсказка и F1 / ⌘-клик. Тесты: каждая мнемоника
      ассемблера описана, каждый пример собирается, русский текст полный и код примеров тот же. CLI — см. таблицу
      крейтов
- [x] 7.5 Сопроцессор 1 (плавающая точка) как в MARS: ядро `fpu.rs`, ассемблер и дизассемблер всех форм,
      сервисы 2, 3, 6, 7, 43, 44, группа FPU в окне регистров, `$f`/`fcc` в выражениях, справочник (группа
      `MipsGroup::Float`, EN/RU). Сверка: выполнение — `float_programs_match_mars`, кодировки — MARS и binutils
- [x] 7.6 Макросы (`.macro`/`.end_macro`) и `.include` как в MARS (сверка `macros_match_mars`); CLI, сессия и GUI читают
      включаемые файлы (GUI: диск, диск C:, файлы примеров — `examples::MIPS_INCLUDES`), список кода — строки тел `<13> …`,
      окно исходника — тело макроса во включённом файле; справочник `.macro/.end_macro/.include`, пример `macros.s`
- [x] 7.7 Устройства MARS в памяти: Keyboard and Display MMIO Simulator и Digital Lab Sim (`rustemu_mips::mmio`, сверка
      `mmio_compare`) и Bitmap Display — окна `rustemu_ui::mips_devices` (`Tool::MmioDisplay`, `Tool::Bitmap`, `Tool::DigitalLab`, меню tools окна эмулятора MIPS;
      настройки — `Settings::mips_devices`; клавиши — очередь `DevicesState::queue`, следующая — когда программа прочитала
      предыдущую; при emulate открывается окно устройства, к которому обращается код — `mips_devices::devices_used` по смещению после `lui …, 0xffff`; bitmap рисует память как есть),
      CLI `--keys` и `[MMIO display]`, справочник (раздел устройств во вкладке регистров), примеры `keyboard_display.s`,
      `bitmap.s`, `digital_lab.s`. Других инструментов MARS (Data Cache, BHT…) нет
- [x] 7.8 Настройки MARS (`rustemu_mips::settings`): отложенные переходы, самомодифицирующийся код (по выбору, по умолчанию
      выключен, как в MARS), старт с глобальной `main`, запрет псевдоинструкций, предупреждения как ошибки, компактные
      конфигурации памяти (шаблоны COMPACT псевдоинструкций, устройства с 0x7f00); ассемблер `assemble_settings`, CLI,
      меню settings (`rustemu_ui::mars_settings`), пометка слота задержки в списке кода, справочник. Нет: аргументов
      программы, файла обработчика исключений, ввода в диалогах, сборки всех файлов папки — см. 7.9
- [x] 7.9 Остальное из MARS: диалоги 50–59 (`rustemu_mips::dialog`, окно `mips_ui::dialog_window`, в CLI — консоль),
      ввод в окне (popup input), аргументы программы, программы из нескольких файлов (`asm::assemble_files`: файл
      обработчика исключений, все файлы папки; CLI `--project`, `--exception-handler`, `--program-args`); сверки с MARS —
      настройки, проекты, аргументы, сервисы ввода. Нет: других инструментов MARS (Data Cache, BHT, Instruction
      Statistics, X-Ray…). Все отличия от MARS — таблица «Differences from MARS» / «Отличия от MARS» на страницах MIPS32
      вики; меняя поведение MIPS, правьте её

### Этап 6. Полировка
- [x] 6.1 Сравнение с Emu8086: корпус из 84 оригинальных примеров (сборка + выполнение), устройства по
      исходникам оригинала, параметры командной строки, виртуальная дискета, шаблоны New
- [x] 6.7 Интерфейс планшета — крейт `rustemu-tablet`, работает в веб-версии (iPad, сенсорные экраны, узкое
      окно) и окном на Mac для разработки. **Нативные iPadOS/Android убраны** (решение 2026-10-28: без Apple
      Developer ID; код — в локальной ветке `archive/native-tablet`, на GitHub не отправлять). Модули: `lib.rs`
      (`TabletApp`, `TabletState`, `tablet_ui`), `theme.rs` (тёмная тема через `FrontEnd::fixed_theme`), `widgets.rs`
      (иконки, кнопки, карточки), `bars.rs` (верхняя панель: имя файла — меню проекта New / Open from Files / Example library / Save draft / My drafts / Export .asm / Share source — `App::share_source`, `platform::share_file`: Web Share API через `Reflect`, иначе скачивание; Run/Pause/Step/Reset, вкладки, меню ⋮ — в нём и *Virtual drive* = `drive_ui::virtual_drive_menu`, статус),
      `tabs.rs` (раскладка по UX-спецификации V2.2: у вкладки главная область + контекст; альбомная — две колонки,
      портретная/узкая — главная сверху; Editor — исходник ~60% + Output + превью CPU, без дизассемблера; Debugger —
      Last step + Code + инструменты слева, CPU + Output справа, Output становится инструментом, когда не помещается),
      `cpu.rs` (AX–DX слово + байты — вычисляются из слова, `Sel`; SP BP SI DI, сегменты + IP, 9 флагов; регистр AX–DX —
      одна ячейка: сверху слово, снизу байты, каждое — своя цель касания; изменённое — зелёным с точкой по `exec.prev_cpu`; инспектор HEX/DEC/BIN, правка через
      `App::edit_cpu` только при `App::cpu_editable`), `debugger.rs` (Last step из `Session::last_step` —
      `rustemu_session::step::StepView`: инструкция, строка, изменения регистров/флагов/памяти по истории шага назад
      `Machine::last_step`, объяснение флагов по `alu_trace`; шаг, начатый в BIOS, показывается вызывающим INT;
      Code: Source / Mixed / Assembly, последняя строка — синяя полоса, следующая — янтарная стрелка, BIOS скрыт —
      `Session::bios_caller`, галочка *BIOS internals*; инструменты по одному — `DebugTool`, среди них Debug log (кнопка ALU и *Open ALU details* открывают плавающее окно ALU) — консоль `console_ui`; окном в планшете она не
      показывается), `cards.rs` (редактор,
      Output с плашкой ожидания клавиши, watch, точки останова, стек вызовов, трасса), `memory.rs`
      (`MemoryView`: hex-редактор, инспектор, сегменты, стек, карта), `keypad.rs` (PC-клавиатура — нижняя панель:
      заголовок PC Keyboard / состояние / Fn keys / Hide, клавиши ≥ 44 pt, высота ~28% окна; US +
      Esc/Tab/Ctrl/стрелки/F1–F10/Ins…PgDn, Shift/Ctrl на одну клавишу, открывается при начале ожидания; клавиша на
      паузе в ожидающем шаге завершает шаг; экран рядом — только на Memory), `tests.rs` (снимки `30_tablet_*`,
      `31_tablet_keyboard*`, `32_tablet_*`; сценарии R1–R5, R11, R12 спецификации). Step недоступен при
      `App::waiting_for_key`; причина остановки — `exec.last_event`. Трасса — `Machine::history_cpus`.
      Дальше по спецификации: PR 4 — Output: Screen | Devices | Messages, ряд ASM-клавиш над системной клавиатурой,
      быстрые переходы Memory (CS:IP, DS:SI, SS:SP), README со скриншотами Платформы в `rustemu_ui::platform` — только desktop (`rfd`) и wasm
- [~] 6.8 Веб-версия (`rustemu-web`, trunk → GitHub Pages, задания `web`/`deploy-web` в CI): WebGPU с откатом на
      WebGL 2 (`rustemu_ui::wgpu_options`); время — `web-time` (std `Instant` в браузере паникует), часовой пояс —
      `js_sys::Date`; файлы — `platform::request_upload`/`take_uploads` (rfd `AsyncFileDialog`) и `download`;
      `Document::name` — имя файла без пути, `Session::load_named` — загрузка из байтов. Диск C: в браузере —
      `rustemu_core::memdrive::MemDrive` (`Machine::set_dos_memory_drive`; в `dos.rs` диск — `DosDrive::Host`/`Memory`,
      открытый файл — `OpenFile`), хранится в eframe storage (`App::persist`/`restore_drive`, base64), окно
      `drive_ui.rs` (загрузка, просмотр, скачивание). «Мои программы» — `programs.rs` (`FrontEnd::browser` вместо
      `is_web()`, чтобы тестировать нативно; Save → `save_program`/диалог имени `SaveName`, Open → окно, загрузка
      исходника → `import_program`; черновик редактора `Draft` восстанавливается при следующем визите). `App::persist`
      не пишет неизменённые значения (`App::put`; записанное читается обратно — при переполнении квоты
      ключ в `unsaved_keys`, повтор при следующем сохранении, одно предупреждение); веб сохраняет каждые 10 с и сам — на `visibilitychange`/`pagehide`
      через `WebRunner::app_mut` (eframe 0.37 слушает несуществующее `onbeforeunload`); без localStorage — notice и
      вопрос перед закрытием. Паника: свой хук после хука eframe записывает черновик (`layout::CRASH_DRAFT` — копия
      `App::draft_record`, обновляется при правке) и показывает сообщение поверх холста (перезагрузить / сообщить);
      на десктопе `App::catching` (шаг, `tick`, сборка) — уведомление вместо падения. Экран текстового режима — растровый шрифт 8×16 (`font::glyph8x16`, Unscii 16, public
      domain; 4 глифа нарисованы) через атлас с «sharp bilinear» (`screen.rs`). Не сделано: PWA/офлайн, ☻☼⌂ⁿ в
      окнах с текстом (таблица ASCII, дамп памяти). Ссылки «поделиться» — `share.rs`: `#name=…&code=<deflate
      (miniz_oxide), base64url>[&run]`, окно *share link* (File → Share link…, и на Mac — ведёт на `WEB_URL`);
      веб открывает ссылку при запуске и на `hashchange` (`App::open_link` → `PendingAction::Shared`, спрашивает про
      несохранённое) и убирает её из адреса (`history.replaceState`). Программа из ссылки, пока не сохранена
      (`App::from_link`, флаг в `Draft`), работает с пустым временным диском C: (`App::use_temporary_drive`; свой —
      в `Web::own_drive`, его и сохраняет `persist`), уведомление при первом запуске и в окне drive C:. Диск в памяти —
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
- [~] 6.3 Производительность (release, M-серия): ~50–55 млн инстр/с без истории, ~32 млн с историей шага назад;
      MIPS — ~76 и ~27 млн (`cargo test -p rustemu-mips --release --test speed -- --ignored --nocapture`)
      (лёгкий снимок CPU+память на каждую инструкцию; устройства, DOS, очереди и дискета — только для IN/OUT, INT,
      исключений, IRQ и кода BIOS, см. `Machine::may_touch_system`)
- [x] 6.4 `.startup`/`.exit` (MASM 6), корпус примеров Emu8086
- [x] 6.5 Git: ветка `main`, CI — `.github/workflows/ci.yml` (macOS: fmt, clippy -D warnings, тесты с NASM; задание
      `bundle` на `main` — универсальный .app в артефактах). `audit.yml` — `cargo audit` (база RustSec) при изменении
      `Cargo.toml`/`Cargo.lock` и раз в неделю. Вики — страницы в `wiki/` (английский + русский, для пользователей), `wiki.yml`
      копирует их в вики GitHub при push в `main` (вики должна быть создана один раз вручную); меняя поведение — правьте и вики. `release.yml`: тег `vX.Y.Z` (= версии в Cargo.toml) →
      GitHub Release с zip, описание — `release-notes/vX.Y.Z.md` (если есть). Без Developer ID и нотаризации (в README — как открыть при первом запуске)


### Предсказуемый запуск (2026-11-08)

`rustemu-target::resources::ResourceBudget` — общая политика квот, `Target::resources()` — отчёт
владельцев; Options → Resource usage добавляет счётчики сборки. См. `wiki/Resource-budgets.md`.
`rustemu-build` содержит owned request/response и bounded loader; native `build_task` запускает
поток, web — отдельный module Worker (второй Trunk asset). `App::new` включает background builds;
`with_settings` по умолчанию синхронный для headless-клиентов. `source_changed` отменяет задачу;
`poll_build` проверяет версию, текст, имя, архитектуру и настройки перед применением результата.
Кеш карточки шага использует CPU + ревизию памяти; изменения программы инвалидируют кеш.
