//! Built-in quick reference.

pub const HELP: &str = r#"KEYBOARD
  F9 / ⌘E    assemble and load into the emulator
  ⌘B         compile to an executable file (.com/.exe/.bin) + .lst + .symbol
  ⌘S / ⌘O    save / open; the web version keeps programs in the browser ("my programs")
  File -> Share link...  a link that opens the program in the web version
  F5 / ⌘R    run / stop          F8   single step      ⇧F8  step back
  F10        step over: run the current source line (macro, CALL, INT) to completion
  ⇧F11       step out: leave the current macro / procedure (back to the caller)
  F8 on a macro line (PRINT, PUTC...) enters the macro, F10 runs it as one line
  click the emulator screen to type into the running program

PROGRAM TYPES
  #make_COM#   org 100h, single segment, ends with RET or INT 20h
  #make_EXE#   segments: data segment / code segment / stack segment, END start
  #make_BIN#   raw binary, #load_segment=XXXX#, #load_offset=XXXX#, #CS=..#, #IP=..#
  #make_BOOT#  boot sector loaded at 0000:7C00 (org 7C00h)
  without a directive: EXE when segments are used, COM otherwise

DIRECTIVES
  ORG n   DB/DW/DD/DQ/DT   n DUP(x)   ?   EQU   =   $   LABEL
  SEGMENT/ENDS   PROC [FAR]/ENDP   END [label]   ASSUME (ignored)
  .MODEL .STACK [n] .DATA .CODE @DATA   EVEN  ALIGN n
  INCLUDE 'file'   MACRO/ENDM/LOCAL/EXITM   REPT n   IF/IFDEF/IFNDEF/ELSE/ENDIF
  operators: + - * / MOD SHL SHR AND OR XOR NOT  OFFSET SEG TYPE LENGTH SIZE LOW HIGH
  BYTE PTR / WORD PTR / DWORD PTR, short forms b. and w.   SHORT / NEAR PTR / FAR PTR

EMU8086.INC (include 'emu8086.inc')
  PUTC c   GOTOXY col,row   PRINT 'text'   PRINTN 'text'   CURSOROFF  CURSORON
  DEFINE_SCAN_NUM      CALL SCAN_NUM       ; CX = number typed
  DEFINE_PRINT_NUM     CALL PRINT_NUM      ; AX signed (needs PRINT_NUM_UNS)
  DEFINE_PRINT_NUM_UNS CALL PRINT_NUM_UNS  ; AX unsigned
  DEFINE_PRINT_STRING  CALL PRINT_STRING   ; DS:SI, zero-terminated
  DEFINE_PTHIS         CALL PTHIS / DB 'text', 0
  DEFINE_GET_STRING    CALL GET_STRING     ; DS:DI buffer, DX size
  DEFINE_CLEAR_SCREEN  CALL CLEAR_SCREEN

INT 21h (DOS)
  AH=01 read char+echo -> AL      AH=02 print DL        AH=06 direct I/O (DL=FF read)
  AH=07/08 read char (no echo)    AH=09 print DS:DX '$'  AH=0A buffered input DS:DX
  AH=0B input status              AH=0C flush + call AL  AH=25/35 set/get vector AL
  AH=2A date  AH=2C time          AH=30 version          AH=4C exit (AL = code)
  files (drive C: = ~/Documents/RustEmu8086/vdrive/C, Emulator -> Virtual drive -> show drive C:
  folder; options can make it the folder of the program; in the web version a drive in the browser,
  see Emulator -> Virtual drive -> drive C: files):
  AH=3C create 3D open 3E close 3F read 40 write 41 delete 42 seek 43 attributes
  AH=39 mkdir 3A rmdir 3B chdir 47 getcwd 4E/4F find first/next 56 rename

INT 10h (video)
  AH=00 set mode (AL = 3 text 80x25, 13h graphics 320x200x256)
  AH=01 cursor shape   AH=02 set cursor (BH page, DH row, DL col)   AH=03 get cursor
  AH=05 select page    AH=06/07 scroll up/down window   AH=08 read char+attr
  AH=09 write char+attr (CX times)  AH=0A write char  AH=0C/0D put/get pixel
  AH=0E teletype       AH=0F get mode  AH=10h/AL=03 blink off  AH=13 write string

