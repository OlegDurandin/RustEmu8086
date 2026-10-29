//! The 8086/80186 instructions: syntax, flags, description and an example.

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

pub(super) const NONE: &str = "---------";
pub(super) const ARITH: &str = "r---rrrrr";
pub(super) const LOGIC: &str = "0---rrur0";

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
