//! Built-in reference: 8086/80186 instructions, assembler directives, BIOS and DOS interrupts and
//! a guide to the NASM syntax.

/// One instruction (or a family sharing one entry, e.g. JE/JZ).
pub struct Instr {
    /// Main name (upper case).
    pub name: &'static str,
    /// Other names of the same entry.
    pub aliases: &'static [&'static str],
    pub group: Group,
    /// Operand forms.
    pub syntax: &'static [&'static str],
    pub summary: &'static str,
    pub description: &'static str,
    /// Effect on O D I T S Z A P C: `r` = set by the result, `-` = unchanged, `u` = undefined,
    /// `0` / `1` = cleared / set.
    pub flags: &'static str,
    pub example: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Transfer,
    Arithmetic,
    Bcd,
    Logic,
    Shift,
    String,
    Jump,
    Flags,
    Control,
    I186,
}

impl Group {
    pub const ALL: [Group; 10] = [
        Group::Transfer,
        Group::Arithmetic,
        Group::Bcd,
        Group::Logic,
        Group::Shift,
        Group::String,
        Group::Jump,
        Group::Flags,
        Group::Control,
        Group::I186,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Group::Transfer => "Data transfer",
            Group::Arithmetic => "Arithmetic",
            Group::Bcd => "Decimal (BCD) adjust",
            Group::Logic => "Logic",
            Group::Shift => "Shifts and rotates",
            Group::String => "Strings",
            Group::Jump => "Jumps, calls, loops",
            Group::Flags => "Flag control",
            Group::Control => "Processor control",
            Group::I186 => "80186 additions",
        }
    }
}

pub const FLAG_NAMES: [&str; 9] = ["O", "D", "I", "T", "S", "Z", "A", "P", "C"];

const NONE: &str = "---------";
const ARITH: &str = "r---rrrrr";
const LOGIC: &str = "0---rrur0";

macro_rules! instr {
    ($name:literal, [$($alias:literal),*], $group:ident, [$($syn:literal),*], $summary:literal, $desc:literal, $flags:expr, $example:literal) => {
        Instr {
            name: $name,
            aliases: &[$($alias),*],
            group: Group::$group,
            syntax: &[$($syn),*],
            summary: $summary,
            description: $desc,
            flags: $flags,
            example: $example,
        }
    };
}

