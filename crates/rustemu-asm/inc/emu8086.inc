; emu8086.inc - macro and procedure library compatible with the Emu8086 one.
; The library (its macros and procedures, their names, parameters and behaviour)
; belongs to the authors of Emu8086; this text was written for RustEmu8086.
;
; How to use:
;       include 'emu8086.inc'
;
; Macros are expanded in place wherever you write their name:
;       PUTC  GOTOXY  PRINT  PRINTN  CURSOROFF  CURSORON
;
; Procedures must be emitted once with their DEFINE_xxx macro (usually after the
; final RET of the program) and then called with CALL:
;       SCAN_NUM  PRINT_NUM  PRINT_NUM_UNS  PRINT_STRING  PTHIS  GET_STRING  CLEAR_SCREEN
;
; All of them print through BIOS INT 10h, function 0Eh (teletype output).

; PUTC char - prints one character at the cursor (INT 10h/0Eh).
; char: a constant ('A', 13) or an 8-bit register. AX is preserved.
PUTC    MACRO   char
        PUSH    AX
        MOV     AL, char                ; character to print
        MOV     AH, 0Eh                 ; BIOS teletype output
        INT     10h
        POP     AX
ENDM

; GOTOXY col, row - moves the cursor of page 0 (INT 10h/02h).
; col: 0..79, row: 0..24 (constants or 8-bit registers). AX, BX, DX are preserved.
GOTOXY  MACRO   col, row
        PUSH    AX
        PUSH    BX
        PUSH    DX
        MOV     AH, 02h                 ; BIOS: set cursor position
        MOV     DH, row
        MOV     DL, col
        MOV     BH, 0                   ; video page 0
        INT     10h
        POP     DX
        POP     BX
        POP     AX
ENDM

; PRINT 'text' - prints a string literal (no new line).
; The string is stored inside the code and skipped with a JMP. AX, SI are preserved.
PRINT   MACRO   sdat
        LOCAL   next_char, s_dcl, printed, skip_dcl
        PUSH    AX
        PUSH    SI
        JMP     skip_dcl                ; jump over the string data
s_dcl   DB      sdat, 0                 ; the text, zero-terminated
skip_dcl:
        LEA     SI, s_dcl               ; SI -> first character
next_char:
        MOV     AL, CS:[SI]             ; the string lives in the code segment
        CMP     AL, 0                   ; end of string?
        JZ      printed
        INC     SI
        MOV     AH, 0Eh                 ; BIOS teletype output
        INT     10h
        JMP     next_char
printed:
        POP     SI
        POP     AX
ENDM

; PRINTN 'text' - prints a string literal followed by a new line (CR, LF).
; Works like PRINT. AX, SI are preserved.
PRINTN  MACRO   sdat
        LOCAL   next_char, s_dcl, printed, skip_dcl
        PUSH    AX
        PUSH    SI
        JMP     skip_dcl                ; jump over the string data
s_dcl   DB      sdat, 0Dh, 0Ah, 0       ; the text + CR LF, zero-terminated
skip_dcl:
        LEA     SI, s_dcl               ; SI -> first character
next_char:
        MOV     AL, CS:[SI]             ; the string lives in the code segment
        CMP     AL, 0                   ; end of string?
        JZ      printed
        INC     SI
        MOV     AH, 0Eh                 ; BIOS teletype output
        INT     10h
        JMP     next_char
printed:
        POP     SI
        POP     AX
ENDM

; CURSOROFF - hides the text cursor (INT 10h/01h, CH bit 5 set). AX, CX are preserved.
CURSOROFF MACRO
        PUSH    AX
        PUSH    CX
        MOV     AH, 1                   ; BIOS: set cursor shape
        MOV     CX, 2809h               ; bit 5 of CH = cursor invisible
        INT     10h
        POP     CX
        POP     AX
ENDM

; CURSORON - shows the text cursor again (scan lines 8..9). AX, CX are preserved.
CURSORON MACRO
        PUSH    AX
        PUSH    CX
        MOV     AH, 1                   ; BIOS: set cursor shape
        MOV     CX, 0809h               ; visible cursor, scan lines 8..9
        INT     10h
        POP     CX
        POP     AX
ENDM

; DEFINE_SCAN_NUM - emits procedure SCAN_NUM.
; CALL SCAN_NUM reads a signed decimal number from the keyboard into CX
; (range -32768..32767). Every key is echoed as typed; non-digits and digits that
; would overflow are erased again, Backspace removes the last digit, '-' makes the
; number negative and Enter finishes. Other registers are preserved.
DEFINE_SCAN_NUM MACRO
        LOCAL   skip_proc, next_key, not_enter, not_bksp, not_minus, is_digit, erase, overflow, done, positive
        LOCAL   minus_flag, ten, saved
        JMP     skip_proc               ; do not execute the procedure here