INT 16h (keyboard)  AH=00 wait key -> AH scan, AL ascii   AH=01 check (ZF=1 none)
INT 1Ah  AH=00 ticks in CX:DX    INT 33h  mouse AX=0..4    INT 20h  exit

VIRTUAL DEVICES (ports)
  4    traffic lights (word, 3 bits per light: red, yellow, green)
  7    stepper motor (half-step coil pattern; IN bit 7 = ready)
  125  thermometer (IN, degrees)     127  heater (OUT 1 = on)
  130  printer (OUT a character)     199  LED display (OUT word)

NASM SYNTAX (see the "NASM syntax" tab)
  chosen by bits 16, section, times, resb, %define, %macro, byte [..]  or #nasm# / .nasm / Options
  a name is its address, memory is in brackets:  mov ax, msg   mov ax, [count]   inc word [count]
  org 100h -> COM   org 7C00h -> boot sector   other -> BIN    section .text / .data / .bss
  times n db 0   resb/resw/resd   $ $$   .local labels   0x1F 1Fh 0b101   `escapes\n`
  %define (and SQ(x)) %assign %macro..%endmacro (%1, %%label) %rep %if/%ifdef %include  struc/istruc

FLAGS   CF carry  ZF zero  SF sign  OF overflow  PF parity  AF aux  IF int  DF dir  TF trap
JUMPS   unsigned: JA JAE JB JBE   signed: JG JGE JL JLE   JE/JZ JNE/JNZ JC JNC JS JO JP JCXZ
        a conditional jump that is too far is rewritten as  J(not cc) +3 / JMP near
"#;

/// [`HELP`] in Russian.
pub const HELP_RU: &str = r#"КЛАВИШИ
  F9 / ⌘E    собрать и загрузить в эмулятор
  ⌘B         собрать исполняемый файл (.com/.exe/.bin) + .lst + .symbol
  ⌘S / ⌘O    сохранить / открыть; веб-версия хранит программы в браузере ("my programs")
  File -> Share link...  ссылка, которая открывает программу в веб-версии
  F5 / ⌘R    запустить / остановить   F8   один шаг      ⇧F8  шаг назад
  F10        step over: выполнить текущую строку исходника (макрос, CALL, INT) целиком
  ⇧F11       step out: выйти из текущего макроса / процедуры (к месту вызова)
  F8 на строке макроса (PRINT, PUTC...) заходит внутрь, F10 выполняет его как одну строку
  щёлкните по экрану эмулятора, чтобы печатать в работающую программу

ТИПЫ ПРОГРАММ
  #make_COM#   org 100h, один сегмент, кончается RET или INT 20h
  #make_EXE#   сегменты: data segment / code segment / stack segment, END start
  #make_BIN#   двоичный файл, #load_segment=XXXX#, #load_offset=XXXX#, #CS=..#, #IP=..#
  #make_BOOT#  загрузочный сектор в 0000:7C00 (org 7C00h)
  без директивы: EXE, если есть сегменты, иначе COM

ДИРЕКТИВЫ
  ORG n   DB/DW/DD/DQ/DT   n DUP(x)   ?   EQU   =   $   LABEL
  SEGMENT/ENDS   PROC [FAR]/ENDP   END [метка]   ASSUME (пропускается)
  .MODEL .STACK [n] .DATA .CODE @DATA   EVEN  ALIGN n
  INCLUDE 'файл'   MACRO/ENDM/LOCAL/EXITM   REPT n   IF/IFDEF/IFNDEF/ELSE/ENDIF
  операторы: + - * / MOD SHL SHR AND OR XOR NOT  OFFSET SEG TYPE LENGTH SIZE LOW HIGH
  BYTE PTR / WORD PTR / DWORD PTR, короткие формы b. и w.   SHORT / NEAR PTR / FAR PTR