pub const INSTRUCTIONS: &[Instr] = &[
    // ---- data transfer -------------------------------------------------------------------
    instr!(
        "MOV",
        [],
        Transfer,
        [
            "reg, reg",
            "reg, memory",
            "memory, reg",
            "reg, immediate",
            "memory, immediate",
            "sreg, reg16/memory",
            "reg16/memory, sreg"
        ],
        "Copy the second operand into the first.",
        "Copies the source operand to the destination. Both operands must have the same size. Memory-to-memory moves are not possible, and a segment register cannot be loaded with an immediate value (load a general register first). CS cannot be the destination.",
        NONE,
        "mov ax, 1234h\nmov ds, ax      ; segment registers are loaded from a register\nmov al, var1    ; read a byte variable\nmov w.[bx], 5   ; word in memory at DS:BX"
    ),
    instr!(
        "XCHG",
        [],
        Transfer,
        ["reg, reg", "reg, memory", "memory, reg"],
        "Exchange the values of two operands.",
        "Swaps the contents of the two operands. XCHG AX, reg has a short one-byte encoding; NOP is XCHG AX, AX.",
        NONE,
        "mov ax, 1\nmov bx, 2\nxchg ax, bx     ; AX = 2, BX = 1"
    ),
    instr!(
        "LEA",
        [],
        Transfer,
        ["reg16, memory"],
        "Load the offset (effective address) of a memory operand.",
        "Calculates the offset of the memory operand and stores it in the register; memory is not accessed. Unlike OFFSET it works with index registers, e.g. LEA SI, [BX+DI+4].",
        NONE,
        "lea dx, msg     ; DX = offset of msg\nmov ah, 9\nint 21h\nlea si, table[bx]"
    ),
    instr!(
        "LDS",
        [],
        Transfer,
        ["reg16, memory"],
        "Load a far pointer into DS and a register.",
        "Reads a 32-bit far pointer from memory: the first word goes to the register, the second word (segment) to DS.",
        NONE,
        "lds si, pointer ; SI = offset, DS = segment\npointer dd 0B8000000h"
    ),
    instr!(
        "LES",
        [],
        Transfer,
        ["reg16, memory"],
        "Load a far pointer into ES and a register.",
        "Reads a 32-bit far pointer from memory: the first word goes to the register, the second word (segment) to ES.",
        NONE,
        "les di, video   ; ES:DI -> video memory\nvideo dd 0B8000000h"
    ),
    instr!(
        "PUSH",
        [],
        Transfer,
        ["reg16", "sreg", "memory16", "immediate (80186)"],
        "Store a word on the stack.",
        "Decrements SP by 2 and writes the operand to SS:SP. Only words can be pushed. PUSH with an immediate value is an 80186 instruction (supported by Emu8086).",
        NONE,
        "push ax\npush ds\npush word ptr [bx]"
    ),
    instr!(
        "POP",
        [],
        Transfer,
        ["reg16", "sreg (not CS)", "memory16"],
        "Take a word from the stack.",
        "Reads the word at SS:SP into the operand and increments SP by 2. POP CS is not allowed.",
        NONE,
        "push ds\npop es          ; ES = DS"
    ),
    instr!(
        "PUSHF",
        [],
        Transfer,
        [],
        "Store the flags register on the stack.",
        "Decrements SP by 2 and writes FLAGS to SS:SP.",
        NONE,
        "pushf\npop ax          ; AX = flags"
    ),
    instr!(
        "POPF",
        [],
        Transfer,
        [],
        "Take the flags register from the stack.",
        "Loads FLAGS from SS:SP and increments SP by 2. All flags are replaced.",
        "rrrrrrrrr",
        "pushf\n; ... code that changes flags ...\npopf            ; restore them"
    ),
    instr!(
        "LAHF",
        [],
        Transfer,
        [],
        "Load AH from the low byte of FLAGS.",
        "Copies SF, ZF, AF, PF and CF into AH (bits 7, 6, 4, 2, 0).",
        NONE,
        "lahf            ; AH = SF ZF - AF - PF - CF"
    ),
    instr!(
        "SAHF",
        [],
        Transfer,
        [],
        "Store AH into the low byte of FLAGS.",
        "Sets SF, ZF, AF, PF and CF from bits 7, 6, 4, 2 and 0 of AH.",
        "----rrrrr",
        "mov ah, 1\nsahf            ; CF = 1, other status flags = 0"
    ),
    instr!(
        "CBW",
        [],
        Transfer,
        [],
        "Convert byte to word: sign-extend AL into AX.",
        "Copies bit 7 of AL into every bit of AH, so AX holds the same signed value as AL.",
        NONE,
        "mov al, -5      ; AL = 0FBh\ncbw             ; AX = 0FFFBh = -5"
    ),
    instr!(
        "CWD",
        [],
        Transfer,
        [],
        "Convert word to double word: sign-extend AX into DX:AX.",
        "Copies bit 15 of AX into every bit of DX. Use it before IDIV with a word divisor.",
        NONE,
        "mov ax, -100\ncwd             ; DX = 0FFFFh\nidiv bx"
    ),
    instr!(
        "XLATB",
        ["XLAT"],
        Transfer,
        ["", "[bx] (segment override allowed)"],
        "Translate a byte using a table: AL = DS:[BX + AL].",
        "Uses AL as an unsigned index into a 256-byte table addressed by DS:BX (another segment can be given as an override) and loads the table entry into AL.",
        NONE,
        "lea bx, hexdigits\nmov al, 0Ch\nxlatb           ; AL = 'C'\nhexdigits db '0123456789ABCDEF'"
    ),
    instr!(
        "IN",
        [],
        Transfer,
        ["AL, imm8", "AX, imm8", "AL, DX", "AX, DX"],
        "Read a byte or word from an I/O port.",
        "Reads from the port given by an 8-bit constant (0..255) or by DX (any port 0..65535). In the emulator the virtual devices live on I/O ports: 125 thermometer, 7 stepper motor, 10/11 robot data and status, ...",
        NONE,
        "in al, 125      ; read the thermometer\nmov dx, 3DAh\nin al, dx"
    ),
    instr!(
        "OUT",
        [],
        Transfer,
        ["imm8, AL", "imm8, AX", "DX, AL", "DX, AX"],
        "Write a byte or word to an I/O port.",
        "Writes AL or AX to the port given by an 8-bit constant or by DX. Virtual devices: 4 traffic lights (word), 7 stepper motor, 9 robot command, 127 heater, 130 printer, 199 LED display (word).",
        NONE,
        "mov ax, 0249h\nout 4, ax       ; traffic lights\nmov al, 'A'\nout 130, al     ; printer"
    ),
    // ---- arithmetic ----------------------------------------------------------------------
    instr!(
        "ADD",
        [],
        Arithmetic,
        ["reg, reg", "reg, memory", "memory, reg", "reg, immediate", "memory, immediate"],
        "Add: operand1 = operand1 + operand2.",
        "Adds the source to the destination. CF signals an unsigned overflow (carry out of the top bit), OF a signed overflow.",
        ARITH,
        "mov al, 250\nadd al, 10      ; AL = 4, CF = 1"
    ),
    instr!(
        "ADC",
        [],
        Arithmetic,
        ["reg, reg", "reg, memory", "memory, reg", "reg, immediate", "memory, immediate"],
        "Add with carry: operand1 = operand1 + operand2 + CF.",
        "Like ADD but also adds the carry flag; used to add numbers longer than 16 bits.",
        ARITH,
        "; DX:AX = DX:AX + CX:BX\nadd ax, bx\nadc dx, cx"
    ),
    instr!(
        "SUB",
        [],
        Arithmetic,
        ["reg, reg", "reg, memory", "memory, reg", "reg, immediate", "memory, immediate"],
        "Subtract: operand1 = operand1 - operand2.",
        "Subtracts the source from the destination. CF signals an unsigned borrow, OF a signed overflow.",
        ARITH,
        "mov al, 5\nsub al, 7       ; AL = 0FEh (-2), CF = 1"
    ),
    instr!(
        "SBB",
        [],
        Arithmetic,
        ["reg, reg", "reg, memory", "memory, reg", "reg, immediate", "memory, immediate"],
        "Subtract with borrow: operand1 = operand1 - operand2 - CF.",
        "Like SUB but also subtracts the carry flag; used for numbers longer than 16 bits.",
        ARITH,
        "; DX:AX = DX:AX - CX:BX\nsub ax, bx\nsbb dx, cx"
    ),
    instr!(
        "CMP",
        [],
        Arithmetic,
        ["reg, reg", "reg, memory", "memory, reg", "reg, immediate", "memory, immediate"],
        "Compare: set the flags as for operand1 - operand2.",
        "Subtracts the source from the destination without storing the result; only the flags change. Usually followed by a conditional jump: JA/JB/... for unsigned, JG/JL/... for signed numbers.",
        ARITH,
        "cmp al, '0'\njb not_digit\ncmp al, '9'\nja not_digit"
    ),
    instr!(
        "INC",
        [],
        Arithmetic,
        ["reg", "memory"],
        "Increment by one.",
        "Adds 1 to the operand. Unlike ADD, the carry flag is not changed.",
        "r---rrrr-",
        "inc cx\ninc byte ptr [si]"
    ),
    instr!(
        "DEC",
        [],
        Arithmetic,
        ["reg", "memory"],
        "Decrement by one.",
        "Subtracts 1 from the operand. Unlike SUB, the carry flag is not changed.",
        "r---rrrr-",
        "dec cx\njnz again"
    ),
    instr!(
        "NEG",
        [],
        Arithmetic,
        ["reg", "memory"],
        "Negate: operand = 0 - operand (two's complement).",
        "Changes the sign of a signed number. CF is set unless the operand was zero.",
        ARITH,
        "mov ax, 5\nneg ax          ; AX = 0FFFBh = -5"
    ),
    instr!(
        "MUL",
        [],
        Arithmetic,
        ["reg8/memory8", "reg16/memory16"],
        "Unsigned multiply: AX = AL * operand8, or DX:AX = AX * operand16.",
        "Multiplies AL (byte operand) or AX (word operand) by the operand as unsigned numbers. CF = OF = 1 when the upper half of the result (AH or DX) is not zero.",
        "r---uuuur",
        "mov al, 200\nmov bl, 4\nmul bl          ; AX = 800"
    ),
    instr!(
        "IMUL",
        [],
        Arithmetic,
        ["reg8/memory8", "reg16/memory16", "reg16, reg16/memory, immediate (80186)"],
        "Signed multiply: AX = AL * operand8, or DX:AX = AX * operand16.",
        "Like MUL but treats the numbers as signed. CF = OF = 1 when the upper half is not just the sign extension of the lower half. The three-operand form is an 80186 instruction.",
        "r---uuuur",
        "mov al, -3\nmov bl, 4\nimul bl         ; AX = -12"
    ),
    instr!(
        "DIV",
        [],
        Arithmetic,
        ["reg8/memory8", "reg16/memory16"],
        "Unsigned divide: AL = AX / op8, AH = remainder; AX = DX:AX / op16, DX = remainder.",
        "Divides AX (byte divisor) or DX:AX (word divisor) by the operand as unsigned numbers. Division by zero or a quotient that does not fit raises interrupt 0 (divide error). Clear DX before dividing a 16-bit number by a word!",
        "u---uuuuu",
        "mov ax, 1000\nmov dx, 0\nmov bx, 7\ndiv bx          ; AX = 142, DX = 6"
    ),
    instr!(
        "IDIV",
        [],
        Arithmetic,
        ["reg8/memory8", "reg16/memory16"],
        "Signed divide: AL = AX / op8, AH = remainder; AX = DX:AX / op16, DX = remainder.",
        "Like DIV but signed; the remainder has the sign of the dividend. Use CBW / CWD to sign-extend the dividend first.",
        "u---uuuuu",
        "mov ax, -7\ncwd\nmov bx, 2\nidiv bx         ; AX = -3, DX = -1"
    ),
    // ---- BCD -----------------------------------------------------------------------------
    instr!(
        "AAA",
        [],
        Bcd,
        [],
        "ASCII adjust after addition (unpacked BCD).",
        "After adding two unpacked BCD digits in AL: if the low nibble of AL is above 9 or AF = 1, adds 6 to AL and 1 to AH and sets AF and CF; otherwise clears them. The high nibble of AL is cleared.",
        "u---uurur",
        "mov ax, 9\nadd al, 5\naaa             ; AX = 0104h (\"14\")"
    ),
    instr!(
        "AAS",
        [],
        Bcd,
        [],
        "ASCII adjust after subtraction (unpacked BCD).",
        "After subtracting unpacked BCD digits in AL: if the low nibble is above 9 or AF = 1, subtracts 6 from AL and 1 from AH and sets AF and CF; otherwise clears them. The high nibble of AL is cleared.",
        "u---uurur",
        "mov al, 5\nsub al, 8\naas             ; AL = 7, CF = 1 (borrow)"
    ),
    instr!(
        "AAM",
        [],
        Bcd,
        ["", "imm8"],
        "ASCII adjust after multiply: AH = AL / 10, AL = AL mod 10.",
        "Splits AL into two unpacked BCD digits. Another base can be given as an operand (AAM 16). A base of 0 raises the divide error.",
        "u---rruru",
        "mov al, 7\nmov bl, 8\nmul bl\naam             ; AX = 0506h (\"56\")"
    ),
    instr!(
        "AAD",
        [],
        Bcd,
        ["", "imm8"],
        "ASCII adjust before division: AL = AH * 10 + AL, AH = 0.",
        "Combines two unpacked BCD digits in AH and AL into a binary number, before a division.",
        "u---rruru",
        "mov ax, 0207h   ; \"27\"\naad             ; AX = 27"
    ),
    instr!(
        "DAA",
        [],
        Bcd,
        [],
        "Decimal adjust after addition (packed BCD).",
        "Corrects AL after adding two packed BCD numbers (two digits per byte).",
        "u---rrrrr",
        "mov al, 38h\nadd al, 45h     ; AL = 7Dh\ndaa             ; AL = 83h"
    ),
    instr!(
        "DAS",
        [],
        Bcd,
        [],
        "Decimal adjust after subtraction (packed BCD).",
        "Corrects AL after subtracting two packed BCD numbers.",
        "u---rrrrr",
        "mov al, 83h\nsub al, 38h\ndas             ; AL = 45h"
    ),
    // ---- logic ---------------------------------------------------------------------------
    instr!(
        "AND",
        [],
        Logic,
        ["reg, reg", "reg, memory", "memory, reg", "reg, immediate", "memory, immediate"],
        "Bitwise AND.",
        "Each result bit is 1 only when both operand bits are 1. Used to clear bits (masking). CF and OF are cleared.",
        LOGIC,
        "and al, 0Fh     ; keep the low nibble\nand al, 11011111b ; to upper case"
    ),
    instr!(
        "OR",
        [],
        Logic,
        ["reg, reg", "reg, memory", "memory, reg", "reg, immediate", "memory, immediate"],
        "Bitwise OR.",
        "Each result bit is 1 when at least one operand bit is 1. Used to set bits. CF and OF are cleared.",
        LOGIC,
        "or al, 30h      ; digit 0..9 -> '0'..'9'\nor ax, ax       ; test AX for zero"
    ),
    instr!(
        "XOR",
        [],
        Logic,
        ["reg, reg", "reg, memory", "memory, reg", "reg, immediate", "memory, immediate"],
        "Bitwise exclusive OR.",
        "Each result bit is 1 when the operand bits differ. XOR reg, reg is the usual way to clear a register. CF and OF are cleared.",
        LOGIC,
        "xor ax, ax      ; AX = 0\nxor al, 20h     ; toggle letter case"
    ),
    instr!(
        "NOT",
        [],
        Logic,
        ["reg", "memory"],
        "Invert all bits (one's complement).",
        "Each bit of the operand is inverted. No flags are changed.",
        NONE,
        "mov al, 0Fh\nnot al          ; AL = 0F0h"
    ),
    instr!(
        "TEST",
        [],
        Logic,
        ["reg, reg", "reg, memory", "reg, immediate", "memory, immediate"],
        "Set the flags as for a bitwise AND, without storing the result.",
        "Usually followed by JZ / JNZ to check single bits.",
        LOGIC,
        "test al, 1\njnz odd         ; bit 0 set"
    ),
    // ---- shifts --------------------------------------------------------------------------
    instr!(
        "SHL",
        ["SAL"],
        Shift,
        ["reg/memory, 1", "reg/memory, CL", "reg/memory, imm (more than 1: repeated 1-bit shifts)"],
        "Shift left (multiply by 2): bits move left, 0 comes in, the last bit out goes to CF.",
        "SHL and SAL are the same instruction. The 8086 only shifts by 1 or by CL; a constant count is assembled as several 1-bit shifts. OF is defined for 1-bit shifts only.",
        "r---rrurr",
        "mov al, 3\nshl al, 1       ; AL = 6\nmov cl, 4\nshl ax, cl      ; AX = AX * 16"
    ),
    instr!(
        "SHR",
        [],
        Shift,
        ["reg/memory, 1", "reg/memory, CL", "reg/memory, imm"],
        "Shift right (unsigned divide by 2): 0 comes in from the left, the last bit out goes to CF.",
        "Use SAR for signed numbers. OF is defined for 1-bit shifts only.",
        "r---rrurr",
        "mov al, 0F0h\nshr al, 4       ; AL = 0Fh"
    ),
    instr!(
        "SAR",
        [],
        Shift,
        ["reg/memory, 1", "reg/memory, CL", "reg/memory, imm"],
        "Shift arithmetic right (signed divide by 2): the sign bit is kept.",
        "Each shift copies the top bit back into the top position, so negative numbers stay negative (rounding toward minus infinity).",
        "r---rrurr",
        "mov al, -8\nsar al, 1       ; AL = -4"
    ),
    instr!(
        "ROL",
        [],
        Shift,
        ["reg/memory, 1", "reg/memory, CL", "reg/memory, imm"],
        "Rotate left: the top bit goes to the bottom and to CF.",
        "No bits are lost; OF is defined for 1-bit rotations only.",
        "r-------r",
        "mov al, 81h\nrol al, 1       ; AL = 03h, CF = 1"
    ),
    instr!(
        "ROR",
        [],
        Shift,
        ["reg/memory, 1", "reg/memory, CL", "reg/memory, imm"],
        "Rotate right: the bottom bit goes to the top and to CF.",
        "No bits are lost; OF is defined for 1-bit rotations only.",
        "r-------r",
        "mov al, 1\nror al, 1       ; AL = 80h, CF = 1"
    ),
    instr!(
        "RCL",
        [],
        Shift,
        ["reg/memory, 1", "reg/memory, CL", "reg/memory, imm"],
        "Rotate left through carry: CF goes to the bottom, the top bit goes to CF.",
        "The carry flag acts as an extra bit (9- or 17-bit rotation); used to shift multi-word numbers.",
        "r-------r",
        "; shift DX:AX left by one bit\nshl ax, 1\nrcl dx, 1"
    ),
    instr!(
        "RCR",
        [],
        Shift,
        ["reg/memory, 1", "reg/memory, CL", "reg/memory, imm"],
        "Rotate right through carry: CF goes to the top, the bottom bit goes to CF.",
        "The carry flag acts as an extra bit; used to shift multi-word numbers right.",
        "r-------r",
        "; shift DX:AX right by one bit\nshr dx, 1\nrcr ax, 1"
    ),
    // ---- strings -------------------------------------------------------------------------
    instr!(
        "MOVSB",
        ["MOVSW", "MOVS"],
        String,
        ["", "dest, src (sizes from the operands)"],
        "Copy a byte/word from DS:[SI] to ES:[DI] and advance SI and DI.",
        "SI and DI are incremented (DF = 0) or decremented (DF = 1) by 1 or 2. With the REP prefix the instruction repeats CX times. The source segment may be overridden; ES:DI cannot.",
        NONE,
        "cld\nlea si, src\nlea di, dst\nmov cx, 10\nrep movsb       ; copy 10 bytes"
    ),
    instr!(
        "CMPSB",
        ["CMPSW", "CMPS"],
        String,
        ["", "src, dest"],
        "Compare DS:[SI] with ES:[DI] (flags as for CMP) and advance SI and DI.",
        "With REPE (repeat while equal) or REPNE (while not equal) and CX as the counter, compares strings.",
        ARITH,
        "cld\nmov cx, len\nrepe cmpsb\njne different"
    ),
    instr!(
        "SCASB",
        ["SCASW", "SCAS"],
        String,
        ["", "dest"],
        "Compare AL/AX with ES:[DI] (flags as for CMP) and advance DI.",
        "With REPNE it searches for a value, with REPE it skips matching values.",
        ARITH,
        "cld\nlea di, str1\nmov cx, 10\nmov al, 'x'\nrepne scasb\nje found"
    ),
    instr!(
        "LODSB",
        ["LODSW", "LODS"],
        String,
        ["", "src"],
        "Load AL/AX from DS:[SI] and advance SI.",
        "Usually used in a loop to read a string element by element.",
        NONE,
        "cld\nlea si, msg\nnext: lodsb\ncmp al, 0\njne next"
    ),
    instr!(
        "STOSB",
        ["STOSW", "STOS"],
        String,
        ["", "dest"],
        "Store AL/AX at ES:[DI] and advance DI.",
        "With REP it fills memory: CX elements.",
        NONE,
        "mov ax, 0B800h\nmov es, ax\nxor di, di\nmov ax, 0720h\nmov cx, 2000\nrep stosw       ; clear the screen"
    ),
    instr!(
        "REP",
        ["REPE", "REPZ", "REPNE", "REPNZ"],
        String,
        ["REP movs/stos/lods", "REPE / REPZ cmps/scas", "REPNE / REPNZ cmps/scas"],
        "Repeat a string instruction CX times.",
        "Before each step CX is checked: when it is 0 the instruction ends; otherwise the step runs and CX is decremented. REPE/REPZ also stops when ZF = 0 after a comparison, REPNE/REPNZ when ZF = 1.",
        NONE,
        "mov cx, 100\nrep stosb\nrepne scasb"
    ),
    // ---- jumps ---------------------------------------------------------------------------
    instr!(
        "JMP",
        [],
        Jump,
        [
            "label (short / near / far)",
            "reg16 / memory16 (near, indirect)",
            "dword memory (far, indirect)",
            "segment:offset"
        ],
        "Unconditional jump.",
        "Continues execution at the target. The assembler picks a short (2 bytes, -128..127) or near jump automatically; FAR PTR or a label in another segment gives a far jump that also loads CS.",
        NONE,
        "jmp start\njmp bx          ; to the address in BX\njmp 0FFFFh:0    ; far, reboot"
    ),
    instr!(
        "JA",
        ["JNBE"],
        Jump,
        ["label"],
        "Jump if above (unsigned): CF = 0 and ZF = 0.",
        "For unsigned comparisons. Conditional jumps on the 8086 reach -128..127 bytes; when the label is farther the assembler (like Emu8086) generates an inverted short jump over a near JMP.",
        NONE,
        "cmp al, 'z'\nja not_lower"
    ),
    instr!(
        "JAE",
        ["JNB", "JNC"],
        Jump,
        ["label"],
        "Jump if above or equal (unsigned) / no carry: CF = 0.",
        "For unsigned comparisons, or to test the carry flag.",
        NONE,
        "cmp ax, bx\njae ok"
    ),
    instr!(
        "JB",
        ["JNAE", "JC"],
        Jump,
        ["label"],
        "Jump if below (unsigned) / carry: CF = 1.",
        "For unsigned comparisons, or after DOS functions that report errors in CF.",
        NONE,
        "mov ah, 3Dh\nint 21h\njc error"
    ),
    instr!(
        "JBE",
        ["JNA"],
        Jump,
        ["label"],
        "Jump if below or equal (unsigned): CF = 1 or ZF = 1.",
        "For unsigned comparisons.",
        NONE,
        "cmp al, '9'\njbe digit"
    ),
    instr!(
        "JE",
        ["JZ"],
        Jump,
        ["label"],
        "Jump if equal / zero: ZF = 1.",
        "After CMP the operands were equal; after arithmetic the result was zero.",
        NONE,
        "cmp al, 27\nje escape"
    ),
    instr!(
        "JNE",
        ["JNZ"],
        Jump,
        ["label"],
        "Jump if not equal / not zero: ZF = 0.",
        "The most common loop condition after DEC.",
        NONE,
        "dec cx\njnz again"
    ),
    instr!(
        "JG",
        ["JNLE"],
        Jump,
        ["label"],
        "Jump if greater (signed): ZF = 0 and SF = OF.",
        "For signed comparisons.",
        NONE,
        "cmp ax, 0\njg positive"
    ),
    instr!(
        "JGE",
        ["JNL"],
        Jump,
        ["label"],
        "Jump if greater or equal (signed): SF = OF.",
        "For signed comparisons.",
        NONE,
        "cmp ax, bx\njge done"
    ),
    instr!(
        "JL",
        ["JNGE"],
        Jump,
        ["label"],
        "Jump if less (signed): SF != OF.",
        "For signed comparisons.",
        NONE,
        "cmp ax, 0\njl negative"
    ),
    instr!(
        "JLE",
        ["JNG"],
        Jump,
        ["label"],
        "Jump if less or equal (signed): ZF = 1 or SF != OF.",
        "For signed comparisons.",
        NONE,
        "cmp bx, cx\njle smaller"
    ),
    instr!(
        "JO",
        [],
        Jump,
        ["label"],
        "Jump if overflow: OF = 1.",
        "Signed overflow happened.",
        NONE,
        "add ax, bx\njo overflow"
    ),
    instr!(
        "JNO",
        [],
        Jump,
        ["label"],
        "Jump if no overflow: OF = 0.",
        "No signed overflow.",
        NONE,
        "add ax, bx\njno ok"
    ),
    instr!(
        "JS",
        [],
        Jump,
        ["label"],
        "Jump if sign (negative): SF = 1.",
        "The result is negative.",
        NONE,
        "sub ax, bx\njs negative"
    ),
    instr!(
        "JNS",
        [],
        Jump,
        ["label"],
        "Jump if not sign (positive or zero): SF = 0.",
        "The result is not negative.",
        NONE,
        "or ax, ax\njns positive"
    ),
    instr!(
        "JP",
        ["JPE"],
        Jump,
        ["label"],
        "Jump if parity even: PF = 1.",
        "The low byte of the result has an even number of 1 bits.",
        NONE,
        "test al, al\njp even_bits"
    ),
    instr!(
        "JNP",
        ["JPO"],
        Jump,
        ["label"],
        "Jump if parity odd: PF = 0.",
        "The low byte of the result has an odd number of 1 bits.",
        NONE,
        "test al, al\njnp odd_bits"
    ),
    instr!(
        "JCXZ",
        [],
        Jump,
        ["label"],
        "Jump if CX = 0.",
        "Typically placed before a LOOP to skip it when the counter is zero. When the label is out of range, Emu8086 (and RustEmu8086) assemble it as OR CX, CX + JZ.",
        NONE,
        "jcxz skip\nnext: ; ...\nloop next\nskip:"
    ),
    instr!(
        "LOOP",
        [],
        Jump,
        ["label"],
        "Decrement CX and jump if CX != 0.",
        "CX is decremented first; the jump is taken while it is not zero. The target must be within -128..127 bytes. No flags change.",
        NONE,
        "mov cx, 5\nnext: ; repeated 5 times\nloop next"
    ),
    instr!(
        "LOOPE",
        ["LOOPZ"],
        Jump,
        ["label"],
        "Decrement CX and jump if CX != 0 and ZF = 1.",
        "Loops while the elements are equal.",
        NONE,
        "next: cmp [si], al\ninc si\nloope next"
    ),
    instr!(
        "LOOPNE",
        ["LOOPNZ"],
        Jump,
        ["label"],
        "Decrement CX and jump if CX != 0 and ZF = 0.",
        "Loops until an element matches.",
        NONE,
        "next: cmp [si], al\ninc si\nloopne next"
    ),
    instr!(
        "CALL",
        [],
        Jump,
        ["label / proc", "reg16 / memory16 (near, indirect)", "dword memory (far)", "segment:offset"],
        "Call a procedure: push the return address and jump.",
        "A near call pushes IP; a far call (FAR PROC, FAR PTR) pushes CS and IP. Return with RET / RETF.",
        NONE,
        "call print_num\ncall far ptr far_proc"
    ),
    instr!(
        "RET",
        ["RETN"],
        Jump,
        ["", "imm16"],
        "Return from a procedure.",
        "Pops IP (and CS for a far procedure: inside a PROC FAR the assembler emits RETF). RET n also removes n bytes of parameters from the stack. In a COM program the final RET returns to DOS.",
        NONE,
        "my_proc proc\n    ; ...\n    ret\nmy_proc endp"
    ),
    instr!(
        "RETF",
        [],
        Jump,
        ["", "imm16"],
        "Far return: pop IP and CS.",
        "Returns from a far call.",
        NONE,
        "far_proc proc far\n    retf\nfar_proc endp"
    ),
    instr!(
        "INT",
        [],
        Jump,
        ["imm8"],
        "Software interrupt: push FLAGS, CS, IP and jump through the interrupt vector.",
        "Calls the handler whose address is stored at 0000:n*4. Clears IF and TF. INT 10h = video BIOS, 16h = keyboard, 21h = DOS, ... (see Interrupts). INT 3 is the one-byte breakpoint.",
        "--00-----",
        "mov ah, 9\nlea dx, msg\nint 21h         ; print a $-terminated string"
    ),
    instr!(
        "INTO",
        [],
        Jump,
        [],
        "Interrupt 4 if OF = 1.",
        "Calls INT 4 when the last signed operation overflowed.",
        "--00-----",
        "add ax, bx\ninto"
    ),
    instr!(
        "IRET",
        [],
        Jump,
        [],
        "Return from an interrupt handler: pop IP, CS and FLAGS.",
        "Ends an interrupt service routine; all flags are restored from the stack.",
        "rrrrrrrrr",
        "my_handler:\n    ; ...\n    iret"
    ),
    // ---- flags ---------------------------------------------------------------------------
    instr!("CLC", [], Flags, [], "Clear the carry flag: CF = 0.", "", "--------0", "clc"),
    instr!("STC", [], Flags, [], "Set the carry flag: CF = 1.", "", "--------1", "stc"),
    instr!("CMC", [], Flags, [], "Complement (invert) the carry flag.", "", "--------r", "cmc"),
    instr!(
        "CLD",
        [],
        Flags,
        [],
        "Clear the direction flag: string instructions go forward (SI, DI increase).",
        "",
        "-0-------",
        "cld\nrep movsb"
    ),
    instr!(
        "STD",
        [],
        Flags,
        [],
        "Set the direction flag: string instructions go backward (SI, DI decrease).",
        "",
        "-1-------",
        "std\nrep movsb"
    ),
    instr!(
        "CLI",
        [],
        Flags,
        [],
        "Clear the interrupt flag: hardware interrupts are disabled.",
        "",
        "--0------",
        "cli\n; change the interrupt vector\nsti"
    ),
    instr!("STI", [], Flags, [], "Set the interrupt flag: hardware interrupts are enabled.", "", "--1------", "sti"),
    // ---- control -------------------------------------------------------------------------
    instr!(
        "NOP",
        [],
        Control,
        [],
        "No operation.",
        "Does nothing (one byte, 90h); used for padding or as a placeholder.",
        NONE,
        "nop"
    ),
    instr!(
        "HLT",
        [],
        Control,
        [],
        "Halt until an interrupt arrives.",
        "The processor stops. In the emulator, HLT ends a BIN program.",
        NONE,
        "hlt"
    ),
    instr!(
        "WAIT",
        ["FWAIT"],
        Control,
        [],
        "Wait for the coprocessor.",
        "Synchronizes with an 8087; does nothing in the emulator.",
        NONE,
        "wait"
    ),
    instr!(
        "LOCK",
        [],
        Control,
        ["LOCK instruction"],
        "Bus lock prefix.",
        "Makes the following instruction atomic on multiprocessor systems; no effect in the emulator.",
        NONE,
        "lock xchg ax, [bx]"
    ),
    instr!(
        "ESC",
        [],
        Control,
        ["imm6, memory"],
        "Escape to the coprocessor.",
        "Passes an instruction to an 8087 coprocessor; ignored by the emulator.",
        NONE,
        "esc 5, [bx]"
    ),
    // ---- 80186 ---------------------------------------------------------------------------
    instr!(
        "PUSHA",
        [],
        I186,
        [],
        "Push all general registers: AX, CX, DX, BX, SP, BP, SI, DI.",
        "80186 instruction, supported by Emu8086. The SP value pushed is the one before PUSHA.",
        NONE,
        "my_proc proc\n    pusha\n    ; ...\n    popa\n    ret\nmy_proc endp"
    ),
    instr!(
        "POPA",
        [],
        I186,
        [],
        "Pop all general registers (the saved SP is skipped).",
        "80186 instruction; restores what PUSHA saved: DI, SI, BP, (SP), BX, DX, CX, AX.",
        NONE,
        "popa"
    ),
    instr!(
        "LEAVE",
        [],
        I186,
        [],
        "Release a stack frame: MOV SP, BP and POP BP.",
        "80186: one byte instead of two instructions at the end of a procedure that set up BP.",
        NONE,
        "my_proc proc\n    push bp\n    mov bp, sp\n    sub sp, 4   ; local variables\n    ; ...\n    leave\n    ret\nmy_proc endp"
    ),
];

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
        "%define NAME text replaces NAME by the text in the following lines; %assign NAME expr evaluates the expression (it can be redefined). %define with parameters is not supported - use %macro.",
        "%define CR 13\n%assign COUNT 2\nbits 16\norg 100h\n    mov al, CR\n    mov cx, COUNT\n    ret"
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
        text: "By default the syntax is detected. NASM is chosen when the source has any of:\n- bits 16 / [bits 16], [org ...], section .text, cpu, global\n- preprocessor directives %define, %macro, %include, %assign, %rep, %if\n- times, resb / resw / resd, $$\n- a size without PTR: byte [..], word [..]\n\nA .nasm file extension or a line #nasm# also selects NASM. Otherwise the source is Emu8086 (MASM).\n\nThe syntax can be set explicitly: options window -> assembler syntax (auto / Emu8086 / NASM); on the command line: rustemu asm file --syntax nasm. File -> New offers NASM templates (\"use NASM syntax\").",
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
        text: "- Labels may have a colon or not (msg db ...).\n- Local labels start with a dot: .loop belongs to the label before it and is named PREVIOUS.LOOP inside the program (that name is shown in the debugger too).\n- Numbers: 0x1F, 1Fh, $1F, 0b1010, 1010b, 0o17, 17q, 0d99.\n- Strings: '...', \"...\" and `...`; backquoted strings understand \\n, \\t, \\r, \\0, \\xHH.\n- $ is the current address, $$ the start of the section.\n- Operators: | ^ & << >> + - * / % ~ !, comparisons == != < > <= >= && ||.",
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
        text: "- %define / %undef - text macros; %assign - numeric variables.\n- %macro name n ... %endmacro - macros with parameters %1 ... %9 and local labels %%label.\n- %rep n ... %endrep - repeat a block.\n- %if / %elif / %else / %endif, %ifdef / %ifndef - conditional assembly.\n- %include \"file\" - insert a file (read as NASM); %error stops with a message.",
        table: &[],
        example: "bits 16\norg 100h\n%define NEWLINE 13, 10\n%assign COUNT 3\n%macro print 1\n    mov dx, %1\n    mov ah, 9\n    int 21h\n%endmacro\n%macro wait_key 0\n%%again:\n    mov ah, 1\n    int 16h\n    jz %%again\n%endmacro\n    print hello\n%rep COUNT\n    nop\n%endrep\n    ret\nhello db 'Hi', NEWLINE, '$'",
    },
    NasmSection {
        title: "Debugging NASM programs",
        text: "The debugger follows the NASM meaning of names. In watch expressions, breakpoint conditions and the hover in the editor, count is the address and [count] the value (its size comes from dw / db). The vars window shows variables as usual.\n\nLocal labels appear with their full names (main.loop) in the disassembly and the symbol table.",
        table: &[],
        example: "bits 16\norg 100h\nmain:\n    mov cx, [count]      ; watch [count]: 3, 2, 1...\n.loop:\n    dec word [count]\n    loop .loop\n    ret\ncount dw 3",
    },
    NasmSection {
        title: "Not supported",
        text: "- Output formats other than -f bin (no EXE, no object files).\n- %define with parameters, a greedy last macro parameter (%macro m 1+), %0, %rotate.\n- incbin, struc, section names other than .text / .data / .bss, context macros (%push, %$label).\n- Far conditional jumps: without cpu 8086 NASM emits the 80386 form (0F 8x), which the 8086 does not have; RustEmu8086, like Emu8086, uses a short jump with the opposite condition over a near JMP, so these bytes differ from NASM.",
        table: &[],
        example: "",
    },
];

