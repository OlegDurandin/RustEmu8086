//! Assembler directives and operators, and the guide to the NASM syntax.

/// Assembler directives and operators.
pub struct Directive {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub summary: &'static str,
    pub description: &'static str,
    pub example: &'static str,
}

macro_rules! dir {
    ($name:literal, [$($alias:literal),*], $summary:literal, $desc:literal, $example:literal) => {
        Directive { name: $name, aliases: &[$($alias),*], summary: $summary, description: $desc, example: $example }
    };
}

pub const DIRECTIVES: &[Directive] = &[
    dir!(
        "ORG",
        [],
        "Set the location counter.",
        "ORG 100h is required at the top of a COM program (and makes the output a COM file); boot sectors use ORG 7C00h.",
        "org 100h"
    ),
    dir!(
        "DB",
        ["DW", "DD", "DQ", "DT"],
        "Define data: bytes, words, double words, quad words, ten bytes.",
        "Reserves memory and gives it initial values. Strings are allowed in DB. A name before the directive makes a typed variable; ? means \"no particular value\" (stored as zero).",
        "msg   db 'Hello', 0Dh, 0Ah, '$'\nnum   dw 1234h\nbuf   db 20 dup(?)\nfar_p dd 0B8000000h"
    ),
    dir!(
        "DUP",
        [],
        "Repeat data: count DUP(values).",
        "Values can be nested: 2 DUP(1, 3 DUP(0)).",
        "table db 256 dup(0)\nmatrix dw 3 dup(3 dup(0))"
    ),
    dir!(
        "EQU",
        ["="],
        "Define a constant.",
        "NAME EQU value defines a constant that cannot be redefined; NAME = value can be redefined later. A list of values (EQU 0Dh, 0Ah, '$') defines a text equate that is substituted where the name is used.",
        "CR equ 0Dh\ncount = 10\nnew_line equ 0Dh, 0Ah, '$'"
    ),
    dir!(
        "SEGMENT",
        ["ENDS"],
        "Begin / end a segment (EXE programs).",
        "Code, data and stack live in separate segments. Load DS with MOV AX, data / MOV DS, AX. A segment named STACK (or with the STACK attribute) becomes the stack.",
        "data segment\n    msg db 'hi$'\nends\ncode segment\nstart:\n    mov ax, data\n    mov ds, ax\nends\nend start"
    ),
    dir!(
        ".MODEL",
        [".DATA", ".CODE", ".STACK", ".STARTUP", ".EXIT"],
        "Simplified segments (MASM style).",
        ".DATA and .CODE open the data and code segments, @DATA is the data segment, .STACK n reserves the stack. .STARTUP loads DS, .EXIT returns to DOS.",
        ".model small\n.stack 100h\n.data\nmsg db 'hi$'\n.code\n.startup\n    lea dx, msg\n    mov ah, 9\n    int 21h\n.exit\nend"
    ),
    dir!(
        "PROC",
        ["ENDP"],
        "Begin / end a procedure.",
        "NAME PROC [NEAR|FAR] ... NAME ENDP. Inside a FAR procedure RET is assembled as RETF.",
        "print proc near\n    ; ...\n    ret\nprint endp"
    ),
    dir!(
        "END",
        [],
        "End of the source code; optionally sets the entry point.",
        "Everything after END is ignored. In EXE programs END start sets the first instruction.",
        "start:\n    mov ax, 4C00h\n    int 21h\nend start   ; the program begins at start"
    ),
    dir!(
        "MACRO",
        ["ENDM", "LOCAL"],
        "Define a macro.",
        "NAME MACRO params ... ENDM. LOCAL declares labels that get unique names in every expansion. Macros are expanded where their name is written.",
        "putc macro ch\n    mov al, ch\n    mov ah, 0Eh\n    int 10h\nendm\n    putc 'A'"
    ),
    dir!(
        "INCLUDE",
        [],
        "Insert another source file.",
        "include 'emu8086.inc' adds the Emu8086 macro library (PRINT, PUTC, SCAN_NUM, ...).",
        "include 'emu8086.inc'"
    ),
    dir!(
        "OFFSET",
        [],
        "Offset of a variable or label.",
        "MOV DX, OFFSET msg is the same as LEA DX, msg.",
        "mov dx, offset msg"
    ),
    dir!(
        "SEG",
        [],
        "Segment of a variable or label.",
        "Needs a relocation (EXE) or is fixed up when the program is loaded.",
        "mov ax, seg msg\nmov ds, ax"
    ),
    dir!(
        "PTR",
        ["BYTE", "WORD", "DWORD", "B.", "W."],
        "Give the size of a memory operand.",
        "BYTE PTR / WORD PTR (short forms b. and w.) are needed when the size cannot be taken from a register or a variable.",
        "mov byte ptr [bx], 1\nmov w.[si], 1234h\ncall dword ptr [bx]"
    ),
    dir!(
        "LABEL",
        [],
        "Define a label with a type at the current location.",
        "NAME LABEL BYTE | WORD | DWORD | NEAR | FAR.",
        "bytes label byte\nwords dw 1, 2, 3"
    ),
    dir!(
        "#MAKE_COM#",
        ["#MAKE_EXE#", "#MAKE_BIN#", "#MAKE_BOOT#"],
        "Emu8086: choose the output file type.",
        "Without a directive the type is taken from the code: ORG 100h gives COM, segments give EXE.",
        "#make_bin#"
    ),
    dir!(
        "#LOAD_SEGMENT=",
        ["#LOAD_OFFSET=", "#CS=", "#IP=", "#AX=", "#MEM="],
        "Emu8086: load address and register values for BIN files.",
        "Values are hexadecimal. #MEM=address,bytes# writes bytes to memory before the program starts.",
        "#make_bin#\n#load_segment=0500#\n#cs=0500#\n#ip=0000#"
    ),
    dir!(
        "#START=",
        [],
        "Emu8086: open a virtual device with the program.",
        "For example #start=robot.exe#, #start=traffic_lights.exe#, #start=stepper_motor.exe#, #start=led_display.exe#, #start=thermometer.exe#.",
        "#start=traffic_lights.exe#"
    ),
    dir!(
        "REPT",
        ["IF", "IFDEF", "IFNDEF", "ELSE", "ENDIF"],
        "Repetition and conditional assembly.",
        "REPT n ... ENDM repeats a block; IF expr / IFDEF name ... ELSE ... ENDIF assembles a block conditionally.",
        "rept 3\n    nop\nendm\nifdef DEBUG\n    int 3\nendif"
    ),
    // ---- NASM syntax ---------------------------------------------------------------------
    dir!(
        "BITS",
        ["USE16"],
        "NASM: generate 16-bit code.",
        "BITS 16 (or [bits 16]) starts most NASM programs for the 8086; it also makes RustEmu8086 use the NASM syntax. Only 16-bit code can run on the 8086. In NASM a plain name is an address (mov dx, msg) and [name] reads memory (mov ax, [count]); the size of memory with a number is written as BYTE / WORD without PTR.",
        "bits 16\norg 100h\n    mov dx, msg\n    mov ax, [count]\n    mov byte [count], 5\n    ret\nmsg db 'hi$'\ncount dw 0"
    ),
    dir!(
        "SECTION",
        [".TEXT", ".BSS"],
        "NASM: switch between the code, data and uninitialized data sections.",
        "section .text, section .data and section .bss; in the output .data follows the code (aligned to 4 bytes) and .bss follows .data without taking space in the file (RESB / RESW reserve it).",
        "bits 16\norg 100h\nsection .text\n    mov dx, msg\n    mov ah, 9\n    int 21h\n    ret\nsection .data\nmsg db 'hi$'\nsection .bss\nbuffer resb 16"
    ),
    dir!(
        "TIMES",
        [],
        "NASM: repeat an instruction or data definition n times.",
        "$ is the current address and $$ the start of the section, so TIMES 510-($-$$) DB 0 pads a boot sector to 510 bytes.",
        "bits 16\norg 100h\ntimes 3 nop\ntable: times 8 dw 0\n    ret"
    ),
    dir!(
        "RESB",
        ["RESW", "RESD", "RESQ", "REST"],
        "NASM: reserve uninitialized bytes / words / double words (usually in section .bss).",
        "buffer resb 64 reserves 64 bytes; in .bss they are not stored in the file.",
        "bits 16\norg 100h\n    mov si, buffer\n    ret\nsection .bss\nbuffer resb 64\nwords resw 4"
    ),
    dir!(
        "%DEFINE",
        ["%XDEFINE", "%UNDEF", "%ASSIGN"],
        "NASM: define a text macro (%define) or a numeric variable (%assign).",
        "%define NAME text replaces NAME by the text in the following lines; %define NAME(a, b) text takes arguments: NAME(1, 2) becomes the text with a and b replaced. %assign NAME expr evaluates the expression (it can be redefined).",
        "%define CR 13\n%define SQ(x) ((x)*(x))\n%define CELL(row, col) ((row)*80 + (col))*2\n%assign COUNT 2\nbits 16\norg 100h\n    mov al, CR\n    mov ax, SQ(5)          ; 25\n    mov di, CELL(2, 10)    ; offset in video memory\n    mov cx, COUNT\n    ret"
    ),
    dir!(
        "STRUC",
        ["ENDSTRUC", "ISTRUC", "IEND", "AT"],
        "NASM: describe a structure (struc) and fill one in (istruc).",
        "struc name ... endstruc lists the fields with resb / resw / resd: .x resw 1 makes name.x the offset of the field, and name_size the size of the whole structure. istruc name ... iend creates one in memory: at name.x, dw 5 puts the data at that field; the fields not given are zeros.",
        "bits 16\norg 100h\nstruc point\n    .x: resw 1\n    .y: resw 1\nendstruc\n    mov ax, [p + point.y]   ; 20\n    mov cx, point_size      ; 4\n    ret\np: istruc point\n    at point.x, dw 10\n    at point.y, dw 20\niend"
    ),
    dir!(
        "%MACRO",
        ["%ENDMACRO"],
        "NASM: define a macro with parameters %1, %2, ...",
        "%macro name n ... %endmacro; %1..%9 are the arguments and %%label is a label local to each expansion.",
        "%macro print 1\n    mov dx, %1\n    mov ah, 9\n    int 21h\n%endmacro\nbits 16\norg 100h\n    print msg\n    ret\nmsg db 'hi$'"
    ),
    dir!(
        "%REP",
        ["%ENDREP"],
        "NASM: repeat a block of lines.",
        "%rep n ... %endrep assembles the lines n times.",
        "bits 16\norg 100h\n%rep 3\n    nop\n%endrep\n    ret"
    ),
    dir!(
        "%IF",
        ["%ELIF", "%ELSE", "%ENDIF", "%IFDEF", "%IFNDEF"],
        "NASM: conditional assembly.",
        "%if expr / %elif expr / %else / %endif, and %ifdef / %ifndef name; comparisons ==, !=, <, >, <=, >= and &&, || can be used.",
        "%define DEBUG\nbits 16\norg 100h\n%ifdef DEBUG\n    int 3\n%endif\n%if 2 + 2 == 4\n    nop\n%else\n    hlt\n%endif\n    ret"
    ),
    dir!(
        "%INCLUDE",
        [],
        "NASM: insert another source file.",
        "The included file is also read as NASM.",
        "%include \"macros.inc\""
    ),
];