EMU8086.INC (include 'emu8086.inc')
  PUTC c   GOTOXY кол,стр   PRINT 'текст'   PRINTN 'текст'   CURSOROFF  CURSORON
  DEFINE_SCAN_NUM      CALL SCAN_NUM       ; CX = введённое число
  DEFINE_PRINT_NUM     CALL PRINT_NUM      ; AX со знаком (нужен PRINT_NUM_UNS)
  DEFINE_PRINT_NUM_UNS CALL PRINT_NUM_UNS  ; AX без знака
  DEFINE_PRINT_STRING  CALL PRINT_STRING   ; DS:SI, кончается нулём
  DEFINE_PTHIS         CALL PTHIS / DB 'текст', 0
  DEFINE_GET_STRING    CALL GET_STRING     ; буфер DS:DI, размер DX
  DEFINE_CLEAR_SCREEN  CALL CLEAR_SCREEN

INT 21h (DOS)
  AH=01 символ с эхом -> AL       AH=02 вывести DL       AH=06 прямой ввод-вывод (DL=FF чтение)
  AH=07/08 символ без эха         AH=09 строка DS:DX '$' AH=0A ввод строки DS:DX
  AH=0B есть ли клавиша           AH=0C очистить + AL    AH=25/35 вектор AL: задать/прочитать
  AH=2A дата  AH=2C время         AH=30 версия           AH=4C выход (AL = код)
  файлы (диск C: = ~/Documents/RustEmu8086/vdrive/C, Emulator -> Virtual drive -> show drive C:
  folder; в options можно сделать им папку программы; в веб-версии — диск в браузере,
  см. Emulator -> Virtual drive -> drive C: files):
  AH=3C создать 3D открыть 3E закрыть 3F читать 40 писать 41 удалить 42 seek 43 атрибуты
  AH=39 mkdir 3A rmdir 3B chdir 47 текущий каталог 4E/4F найти первый/следующий 56 переименовать

INT 10h (видео)
  AH=00 режим (AL = 3 текст 80x25, 13h графика 320x200x256)
  AH=01 форма курсора  AH=02 курсор (BH страница, DH строка, DL столбец)   AH=03 где курсор
  AH=05 страница       AH=06/07 прокрутка окна вверх/вниз   AH=08 прочитать символ+атрибут
  AH=09 символ+атрибут (CX раз)  AH=0A символ  AH=0C/0D нарисовать/прочитать точку
  AH=0E телетайп       AH=0F режим     AH=10h/AL=03 без мерцания  AH=13 строка

INT 16h (клавиатура)  AH=00 ждать клавишу -> AH скан-код, AL ascii   AH=01 проверить (ZF=1 нет)
INT 1Ah  AH=00 тики в CX:DX    INT 33h  мышь AX=0..4    INT 20h  выход

ВИРТУАЛЬНЫЕ УСТРОЙСТВА (порты)
  4    светофор (слово, по 3 бита на светофор: красный, жёлтый, зелёный)
  7    шаговый двигатель (полушаговая схема обмоток; IN бит 7 = готов)
  125  термометр (IN, градусы)       127  нагреватель (OUT 1 = включён)
  130  принтер (OUT символ)          199  LED-дисплей (OUT слово)

СИНТАКСИС NASM (см. вкладку "синтаксис NASM")
  выбирается по bits 16, section, times, resb, %define, %macro, byte [..]  или #nasm# / .nasm / Options
  имя — это адрес, память — в скобках:  mov ax, msg   mov ax, [count]   inc word [count]
  org 100h -> COM   org 7C00h -> загрузочный сектор   иначе -> BIN    section .text / .data / .bss
  times n db 0   resb/resw/resd   $ $$   .локальные метки   0x1F 1Fh 0b101   `escapes\n`
  %define (и SQ(x)) %assign %macro..%endmacro (%1, %%метка) %rep %if/%ifdef %include  struc/istruc

ФЛАГИ   CF перенос  ZF ноль  SF знак  OF переполнение  PF чётность  AF вспом.  IF прерыв.  DF напр.  TF трасс.
ПЕРЕХОДЫ без знака: JA JAE JB JBE   со знаком: JG JGE JL JLE   JE/JZ JNE/JNZ JC JNC JS JO JP JCXZ
        слишком далёкий условный переход собирается как  J(не cc) +3 / JMP near
"#;