/// A BIOS / DOS interrupt.
pub struct Interrupt {
    pub number: u8,
    pub title: &'static str,
    pub description: &'static str,
    /// Register that selects the function (empty if none).
    pub selector: &'static str,
    pub functions: &'static [Function],
}

pub struct Function {
    pub code: u16,
    pub title: &'static str,
    pub input: &'static str,
    pub output: &'static str,
    pub example: &'static str,
}

macro_rules! func {
    ($code:literal, $title:literal, $input:literal, $output:literal, $example:literal) => {
        Function { code: $code, title: $title, input: $input, output: $output, example: $example }
    };
}

pub const INTERRUPTS: &[Interrupt] = &[
    Interrupt {
        number: 0x10,
        title: "Video services (BIOS)",
        description: "Screen output: video modes, cursor, characters with colours, scrolling and pixels. The text screen is 80x25 in mode 3 (memory at B800:0000, two bytes per character: code and attribute); mode 13h is 320x200 with 256 colours (memory at A000:0000).",
        selector: "AH",
        functions: &[
            func!(
                0x00,
                "Set video mode",
                "AL = mode: 00h/01h text 40x25, 02h/03h text 80x25 (16 colours, 8 pages), 13h graphics 320x200 256 colours",
                "The screen is cleared.",
                "mov ah, 0\nmov al, 3\nint 10h"
            ),
            func!(
                0x01,
                "Set cursor shape",
                "CH = first scan line (bit 5 = 1 hides the cursor), CL = last scan line",
                "",
                "mov ah, 1\nmov cx, 2607h   ; hide the cursor\nint 10h"
            ),
            func!(
                0x02,
                "Set cursor position",
                "DH = row, DL = column, BH = page",
                "",
                "mov ah, 2\nmov dh, 10\nmov dl, 20\nmov bh, 0\nint 10h"
            ),
            func!(
                0x03,
                "Get cursor position and shape",
                "BH = page",
                "DH = row, DL = column, CH/CL = cursor scan lines",
                "mov ah, 3\nmov bh, 0\nint 10h"
            ),
            func!(0x05, "Select active page", "AL = page (0..7)", "", "mov ah, 5\nmov al, 1\nint 10h"),
            func!(
                0x06,
                "Scroll window up",
                "AL = lines (0 = clear the window), BH = attribute for new lines, CH,CL = top row, column, DH,DL = bottom row, column",
                "",
                "mov ax, 0600h   ; clear\nmov bh, 07h\nmov cx, 0\nmov dx, 184Fh\nint 10h"
            ),
            func!(
                0x07,
                "Scroll window down",
                "Same registers as AH = 06h",
                "",
                "mov ax, 0701h\nmov bh, 07h\nmov cx, 0\nmov dx, 184Fh\nint 10h"
            ),
            func!(
                0x08,
                "Read character and attribute at the cursor",
                "BH = page",
                "AL = character, AH = attribute",
                "mov ah, 8\nmov bh, 0\nint 10h"
            ),
            func!(
                0x09,
                "Write character and attribute at the cursor",
                "AL = character, BL = attribute, BH = page, CX = repeat count",
                "The cursor does not move.",
                "mov ah, 9\nmov al, '*'\nmov bl, 1Eh     ; yellow on blue\nmov bh, 0\nmov cx, 10\nint 10h"
            ),
            func!(
                0x0A,
                "Write character at the cursor (keep the attribute)",
                "AL = character, BH = page, CX = count",
                "The cursor does not move.",
                "mov ah, 0Ah\nmov al, '-'\nmov bh, 0\nmov cx, 80\nint 10h"
            ),
            func!(
                0x0C,
                "Put pixel",
                "AL = colour, CX = column, DX = row (graphics modes)",
                "",
                "mov ah, 0Ch\nmov al, 15\nmov cx, 100\nmov dx, 50\nint 10h"
            ),
            func!(
                0x0D,
                "Get pixel",
                "CX = column, DX = row",
                "AL = colour",
                "mov ah, 0Dh\nmov cx, 100\nmov dx, 50\nint 10h"
            ),
            func!(
                0x0E,
                "Teletype output",
                "AL = character (BEL, BS, CR, LF are handled), BL = colour in graphics modes",
                "The cursor advances; the screen scrolls at the bottom.",
                "mov ah, 0Eh\nmov al, 'A'\nint 10h"
            ),
            func!(0x0F, "Get video mode", "", "AL = mode, AH = columns, BH = active page", "mov ah, 0Fh\nint 10h"),
            func!(
                0x10,
                "Palette / blink control (AL = 03h)",
                "AL = 03h, BL = 0: bit 7 of attributes means bright background (16 colours), BL = 1: blinking",
                "",
                "mov ax, 1003h\nmov bx, 0\nint 10h"
            ),
            func!(
                0x13,
                "Write string",
                "AL = mode (bit 0: move cursor, bit 1: string contains character/attribute pairs), BH = page, BL = attribute, CX = length, DH,DL = row, column, ES:BP = string",
                "",
                "mov ax, 1301h\nmov bx, 000Eh\nmov cx, 5\nmov dx, 0\nlea bp, txt     ; ES must point to the data\nint 10h"
            ),
        ],
    },
    Interrupt {
        number: 0x16,
        title: "Keyboard services (BIOS)",
        description: "Reading keys from the keyboard buffer. Type into the emulator screen to send keys to the program.",
        selector: "AH",
        functions: &[
            func!(
                0x00,
                "Wait for a key",
                "",
                "AH = scan code, AL = ASCII code (0 for special keys such as arrows and F1..F10). The key is removed from the buffer.",
                "mov ah, 0\nint 16h\ncmp al, 27      ; Esc?"
            ),
            func!(
                0x01,
                "Check for a key",
                "",
                "ZF = 1: no key; ZF = 0: AX = key (it stays in the buffer)",
                "mov ah, 1\nint 16h\njz no_key"
            ),
            func!(0x02, "Get shift flags", "", "AL = state of Shift, Ctrl, Alt, ... keys", "mov ah, 2\nint 16h"),
        ],
    },
    Interrupt {
        number: 0x21,
        title: "DOS services",
        description: "The operating system functions: console input/output, files, time, memory, program exit. File names are relative to the folder of the source file (the virtual drive C:). On errors file functions set CF = 1 and AX = error code.",
        selector: "AH",
        functions: &[
            func!(0x01, "Read a character with echo", "", "AL = character (waits for a key)", "mov ah, 1\nint 21h"),
            func!(0x02, "Write a character", "DL = character", "", "mov ah, 2\nmov dl, 'A'\nint 21h"),
            func!(0x05, "Write a character to the printer", "DL = character", "", "mov ah, 5\nmov dl, 'P'\nint 21h"),
            func!(
                0x06,
                "Direct console input/output",
                "DL = 0FFh: read a key without waiting; otherwise DL = character to write",
                "Input: ZF = 1 if no key, else AL = character",
                "mov ah, 6\nmov dl, 0FFh\nint 21h\njz no_key"
            ),
            func!(0x07, "Read a character without echo", "", "AL = character", "mov ah, 7\nint 21h"),
            func!(
                0x08,
                "Read a character without echo (Ctrl+Break checked)",
                "",
                "AL = character",
                "mov ah, 8\nint 21h"
            ),
            func!(
                0x09,
                "Write a string",
                "DS:DX = string ending with '$'",
                "",
                "mov ah, 9\nlea dx, msg\nint 21h\nmsg db 'Hello!$'"
            ),
            func!(
                0x0A,
                "Read a line (buffered input)",
                "DS:DX = buffer: byte 0 = maximum characters (including Enter), byte 1 = filled in with the count, then the characters and 0Dh",
                "",
                "mov ah, 0Ah\nlea dx, buffer\nint 21h\nbuffer db 20, ?, 20 dup(?)"
            ),
            func!(
                0x0B,
                "Check keyboard status",
                "",
                "AL = 0FFh if a key is waiting, 0 otherwise",
                "mov ah, 0Bh\nint 21h"
            ),
            func!(
                0x0C,
                "Flush the keyboard buffer, then call function AL",
                "AL = 01h, 06h, 07h, 08h or 0Ah",
                "As the called function",
                "mov ax, 0C01h\nint 21h"
            ),
            func!(0x19, "Get the current drive", "", "AL = drive (0 = A:, 2 = C:)", "mov ah, 19h\nint 21h"),
            func!(
                0x25,
                "Set an interrupt vector",
                "AL = interrupt number, DS:DX = new handler",
                "",
                "mov ax, 251Ch\nlea dx, my_timer\nint 21h"
            ),
            func!(
                0x2A,
                "Get the date",
                "",
                "CX = year, DH = month, DL = day, AL = day of the week (0 = Sunday)",
                "mov ah, 2Ah\nint 21h"
            ),
            func!(
                0x2C,
                "Get the time",
                "",
                "CH = hour, CL = minute, DH = second, DL = 1/100 seconds",
                "mov ah, 2Ch\nint 21h"
            ),
            func!(0x30, "Get the DOS version", "", "AL = major, AH = minor version", "mov ah, 30h\nint 21h"),
            func!(
                0x35,
                "Get an interrupt vector",
                "AL = interrupt number",
                "ES:BX = handler",
                "mov ax, 3510h\nint 21h"
            ),
            func!(
                0x39,
                "Create a directory",
                "DS:DX = path (zero-terminated)",
                "CF = 1 on error",
                "mov ah, 39h\nlea dx, dir\nint 21h\ndir db 'c:\\test', 0"
            ),
            func!(0x3A, "Remove a directory", "DS:DX = path", "CF = 1 on error", "mov ah, 3Ah\nlea dx, dir\nint 21h"),
            func!(
                0x3B,
                "Change the current directory",
                "DS:DX = path",
                "CF = 1 on error",
                "mov ah, 3Bh\nlea dx, dir\nint 21h"
            ),
            func!(
                0x3C,
                "Create a file",
                "DS:DX = file name, CX = attributes (0 = normal)",
                "AX = handle, CF = 1 on error",
                "mov ah, 3Ch\nmov cx, 0\nlea dx, fname\nint 21h\njc error\nmov handle, ax\nfname db 'test.txt', 0"
            ),
            func!(
                0x3D,
                "Open a file",
                "DS:DX = file name, AL = mode: 0 read, 1 write, 2 read/write",
                "AX = handle, CF = 1 on error",
                "mov ax, 3D00h\nlea dx, fname\nint 21h\njc error"
            ),
            func!(0x3E, "Close a file", "BX = handle", "CF = 1 on error", "mov ah, 3Eh\nmov bx, handle\nint 21h"),
            func!(
                0x3F,
                "Read from a file or the keyboard",
                "BX = handle (0 = keyboard), CX = byte count, DS:DX = buffer",
                "AX = bytes read (0 = end of file), CF = 1 on error",
                "mov ah, 3Fh\nmov bx, handle\nmov cx, 100\nlea dx, buffer\nint 21h"
            ),
            func!(
                0x40,
                "Write to a file or the screen",
                "BX = handle (1 = screen), CX = byte count, DS:DX = data",
                "AX = bytes written, CF = 1 on error",
                "mov ah, 40h\nmov bx, 1\nmov cx, 5\nlea dx, msg\nint 21h"
            ),
            func!(0x41, "Delete a file", "DS:DX = file name", "CF = 1 on error", "mov ah, 41h\nlea dx, fname\nint 21h"),
            func!(
                0x42,
                "Move the file pointer (seek)",
                "BX = handle, AL = origin (0 start, 1 current, 2 end), CX:DX = offset",
                "DX:AX = new position, CF = 1 on error",
                "mov ax, 4200h\nmov bx, handle\nxor cx, cx\nxor dx, dx\nint 21h"
            ),
            func!(
                0x43,
                "Get / set file attributes",
                "AL = 0 get, 1 set; DS:DX = file name; CX = attributes to set",
                "CX = attributes, CF = 1 if the file does not exist",
                "mov ax, 4300h\nlea dx, fname\nint 21h\njc not_found"
            ),
            func!(
                0x47,
                "Get the current directory",
                "DL = drive (0 = current), DS:SI = 64-byte buffer",
                "The path is stored without the drive letter",
                "mov ah, 47h\nmov dl, 0\nlea si, path\nint 21h"
            ),
            func!(0x4C, "Terminate the program", "AL = exit code", "", "mov ax, 4C00h\nint 21h"),
            func!(
                0x4E,
                "Find the first file",
                "DS:DX = mask (e.g. '*.txt'), CX = attributes",
                "The DTA is filled (name at offset 1Eh); CF = 1 if nothing found",
                "mov ah, 4Eh\nmov cx, 0\nlea dx, mask\nint 21h"
            ),
            func!(0x4F, "Find the next file", "", "CF = 1 when there are no more files", "mov ah, 4Fh\nint 21h"),
            func!(
                0x56,
                "Rename a file",
                "DS:DX = old name, ES:DI = new name",
                "CF = 1 on error",
                "mov ah, 56h\nlea dx, old\nlea di, new\nint 21h"
            ),
        ],
    },
    Interrupt {
        number: 0x20,
        title: "Terminate the program (DOS)",
        description: "Ends a COM program. The final RET of a COM program jumps to INT 20h at the start of the PSP.",
        selector: "",
        functions: &[],
    },
    Interrupt {
        number: 0x1A,
        title: "Time services (BIOS)",
        description: "System timer and real-time clock.",
        selector: "AH",
        functions: &[
            func!(
                0x00,
                "Get the tick count",
                "",
                "CX:DX = timer ticks since midnight (about 18.2 per second)",
                "mov ah, 0\nint 1Ah\n; DX changes 18 times a second"
            ),
            func!(
                0x02,
                "Get the time (BCD)",
                "",
                "CH = hours, CL = minutes, DH = seconds (packed BCD)",
                "mov ah, 2\nint 1Ah"
            ),
            func!(0x04, "Get the date (BCD)", "", "CX = year, DH = month, DL = day (packed BCD)", "mov ah, 4\nint 1Ah"),
        ],
    },
    Interrupt {
        number: 0x15,
        title: "System services (BIOS)",
        description: "Miscellaneous BIOS functions.",
        selector: "AH",
        functions: &[func!(
            0x86,
            "Wait",
            "CX:DX = interval in microseconds",
            "CF = 0",
            "mov cx, 0Fh\nmov dx, 4240h   ; 1 000 000 us = 1 s\nmov ah, 86h\nint 15h"
        )],
    },
    Interrupt {
        number: 0x13,
        title: "Disk services (BIOS)",
        description: "Sector access to the virtual floppy (Emulator -> Virtual drive). 80 cylinders, 2 heads, 18 sectors of 512 bytes.",
        selector: "AH",
        functions: &[
            func!(0x00, "Reset the disk system", "DL = drive", "CF = 0", "mov ah, 0\nmov dl, 0\nint 13h"),
            func!(
                0x02,
                "Read sectors",
                "AL = count, CH = cylinder, CL = sector (1..18), DH = head, DL = drive (0 = A:), ES:BX = buffer",
                "AL = sectors read, CF = 1 on error",
                "mov ax, 0201h\nmov cx, 0002h\nmov dx, 0\nmov bx, 0800h\nmov es, bx\nxor bx, bx\nint 13h"
            ),
            func!(
                0x03,
                "Write sectors",
                "Same registers as AH = 02h",
                "AL = sectors written, CF = 1 on error",
                "mov ax, 0301h\nmov cx, 0001h\nmov dx, 0\nlea bx, sector\nint 13h"
            ),
            func!(
                0x08,
                "Get drive parameters",
                "DL = drive",
                "CH = last cylinder, CL = sectors per track, DH = last head",
                "mov ah, 8\nmov dl, 0\nint 13h"
            ),
        ],
    },
    Interrupt {
        number: 0x11,
        title: "Get the equipment list (BIOS)",
        description: "AX = equipment word (same as 0040:0010).",
        selector: "",
        functions: &[],
    },
    Interrupt {
        number: 0x12,
        title: "Get the memory size (BIOS)",
        description: "AX = kilobytes of conventional memory (640).",
        selector: "",
        functions: &[],
    },
    Interrupt {
        number: 0x19,
        title: "Reboot (BIOS)",
        description: "Restarts the computer; in the emulator the program ends.",
        selector: "",
        functions: &[],
    },
    Interrupt {
        number: 0x33,
        title: "Mouse services",
        description: "Mouse position and buttons in the emulator screen. Coordinates are on a 640x200 grid (in mode 13h divide X by 2).",
        selector: "AX",
        functions: &[
            func!(
                0x0000,
                "Reset the mouse",
                "",
                "AX = 0FFFFh if a mouse is installed, BX = number of buttons",
                "mov ax, 0\nint 33h"
            ),
            func!(0x0001, "Show the mouse cursor", "", "", "mov ax, 1\nint 33h"),
            func!(0x0002, "Hide the mouse cursor", "", "", "mov ax, 2\nint 33h"),
            func!(
                0x0003,
                "Get position and buttons",
                "",
                "CX = X, DX = Y, BX = buttons (bit 0 left, bit 1 right, bit 2 middle)",
                "mov ax, 3\nint 33h"
            ),
            func!(0x0004, "Set the position", "CX = X, DX = Y", "", "mov ax, 4\nmov cx, 320\nmov dx, 100\nint 33h"),
        ],
    },
];