/// A section of the NASM syntax guide (the reference's "NASM syntax" tab).
pub struct NasmSection {
    pub title: &'static str,
    /// Paragraphs separated by blank lines; a line starting with "- " is a list item.
    pub text: &'static str,
    /// A table: the first row is the header (may be empty).
    pub table: &'static [&'static [&'static str]],
    pub example: &'static str,
}

pub const NASM_GUIDE: &[NasmSection] = &[
    NasmSection {
        title: "Overview",
        text: "Besides the Emu8086 (MASM) syntax, RustEmu8086 assembles NASM programs the way `nasm -f bin` does: COM programs, boot sectors and flat binaries. The output is checked byte for byte against the real NASM.\n\nThe emulator, the debugger and the virtual devices work the same for both syntaxes. The message after assembling tells which syntax was used.",
        table: &[],
        example: "bits 16\norg 100h\n\nsection .text\n    mov dx, msg          ; msg is an address\n    mov ah, 9\n    int 21h\n    mov ax, 4C00h\n    int 21h\n\nsection .data\nmsg db 'Hello from NASM!', 13, 10, '$'",
    },
    NasmSection {
        title: "Choosing the syntax",
        text: "By default the syntax is detected. NASM is chosen when the source has any of:\n- bits 16 / [bits 16], [org ...], section .text, cpu, global\n- preprocessor directives %define, %macro, %include, %assign, %rep, %if\n- times, resb / resw / resd, $$, struc / istruc\n- a size without PTR: byte [..], word [..]\n\nA .nasm file extension or a line #nasm# also selects NASM. Otherwise the source is Emu8086 (MASM). A program that does not assemble in the detected syntax but does in the other one is assembled in that one.\n\nThe syntax can be set explicitly: Options menu (or the options window) -> assembler syntax (auto / Emu8086 / NASM); on the command line: rustemu asm file --syntax nasm. File -> New offers NASM templates (\"use NASM syntax\").",
        table: &[],
        example: "#nasm#\norg 100h\n    mov ax, [value]\n    ret\nvalue dw 1234h",
    },
    NasmSection {
        title: "Names, memory and sizes",
        text: "The main difference from Emu8086: in NASM a name is its address, and memory is read only through square brackets. A size is written before the operand, without PTR.",
        table: &[
            &["Code", "Emu8086 (MASM)", "NASM"],
            &["mov ax, value", "reads the variable", "loads the address of value"],
            &["mov ax, [value]", "reads the variable", "reads the variable"],
            &["mov byte [bx], 5", "error (needs byte ptr)", "the way to write it"],
            &["mov [bx], 5", "a byte if the number fits", "error: no size"],
            &["mov ax, 'ab'", "AH = 'a', AL = 'b'", "AL = 'a', AH = 'b'"],
            &["mov ax, offset msg", "address of msg", "error: write mov ax, msg"],
        ],
        example: "bits 16\norg 100h\n    mov ax, count        ; AX = address of count\n    mov bx, [count]      ; BX = 5, the value\n    mov byte [buf], 'A'  ; the size comes first, no PTR\n    mov word [buf+1], 0\n    inc word [count]\n    mov al, [es:di]      ; segment override inside the brackets\n    ret\ncount dw 5\nbuf   times 4 db 0",
    },
    NasmSection {
        title: "Program formats and sections",
        text: "The format follows ORG:\n- org 100h - a COM program\n- org 7C00h - a boot sector (512 bytes)\n- anything else - a flat binary (BIN)\n\nSections work as in nasm -f bin: .text comes first, .data follows the code aligned to 4 bytes, and .bss takes the addresses after .data but is not stored in the file. Code without a section is in .text.\n\nEXE programs need an object format and a linker in NASM; RustEmu8086 does not do that - use the Emu8086 syntax for EXE.",
        table: &[],
        example: "bits 16\norg 7C00h\nstart:\n    xor ax, ax\n    mov ds, ax\n    mov si, msg\n.next:\n    lodsb\n    or al, al\n    jz .done\n    mov ah, 0Eh\n    int 10h\n    jmp .next\n.done:\n    hlt\n    jmp .done\nmsg db 'Booted!', 0\ntimes 510 - ($ - $$) db 0\ndw 0AA55h",
    },
    NasmSection {
        title: "Labels, numbers and strings",
        text: "- Labels may have a colon or not (msg db ...).\n- Local labels start with a dot: .loop belongs to the label before it and is named PREVIOUS.LOOP inside the program (that name is shown in the debugger too).\n- Numbers: 0x1F, 1Fh, $1F, 0b1010, 1010b, 0o17, 17q, 0d99.\n- Strings: '...', \"...\" and `...`; backquoted strings understand \\n, \\t, \\r, \\0, \\xHH.\n- $ is the current address, $$ the start of the section.\n- Operators: | ^ & << >> + - * / % ~ !, comparisons == != < > <= >= (true is 1), logical && || ^^, and c ? a : b.",
        table: &[],
        example: "bits 16\norg 100h\nstart:\n    mov cx, 0x10\n    mov al, 1010b\n    mov bl, 0o17\n    mov ax, 'ab'         ; AL = 'a', AH = 'b'\n.again:\n    loop .again          ; the label is start.again\n    ret\ntext db `Tab\\there\\n`, 0",
    },
    NasmSection {
        title: "Data and reservations",
        text: "- db / dw / dd / dq / dt define data; strings in dw / dd are padded with zeros to whole elements.\n- times n ... repeats an instruction or data.\n- resb / resw / resd / resq / rest reserve uninitialized space (usually in section .bss).\n- align n / alignb n pad to a multiple of n.\n- equ defines a constant.",
        table: &[],
        example: "bits 16\norg 100h\nsection .text\n    mov si, table\n    mov cx, SIZE\n    ret\nsection .data\ntable   dw 1, 2, 3, 4\nSIZE    equ ($ - table) / 2\n        align 4\nbig     dd 12345678h\nsection .bss\nbuffer  resb 64\ncounts  resw 8",
    },
    NasmSection {
        title: "Preprocessor",
        text: "- %define / %undef - text macros; %define NAME(a, b) ... - macros with arguments used inside expressions: SQ(5); %assign - numeric variables.\n- %macro name n ... %endmacro - macros with parameters %1 ... %9 and local labels %%label.\n- %rep n ... %endrep - repeat a block.\n- %if / %elif / %else / %endif, %ifdef / %ifndef - conditional assembly.\n- %include \"file\" - insert a file (read as NASM); %error stops with a message.",
        table: &[],
        example: "bits 16\norg 100h\n%define NEWLINE 13, 10\n%assign COUNT 3\n%macro print 1\n    mov dx, %1\n    mov ah, 9\n    int 21h\n%endmacro\n%macro wait_key 0\n%%again:\n    mov ah, 1\n    int 16h\n    jz %%again\n%endmacro\n    print hello\n%rep COUNT\n    nop\n%endrep\n    ret\nhello db 'Hi', NEWLINE, '$'",
    },
    NasmSection {
        title: "Structures",
        text: "struc ... endstruc describes the fields of a record: each field is the offset from the start, name_size is the size. istruc ... iend fills one in memory with at field, data; the missing fields are zeros. Read a field as [record + struc.field] or [bx + struc.field].",
        table: &[],
        example: "bits 16\norg 100h\nstruc person\n    .age:  resb 1\n    .name: resb 10\nendstruc\n    mov si, alice\n    mov al, [si + person.age]     ; 30\n    lea dx, [si + person.name]\n    mov ah, 9\n    int 21h\n    ret\nalice: istruc person\n    at person.age, db 30\n    at person.name, db 'Alice$'\niend",
    },
    NasmSection {
        title: "Debugging NASM programs",
        text: "The debugger follows the NASM meaning of names. In watch expressions, breakpoint conditions and the hover in the editor, count is the address and [count] the value (its size comes from dw / db). The vars window shows variables as usual.\n\nLocal labels appear with their full names (main.loop) in the disassembly and the symbol table.",
        table: &[],
        example: "bits 16\norg 100h\nmain:\n    mov cx, [count]      ; watch [count]: 3, 2, 1...\n.loop:\n    dec word [count]\n    loop .loop\n    ret\ncount dw 3",
    },
    NasmSection {
        title: "Not supported",
        text: "- Output formats other than -f bin (no EXE, no object files).\n- A greedy last macro parameter (%macro m 1+), %0, %rotate; %strlen, %substr and other string functions.\n- incbin, floating-point numbers in data, section names other than .text / .data / .bss, context macros (%push, %$label).\n- Far conditional jumps: without cpu 8086 NASM emits the 80386 form (0F 8x), which the 8086 does not have; RustEmu8086, like Emu8086, uses a short jump with the opposite condition over a near JMP, so these bytes differ from NASM.",
        table: &[],
        example: "",
    },
];