SCAN_NUM PROC NEAR
        PUSH    DX
        PUSH    AX
        PUSH    SI
        MOV     CX, 0                   ; CX = number built so far
        MOV     BYTE PTR CS:minus_flag, 0
next_key:
        MOV     AH, 0                   ; BIOS: wait for a key -> AL = ASCII
        INT     16h
        MOV     AH, 0Eh                 ; echo it right away
        INT     10h
        CMP     AL, 0Dh                 ; Enter finishes the input
        JNE     not_enter
        JMP     done
not_enter:
        CMP     AL, 8                   ; Backspace (the cursor already moved back)
        JNE     not_bksp
        MOV     DX, 0                   ; drop the last digit: CX = CX / 10
        MOV     AX, CX
        DIV     WORD PTR CS:ten
        MOV     CX, AX
        PUTC    ' '                     ; clear the character and step back again
        PUTC    8
        JMP     next_key
not_bksp:
        CMP     AL, '-'                 ; minus: the result will be negated
        JNE     not_minus
        MOV     BYTE PTR CS:minus_flag, 1
        JMP     next_key
not_minus:
        CMP     AL, '0'                 ; only digits are accepted
        JB      erase
        CMP     AL, '9'
        JBE     is_digit
erase:
        PUTC    8                       ; remove the echoed character
        PUTC    ' '
        PUTC    8
        JMP     next_key
is_digit:
        MOV     SI, AX
        AND     SI, 0Fh                 ; SI = digit value 0..9
        MOV     CS:saved, CX
        MOV     AX, CX
        MUL     WORD PTR CS:ten         ; DX:AX = CX * 10
        JC      overflow
        ADD     AX, SI                  ; + new digit
        JC      overflow
        MOV     CX, AX                  ; accept the digit
        JMP     next_key
overflow:
        MOV     CX, CS:saved            ; too big: keep the old value, erase the digit
        JMP     erase
done:
        CMP     BYTE PTR CS:minus_flag, 0
        JE      positive
        NEG     CX                      ; apply the sign
positive:
        POP     SI
        POP     AX
        POP     DX
        RET
minus_flag DB   0                       ; 1 when '-' was typed
ten     DW      10
saved   DW      0                       ; value before the last digit
SCAN_NUM ENDP
skip_proc:
ENDM

; DEFINE_PRINT_NUM - emits procedure PRINT_NUM.
; CALL PRINT_NUM prints the signed number in AX (-32768..32767) in decimal.
; Needs DEFINE_PRINT_NUM_UNS as well. AX is preserved.
DEFINE_PRINT_NUM MACRO
        LOCAL   skip_proc, not_negative
        JMP     skip_proc               ; do not execute the procedure here
PRINT_NUM PROC NEAR
        PUSH    AX
        CMP     AX, 0
        JGE     not_negative
        NEG     AX                      ; print '-' and the absolute value
        PUTC    '-'
not_negative:
        CALL    PRINT_NUM_UNS
        POP     AX
        RET
PRINT_NUM ENDP
skip_proc:
ENDM

; DEFINE_PRINT_NUM_UNS - emits procedure PRINT_NUM_UNS.
; CALL PRINT_NUM_UNS prints the unsigned number in AX (0..65535) in decimal.
; All registers are preserved.
DEFINE_PRINT_NUM_UNS MACRO
        LOCAL   skip_proc, divide, output
        JMP     skip_proc               ; do not execute the procedure here
PRINT_NUM_UNS PROC NEAR
        PUSH    AX
        PUSH    BX
        PUSH    CX
        PUSH    DX
        MOV     BX, 10
        MOV     CX, 0                   ; CX = number of digits
divide:
        MOV     DX, 0                   ; DX:AX / 10 -> AX quotient, DX = last digit
        DIV     BX
        PUSH    DX                      ; digits come out in reverse order: keep them on the stack
        INC     CX
        CMP     AX, 0
        JNE     divide
output:
        POP     AX                      ; most significant digit first
        ADD     AL, '0'
        PUTC    AL
        LOOP    output
        POP     DX
        POP     CX
        POP     BX
        POP     AX
        RET
PRINT_NUM_UNS ENDP
skip_proc:
ENDM

; DEFINE_PRINT_STRING - emits procedure PRINT_STRING.
; CALL PRINT_STRING prints the zero-terminated string at DS:SI. AX, SI are preserved.
DEFINE_PRINT_STRING MACRO
        LOCAL   skip_proc, next_char, printed
        JMP     skip_proc               ; do not execute the procedure here