/// Finds an instruction entry by name or alias (case-insensitive).
pub fn find_instruction(name: &str) -> Option<usize> {
    let u = name.to_ascii_uppercase();
    INSTRUCTIONS.iter().position(|i| i.name == u || i.aliases.contains(&u.as_str()))
}

/// Finds a directive entry; `#...#` directives match by their prefix (`#cs=0500#` -> `#CS=`).
pub fn find_directive(name: &str) -> Option<usize> {
    let u = name.to_ascii_uppercase();
    DIRECTIVES.iter().position(|d| {
        std::iter::once(d.name)
            .chain(d.aliases.iter().copied())
            .any(|n| if n.starts_with('#') { u.starts_with(n.trim_end_matches('#')) } else { n == u })
    })
}

pub fn find_interrupt(number: u8) -> Option<usize> {
    INTERRUPTS.iter().position(|i| i.number == number)
}

/// Readable effect of a flag code.
pub fn flag_effect(c: char) -> &'static str {
    match c {
        'r' => "changed",
        'u' => "undefined",
        '0' => "cleared (0)",
        '1' => "set (1)",
        _ => "unchanged",
    }
}

/// One line like `O=changed S=changed ... (D I T unchanged)`.
pub fn flags_summary(flags: &str) -> String {
    let changed: Vec<String> = flags
        .chars()
        .zip(FLAG_NAMES)
        .filter(|(c, _)| *c != '-')
        .map(|(c, n)| match c {
            'r' => n.to_string(),
            'u' => format!("{n}?"),
            v => format!("{n}={v}"),
        })
        .collect();
    if changed.is_empty() { "flags: not affected".into() } else { format!("flags: {}", changed.join(" ")) }
}

/// An article of the reference.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Topic {
    Instr(usize),
    Directive(usize),
    /// A section of [`NASM_GUIDE`].
    Nasm(usize),
    /// Interrupt index and the function code, if known.
    Int(usize, Option<u16>),
}

impl Topic {
    /// Title and one line of details for the editor tooltip.
    pub fn tooltip(self) -> (String, String) {
        match self {
            Topic::Instr(i) => {
                let e = &INSTRUCTIONS[i];
                (format!("{}  {}", e.name, e.summary), flags_summary(e.flags))
            }
            Topic::Directive(i) => (DIRECTIVES[i].name.to_string(), DIRECTIVES[i].summary.to_string()),
            Topic::Nasm(i) => ("NASM syntax".to_string(), NASM_GUIDE[i].title.to_string()),
            Topic::Int(i, f) => {
                let int = &INTERRUPTS[i];
                let head = format!("INT {:02X}h  {}", int.number, int.title);
                match f.and_then(|c| int.functions.iter().find(|x| x.code == c)) {
                    Some(func) => {
                        (head, format!("{} = {}: {}", int.selector, hex(func.code, int.selector), func.title))
                    }
                    None if int.functions.is_empty() => (head, int.description.to_string()),
                    None => (head, format!("function number in {}", int.selector)),
                }
            }
        }
    }
}