PRINT_STRING PROC NEAR
        PUSH    AX
        PUSH    SI
next_char:
        MOV     AL, [SI]                ; next character from DS:SI
        CMP     AL, 0                   ; end of string?
        JZ      printed
        INC     SI
        MOV     AH, 0Eh                 ; BIOS teletype output
        INT     10h
        JMP     next_char
printed:
        POP     SI
        POP     AX
        RET
PRINT_STRING ENDP
skip_proc:
ENDM

; DEFINE_PTHIS - emits procedure PTHIS.
; Prints the zero-terminated string written right after the CALL:
;       CALL PTHIS
;       DB 'Hello', 0
; Execution continues after the string. All registers are preserved.
DEFINE_PTHIS MACRO
        LOCAL   skip_proc, next_char, printed, saved_si
        JMP     skip_proc               ; do not execute the procedure here
PTHIS   PROC NEAR
        MOV     CS:saved_si, SI
        POP     SI                      ; return address = address of the string
        PUSH    AX
next_char:
        MOV     AL, CS:[SI]
        INC     SI
        CMP     AL, 0                   ; end of string?
        JZ      printed
        MOV     AH, 0Eh                 ; BIOS teletype output
        INT     10h
        JMP     next_char
printed:
        POP     AX
        PUSH    SI                      ; return to the byte after the string
        MOV     SI, CS:saved_si
        RET
saved_si DW     0
PTHIS   ENDP
skip_proc:
ENDM

; DEFINE_GET_STRING - emits procedure GET_STRING.
; CALL GET_STRING reads a line from the keyboard into the buffer at DS:DI.
; DX = buffer size including the terminating zero. Enter finishes, Backspace edits.
; All registers are preserved.
DEFINE_GET_STRING MACRO
        LOCAL   skip_proc, wait_key, add_char, finish, empty
        JMP     skip_proc               ; do not execute the procedure here
GET_STRING PROC NEAR
        PUSH    AX
        PUSH    CX
        PUSH    DI
        PUSH    DX
        MOV     CX, 0                   ; CX = characters stored
        CMP     DX, 1
        JBE     empty
        DEC     DX                      ; keep room for the zero
wait_key:
        MOV     AH, 0                   ; BIOS: wait for a key -> AL = ASCII
        INT     16h
        CMP     AL, 0Dh                 ; Enter?
        JE      finish
        CMP     AL, 8                   ; Backspace?
        JNE     add_char
        JCXZ    wait_key                ; nothing to delete
        DEC     CX
        DEC     DI
        PUTC    8                       ; erase the character on the screen
        PUTC    ' '
        PUTC    8
        JMP     wait_key
add_char:
        CMP     CX, DX                  ; buffer full?
        JAE     wait_key
        MOV     [DI], AL                ; store and echo
        INC     DI
        INC     CX
        MOV     AH, 0Eh
        INT     10h
        JMP     wait_key
finish:
        MOV     BYTE PTR [DI], 0        ; terminate the string
empty:
        POP     DX
        POP     DI
        POP     CX
        POP     AX
        RET
GET_STRING ENDP
skip_proc:
ENDM

; DEFINE_CLEAR_SCREEN - emits procedure CLEAR_SCREEN.
; CALL CLEAR_SCREEN clears the active page (INT 10h/06h) and moves the cursor to
; the top-left corner. All registers are preserved.
DEFINE_CLEAR_SCREEN MACRO
        LOCAL   skip_proc
        JMP     skip_proc               ; do not execute the procedure here
CLEAR_SCREEN PROC NEAR
        PUSH    AX
        PUSH    BX
        PUSH    CX
        PUSH    DX
        PUSH    DS
        MOV     AX, 40h                 ; DS = BIOS data area
        MOV     DS, AX
        MOV     AX, 0600h               ; BIOS: scroll up, AL = 0 clears the window
        MOV     BH, 07h                 ; attribute: light grey on black
        MOV     CX, 0                   ; top-left corner
        MOV     DH, DS:[84h]            ; last row
        MOV     DL, DS:[4Ah]            ; number of columns
        DEC     DL                      ; last column
        INT     10h
        MOV     BH, DS:[62h]            ; active page
        MOV     DX, 0                   ; row 0, column 0
        MOV     AH, 02h                 ; BIOS: set cursor position
        INT     10h
        POP     DS
        POP     DX
        POP     CX
        POP     BX
        POP     AX
        RET
CLEAR_SCREEN ENDP
skip_proc:
ENDM