/// Function code as written in the reference: `09h` for AH, `0003h` for AX.
pub fn hex(code: u16, selector: &str) -> String {
    if selector == "AX" { format!("{code:04X}h") } else { format!("{code:02X}h") }
}

/// Code part of a source line (without the comment), and whether char column `col` is inside
/// a string or a comment.
fn code_part(line: &str, col: usize) -> (&str, bool) {
    let mut quote = None;
    for (n, (b, c)) in line.char_indices().enumerate() {
        match (quote, c) {
            (None, '\'' | '"') => quote = Some(c),
            (Some(q), _) if q == c => quote = None,
            (None, ';') => return (&line[..b], col >= n),
            _ => {}
        }
        if n == col && quote.is_some() {
            return (line, true);
        }
    }
    (line, false)
}

/// Upper-case statement tokens after an optional `label:`.
fn statement(code: &str) -> Vec<String> {
    let code = match code.split_once(':') {
        Some((l, rest)) if !l.trim().is_empty() && !l.trim().contains(char::is_whitespace) && !l.contains('[') => rest,
        _ => code,
    };
    code.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).map(str::to_ascii_uppercase).collect()
}

/// Reference article for the word at char index `ci` of `text`: an instruction, a directive,
/// or the interrupt of `INT nn` with the function taken from the preceding `MOV AH, ..`.
pub fn topic_at(text: &str, ci: usize) -> Option<Topic> {
    let chars: Vec<char> = text.chars().collect();
    let is_id = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '@' | '?' | '$' | '.');
    if ci >= chars.len() || !is_id(chars[ci]) {
        return None;
    }
    let mut a = ci;
    while a > 0 && is_id(chars[a - 1]) {
        a -= 1;
    }
    let mut b = ci;
    while b < chars.len() && is_id(chars[b]) {
        b += 1;
    }
    let line_start = chars[..a].iter().rposition(|&c| c == '\n').map_or(0, |p| p + 1);
    let line_end = chars[b..].iter().position(|&c| c == '\n').map_or(chars.len(), |p| b + p);
    let line: String = chars[line_start..line_end].iter().collect();
    let line_no = chars[..a].iter().filter(|&&c| c == '\n').count();
    let (code, ignored) = code_part(&line, a - line_start);
    let word = chars[a..b].iter().collect::<String>().to_ascii_uppercase();
    // NASM preprocessor directives: %define, %macro, ...
    if a > 0 && chars[a - 1] == '%' {
        return find_directive(&format!("%{word}")).map(Topic::Directive);
    }
    // Emu8086 directives in #...#.
    if a > 0 && chars[a - 1] == '#' {
        let next = chars.get(b).copied().unwrap_or(' ');
        return find_directive(&format!("#{word}{}", if next == '=' { "=" } else { "#" })).map(Topic::Directive);
    }
    if ignored {
        return None;
    }
    let tokens = statement(code);
    let pos = tokens.iter().position(|t| *t == word)?;
    if tokens[0] == "INT" && tokens.len() > 1 {
        let n = rustemu_asm::evaluate(&tokens[1]).ok()?;
        let i = find_interrupt(u8::try_from(n).ok()?)?;
        let lines: Vec<&str> = text.lines().take(line_no).collect();
        return Some(Topic::Int(i, preceding_function(&lines, INTERRUPTS[i].selector)));
    }
    // Instructions only in the mnemonic position (AND, SHL, ... are also expression operators).
    let prefixes = ["REP", "REPE", "REPZ", "REPNE", "REPNZ", "LOCK"];
    let mnemonic_pos = pos == 0 || (pos == 1 && prefixes.contains(&tokens[0].as_str()));
    if mnemonic_pos && let Some(i) = find_instruction(&word) {
        return Some(Topic::Instr(i));
    }
    find_directive(&word).map(Topic::Directive)
}

/// Like [`topic_at`] for the text cursor, which may also stand right after the word.
pub fn topic_at_cursor(text: &str, ci: usize) -> Option<Topic> {
    topic_at(text, ci).or_else(|| ci.checked_sub(1).and_then(|c| topic_at(text, c)))
}

/// Value of the function selector (AH or AX) set by the lines before an `INT`, looking back
/// until a label, a jump or another call.
fn preceding_function(lines: &[&str], selector: &str) -> Option<u16> {
    if selector.is_empty() {
        return None;
    }
    for line in lines.iter().rev().take(16) {
        let (code, _) = code_part(line, usize::MAX);
        if code.trim().ends_with(':') {
            return None;
        }
        let t = statement(code);
        let Some(op) = t.first() else { continue };
        let dest = t.get(1).map(String::as_str);
        let src = t.get(2).map(String::as_str);
        let value = || rustemu_asm::evaluate(&t[2..].join(" ")).ok().map(|v| v as u16);
        match (op.as_str(), dest) {
            ("MOV", Some("AX")) if t.len() > 2 => return value().map(|v| if selector == "AX" { v } else { v >> 8 }),
            ("MOV", Some("AH")) if t.len() > 2 && selector == "AH" => return value(),
            ("XOR" | "SUB", Some("AX")) if src == Some("AX") => return Some(0),
            ("XOR" | "SUB", Some("AH")) if src == Some("AH") && selector == "AH" => return Some(0),
            ("INT" | "CALL" | "RET" | "RETF" | "IRET" | "LOOP", _) => return None,
            (j, _) if j.starts_with('J') => return None,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topics_in_source() {
        let src = "start: mov ah, 9 ; mov here\n lea dx, msg\n int 21h\n rep movsb\n mov ax, 1 shl 2\n\
                   msg db 'add$'\n#make_com#\n mov ax, 3\n int 33h\n jmp x\n int 10h";
        let at = |w: &str| topic_at(src, src.find(w).unwrap());
        let name = |t: Option<Topic>| match t {
            Some(Topic::Instr(i)) => INSTRUCTIONS[i].name.to_string(),
            Some(Topic::Directive(i)) => DIRECTIVES[i].name.to_string(),
            Some(Topic::Int(i, f)) => format!("INT {:X} {f:?}", INTERRUPTS[i].number),
            Some(Topic::Nasm(i)) => NASM_GUIDE[i].title.to_string(),
            None => "-".into(),
        };
        assert_eq!(name(at("mov ah")), "MOV");
        assert_eq!(name(at("mov here")), "-");
        assert_eq!(name(at("21h")), "INT 21 Some(9)");
        assert_eq!(name(at("int 21h")), "INT 21 Some(9)");
        assert_eq!(name(at("movsb")), "MOVSB");
        assert_eq!(name(at("rep")), "REP");
        assert_eq!(name(at("shl")), "-");
        assert_eq!(name(at("db")), "DB");
        assert_eq!(name(at("add$")), "-");
        assert_eq!(name(at("make_com")), "#MAKE_COM#");
        assert_eq!(name(at("33h")), "INT 33 Some(3)");
        assert_eq!(name(at("10h")), "INT 10 None");
        // The text cursor right after the word.
        assert_eq!(name(topic_at_cursor(src, src.find("lea").unwrap() + 3)), "LEA");
        assert_eq!(name(topic_at(src, src.find("lea").unwrap() + 3)), "-");
    }

    #[test]
    fn data_is_consistent() {
        for i in INSTRUCTIONS {
            assert_eq!(i.flags.len(), 9, "{}", i.name);
            assert!(i.flags.chars().all(|c| "r-u01".contains(c)), "{}", i.name);
            assert!(!i.summary.is_empty(), "{}", i.name);
        }
        // Every mnemonic the assembler knows has a reference entry.
        for m in [
            "MOV", "ADD", "JNZ", "JCXZ", "LOOPNE", "REPZ", "XLAT", "MOVSW", "SAL", "RETF", "IRET", "PUSHA", "AAM",
            "CMC", "SCASB", "LODSW", "STOSB", "IN", "OUT", "INT", "HLT", "LEAVE",
        ] {
            assert!(find_instruction(m).is_some(), "{m}");
        }
        assert_eq!(find_directive("#cs=0500"), find_directive("#LOAD_SEGMENT="));
        assert!(find_directive("db").is_some());
        assert!(find_interrupt(0x21).is_some());
        assert_eq!(flags_summary(LOGIC), "flags: O=0 S Z A? P C=0");
    }

    #[test]
    fn covers_all_assembler_mnemonics() {
        // Mnemonics accepted by the assembler (excluding prefixes handled by REP).
        let list = [
            "AAA", "AAS", "CBW", "CWD", "CLC", "CLD", "CLI", "CMC", "DAA", "DAS", "HLT", "INTO", "IRET", "LAHF",
            "SAHF", "NOP", "PUSHF", "POPF", "WAIT", "STC", "STD", "STI", "MOVSB", "MOVSW", "CMPSB", "CMPSW", "STOSB",
            "STOSW", "LODSB", "LODSW", "SCASB", "SCASW", "ADC", "SBB", "AND", "OR", "XOR", "CMP", "SUB", "ROL", "ROR",
            "RCL", "RCR", "SHL", "SHR", "SAR", "NOT", "NEG", "MUL", "IMUL", "DIV", "IDIV", "TEST", "XCHG", "INC",
            "DEC", "PUSH", "POP", "LEA", "LDS", "LES", "JMP", "CALL", "RET", "LOOP", "LOOPE", "LOOPZ", "AAD", "ESC",
            "LOCK", "JA", "JAE", "JB", "JBE", "JC", "JE", "JG", "JGE", "JL", "JLE", "JNA", "JNAE", "JNB", "JNBE",
            "JNC", "JNE", "JNG", "JNGE", "JNL", "JNLE", "JNO", "JNP", "JNS", "JO", "JP", "JPE", "JPO", "JS", "JZ",
            "POPA", "PUSHA", "LEAVE",
        ];
        for m in list {
            assert!(rustemu_asm::encode::is_mnemonic(m), "{m}");
            assert!(find_instruction(m).is_some(), "{m}");
        }
        // And every reference name is something the assembler accepts.
        let prefixes = ["REP", "REPE", "REPZ", "REPNE", "REPNZ", "LOCK"];
        for i in INSTRUCTIONS {
            for n in std::iter::once(&i.name).chain(i.aliases) {
                assert!(prefixes.contains(n) || rustemu_asm::encode::is_mnemonic(n), "{n}");
            }
        }
    }

    /// The examples of the NASM guide are recognized as NASM.
    #[test]
    fn nasm_guide_examples_are_nasm() {
        for s in NASM_GUIDE.iter().filter(|s| !s.example.is_empty()) {
            let d = rustemu_asm::detect_dialect("example.asm", s.example);
            assert_eq!(d, rustemu_asm::Dialect::Nasm, "{}", s.title);
        }
    }

    /// Every example assembles once the names it only refers to are defined (as a label, a
    /// byte or a word variable — whichever fits).
    #[test]
    fn examples_assemble() {
        let mut all: Vec<(String, &str)> = Vec::new();
        all.extend(INSTRUCTIONS.iter().map(|i| (i.name.to_string(), i.example)));
        all.extend(DIRECTIVES.iter().map(|d| (d.name.to_string(), d.example)));
        all.extend(NASM_GUIDE.iter().filter(|s| !s.example.is_empty()).map(|s| (s.title.to_string(), s.example)));
        for int in INTERRUPTS {
            all.extend(int.functions.iter().map(|f| (format!("INT {:X}h/{:X}h", int.number, f.code), f.example)));
        }
        let undefined = |src: &str| -> Result<Vec<String>, String> {
            match rustemu_asm::assemble_str(src) {
                Ok(_) => Ok(Vec::new()),
                Err(e) => {
                    let mut names = Vec::new();
                    for er in e.errors {
                        match er.msg.strip_prefix("undefined symbol '").and_then(|s| s.strip_suffix('\'')) {
                            Some(n) if !names.contains(&n.to_string()) => names.push(n.to_string()),
                            Some(_) => {}
                            None => return Err(er.to_string()),
                        }
                    }
                    Ok(names)
                }
            }
        };
        for (name, example) in all {
            // An include of a user file cannot be assembled here.
            if example.contains("%include") {
                continue;
            }
            // Examples with segments or their own ORG are complete programs.
            let own_org = example.lines().any(|l| l.trim_start().to_ascii_lowercase().starts_with("org "));
            let src = if example.contains("\nends") || example.contains(".model") || own_org {
                format!("{example}\n")
            } else {
                format!("org 100h\n{example}\n")
            };
            let missing = undefined(&src).unwrap_or_else(|e| panic!("{name}: {e}\n{src}"));
            let combos = 3usize.pow(missing.len() as u32);
            let ok = (0..combos).any(|mut c| {
                let mut s = src.clone();
                for m in &missing {
                    s.push_str(&[format!("{m}:\n"), format!("{m} db 0\n"), format!("{m} dw 0\n")][c % 3]);
                    c /= 3;
                }
                undefined(&s) == Ok(Vec::new())
            });
            assert!(ok, "{name}: example does not assemble with {missing:?} defined\n{src}");
        }
    }
}
