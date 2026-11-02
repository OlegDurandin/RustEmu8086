//! The instruction table of MARS: its basic instructions (example syntax and encoding mask from
//! `InstructionSet.java`) and pseudo-instructions (`PseudoOps.txt`, read at startup), without the
//! coprocessor 1 ones. MARS is distributed under the MIT license (see `data/PseudoOps.txt`).

/// The encoding format of a basic instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Register format.
    R,
    /// Immediate format.
    I,
    /// Immediate format with a branch target (encoded relative to the next instruction).
    B,
    /// Jump format (a word address in the same 256 MB region).
    J,
}

/// A basic instruction of MARS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Basic {
    /// Example syntax: the operand kinds of this form (`lw $t1,-100($t2)`).
    pub example: &'static str,
    /// 32 characters: `0` / `1` fixed bits, `f` / `s` / `t` the first / second / third operand,
    /// anything else zero.
    pub mask: &'static str,
    /// How the operands are encoded.
    pub format: Format,
    /// MARS's description.
    pub help: &'static str,
}

/// MARS's basic integer instructions (87 forms), in its order (forms of one mnemonic are tried in
/// this order).
pub const BASIC: &[Basic] = &[
    Basic {
        example: "nop",
        mask: "00000000000000000000000000000000",
        format: Format::R,
        help: "Null operation : machine code is all zeroes",
    },
    Basic {
        example: "add $t1,$t2,$t3",
        mask: "000000ssssstttttfffff00000100000",
        format: Format::R,
        help: "Addition with overflow : set $t1 to ($t2 plus $t3)",
    },
    Basic {
        example: "sub $t1,$t2,$t3",
        mask: "000000ssssstttttfffff00000100010",
        format: Format::R,
        help: "Subtraction with overflow : set $t1 to ($t2 minus $t3)",
    },
    Basic {
        example: "addi $t1,$t2,-100",
        mask: "001000sssssffffftttttttttttttttt",
        format: Format::I,
        help: "Addition immediate with overflow : set $t1 to ($t2 plus signed 16-bit immediate)",
    },
    Basic {
        example: "addu $t1,$t2,$t3",
        mask: "000000ssssstttttfffff00000100001",
        format: Format::R,
        help: "Addition unsigned without overflow : set $t1 to ($t2 plus $t3), no overflow",
    },
    Basic {
        example: "subu $t1,$t2,$t3",
        mask: "000000ssssstttttfffff00000100011",
        format: Format::R,
        help: "Subtraction unsigned without overflow : set $t1 to ($t2 minus $t3), no overflow",
    },
    Basic {
        example: "addiu $t1,$t2,-100",
        mask: "001001sssssffffftttttttttttttttt",
        format: Format::I,
        help: "Addition immediate unsigned without overflow : set $t1 to ($t2 plus signed 16-bit immediate), no overflow",
    },
    Basic {
        example: "mult $t1,$t2",
        mask: "000000fffffsssss0000000000011000",
        format: Format::R,
        help: "Multiplication : Set hi to high-order 32 bits, lo to low-order 32 bits of the product of $t1 and $t2 (use mfhi to access hi, mflo to access lo)",
    },
    Basic {
        example: "multu $t1,$t2",
        mask: "000000fffffsssss0000000000011001",
        format: Format::R,
        help: "Multiplication unsigned : Set HI to high-order 32 bits, LO to low-order 32 bits of the product of unsigned $t1 and $t2 (use mfhi to access HI, mflo to access LO)",
    },
    Basic {
        example: "mul $t1,$t2,$t3",
        mask: "011100ssssstttttfffff00000000010",
        format: Format::R,
        help: "Multiplication without overflow  : Set HI to high-order 32 bits, LO and $t1 to low-order 32 bits of the product of $t2 and $t3 (use mfhi to access HI, mflo to access LO)",
    },
    Basic {
        example: "madd $t1,$t2",
        mask: "011100fffffsssss0000000000000000",
        format: Format::R,
        help: "Multiply add : Multiply $t1 by $t2 then increment HI by high-order 32 bits of product, increment LO by low-order 32 bits of product (use mfhi to access HI, mflo to access LO)",
    },
    Basic {
        example: "maddu $t1,$t2",
        mask: "011100fffffsssss0000000000000001",
        format: Format::R,
        help: "Multiply add unsigned : Multiply $t1 by $t2 then increment HI by high-order 32 bits of product, increment LO by low-order 32 bits of product, unsigned (use mfhi to access HI, mflo to access LO)",
    },
    Basic {
        example: "msub $t1,$t2",
        mask: "011100fffffsssss0000000000000100",
        format: Format::R,
        help: "Multiply subtract : Multiply $t1 by $t2 then decrement HI by high-order 32 bits of product, decrement LO by low-order 32 bits of product (use mfhi to access HI, mflo to access LO)",
    },
    Basic {
        example: "msubu $t1,$t2",
        mask: "011100fffffsssss0000000000000101",
        format: Format::R,
        help: "Multiply subtract unsigned : Multiply $t1 by $t2 then decrement HI by high-order 32 bits of product, decement LO by low-order 32 bits of product, unsigned (use mfhi to access HI, mflo to access LO)",
    },
    Basic {
        example: "div $t1,$t2",
        mask: "000000fffffsssss0000000000011010",
        format: Format::R,
        help: "Division with overflow : Divide $t1 by $t2 then set LO to quotient and HI to remainder (use mfhi to access HI, mflo to access LO)",
    },
    Basic {
        example: "divu $t1,$t2",
        mask: "000000fffffsssss0000000000011011",
        format: Format::R,
        help: "Division unsigned without overflow : Divide unsigned $t1 by $t2 then set LO to quotient and HI to remainder (use mfhi to access HI, mflo to access LO)",
    },
    Basic {
        example: "mfhi $t1",
        mask: "0000000000000000fffff00000010000",
        format: Format::R,
        help: "Move from HI register : Set $t1 to contents of HI (see multiply and divide operations)",
    },
    Basic {
        example: "mflo $t1",
        mask: "0000000000000000fffff00000010010",
        format: Format::R,
        help: "Move from LO register : Set $t1 to contents of LO (see multiply and divide operations)",
    },
    Basic {
        example: "mthi $t1",
        mask: "000000fffff000000000000000010001",
        format: Format::R,
        help: "Move to HI registerr : Set HI to contents of $t1 (see multiply and divide operations)",
    },
    Basic {
        example: "mtlo $t1",
        mask: "000000fffff000000000000000010011",
        format: Format::R,
        help: "Move to LO register : Set LO to contents of $t1 (see multiply and divide operations)",
    },
    Basic {
        example: "and $t1,$t2,$t3",
        mask: "000000ssssstttttfffff00000100100",
        format: Format::R,
        help: "Bitwise AND : Set $t1 to bitwise AND of $t2 and $t3",
    },
    Basic {
        example: "or $t1,$t2,$t3",
        mask: "000000ssssstttttfffff00000100101",
        format: Format::R,
        help: "Bitwise OR : Set $t1 to bitwise OR of $t2 and $t3",
    },
    Basic {
        example: "andi $t1,$t2,100",
        mask: "001100sssssffffftttttttttttttttt",
        format: Format::I,
        help: "Bitwise AND immediate : Set $t1 to bitwise AND of $t2 and zero-extended 16-bit immediate",
    },
    Basic {
        example: "ori $t1,$t2,100",
        mask: "001101sssssffffftttttttttttttttt",
        format: Format::I,
        help: "Bitwise OR immediate : Set $t1 to bitwise OR of $t2 and zero-extended 16-bit immediate",
    },
    Basic {
        example: "nor $t1,$t2,$t3",
        mask: "000000ssssstttttfffff00000100111",
        format: Format::R,
        help: "Bitwise NOR : Set $t1 to bitwise NOR of $t2 and $t3",
    },
    Basic {
        example: "xor $t1,$t2,$t3",
        mask: "000000ssssstttttfffff00000100110",
        format: Format::R,
        help: "Bitwise XOR (exclusive OR) : Set $t1 to bitwise XOR of $t2 and $t3",
    },
    Basic {
        example: "xori $t1,$t2,100",
        mask: "001110sssssffffftttttttttttttttt",
        format: Format::I,
        help: "Bitwise XOR immediate : Set $t1 to bitwise XOR of $t2 and zero-extended 16-bit immediate",
    },
    Basic {
        example: "sll $t1,$t2,10",
        mask: "00000000000sssssfffffttttt000000",
        format: Format::R,
        help: "Shift left logical : Set $t1 to result of shifting $t2 left by number of bits specified by immediate",
    },
    Basic {
        example: "sllv $t1,$t2,$t3",
        mask: "000000tttttsssssfffff00000000100",
        format: Format::R,
        help: "Shift left logical variable : Set $t1 to result of shifting $t2 left by number of bits specified by value in low-order 5 bits of $t3",
    },
    Basic {
        example: "srl $t1,$t2,10",
        mask: "00000000000sssssfffffttttt000010",
        format: Format::R,
        help: "Shift right logical : Set $t1 to result of shifting $t2 right by number of bits specified by immediate",
    },
    Basic {
        example: "sra $t1,$t2,10",
        mask: "00000000000sssssfffffttttt000011",
        format: Format::R,
        help: "Shift right arithmetic : Set $t1 to result of sign-extended shifting $t2 right by number of bits specified by immediate",
    },
    Basic {
        example: "srav $t1,$t2,$t3",
        mask: "000000tttttsssssfffff00000000111",
        format: Format::R,
        help: "Shift right arithmetic variable : Set $t1 to result of sign-extended shifting $t2 right by number of bits specified by value in low-order 5 bits of $t3",
    },
    Basic {
        example: "srlv $t1,$t2,$t3",
        mask: "000000tttttsssssfffff00000000110",
        format: Format::R,
        help: "Shift right logical variable : Set $t1 to result of shifting $t2 right by number of bits specified by value in low-order 5 bits of $t3",
    },
    Basic {
        example: "lw $t1,-100($t2)",
        mask: "100011tttttfffffssssssssssssssss",
        format: Format::I,
        help: "Load word : Set $t1 to contents of effective memory word address",
    },
    Basic {
        example: "ll $t1,-100($t2)",
        mask: "110000tttttfffffssssssssssssssss",
        format: Format::I,
        help: "Load linked : Paired with Store Conditional (sc) to perform atomic read-modify-write.  Treated as equivalent to Load Word (lw) because MARS does not simulate multiple processors.",
    },
    Basic {
        example: "lwl $t1,-100($t2)",
        mask: "100010tttttfffffssssssssssssssss",
        format: Format::I,
        help: "Load word left : Load from 1 to 4 bytes left-justified into $t1, starting with effective memory byte address and continuing through the low-order byte of its word",
    },
    Basic {
        example: "lwr $t1,-100($t2)",
        mask: "100110tttttfffffssssssssssssssss",
        format: Format::I,
        help: "Load word right : Load from 1 to 4 bytes right-justified into $t1, starting with effective memory byte address and continuing through the high-order byte of its word",
    },
    Basic {
        example: "sw $t1,-100($t2)",
        mask: "101011tttttfffffssssssssssssssss",
        format: Format::I,
        help: "Store word : Store contents of $t1 into effective memory word address",
    },
    Basic {
        example: "sc $t1,-100($t2)",
        mask: "111000tttttfffffssssssssssssssss",
        format: Format::I,
        help: "Store conditional : Paired with Load Linked (ll) to perform atomic read-modify-write.  Stores $t1 value into effective address, then sets $t1 to 1 for success.  Always succeeds because MARS does not simulate multiple processors.",
    },
    Basic {
        example: "swl $t1,-100($t2)",
        mask: "101010tttttfffffssssssssssssssss",
        format: Format::I,
        help: "Store word left : Store high-order 1 to 4 bytes of $t1 into memory, starting with effective byte address and continuing through the low-order byte of its word",
    },
    Basic {
        example: "swr $t1,-100($t2)",
        mask: "101110tttttfffffssssssssssssssss",
        format: Format::I,
        help: "Store word right : Store low-order 1 to 4 bytes of $t1 into memory, starting with high-order byte of word containing effective byte address and continuing through that byte address",
    },
    Basic {
        example: "lui $t1,100",
        mask: "00111100000fffffssssssssssssssss",
        format: Format::I,
        help: "Load upper immediate : Set high-order 16 bits of $t1 to 16-bit immediate and low-order 16 bits to 0",
    },
    Basic {
        example: "beq $t1,$t2,label",
        mask: "000100fffffssssstttttttttttttttt",
        format: Format::B,
        help: "Branch if equal : Branch to statement at label's address if $t1 and $t2 are equal",
    },
    Basic {
        example: "bne $t1,$t2,label",
        mask: "000101fffffssssstttttttttttttttt",
        format: Format::B,
        help: "Branch if not equal : Branch to statement at label's address if $t1 and $t2 are not equal",
    },
    Basic {
        example: "bgez $t1,label",
        mask: "000001fffff00001ssssssssssssssss",
        format: Format::B,
        help: "Branch if greater than or equal to zero : Branch to statement at label's address if $t1 is greater than or equal to zero",
    },
    Basic {
        example: "bgezal $t1,label",
        mask: "000001fffff10001ssssssssssssssss",
        format: Format::B,
        help: "Branch if greater then or equal to zero and link : If $t1 is greater than or equal to zero, then set $ra to the Program Counter and branch to statement at label's address",
    },
    Basic {
        example: "bgtz $t1,label",
        mask: "000111fffff00000ssssssssssssssss",
        format: Format::B,
        help: "Branch if greater than zero : Branch to statement at label's address if $t1 is greater than zero",
    },
    Basic {
        example: "blez $t1,label",
        mask: "000110fffff00000ssssssssssssssss",
        format: Format::B,
        help: "Branch if less than or equal to zero : Branch to statement at label's address if $t1 is less than or equal to zero",
    },
    Basic {
        example: "bltz $t1,label",
        mask: "000001fffff00000ssssssssssssssss",
        format: Format::B,
        help: "Branch if less than zero : Branch to statement at label's address if $t1 is less than zero",
    },
    Basic {
        example: "bltzal $t1,label",
        mask: "000001fffff10000ssssssssssssssss",
        format: Format::B,
        help: "Branch if less than zero and link : If $t1 is less than or equal to zero, then set $ra to the Program Counter and branch to statement at label's address",
    },
    Basic {
        example: "slt $t1,$t2,$t3",
        mask: "000000ssssstttttfffff00000101010",
        format: Format::R,
        help: "Set less than : If $t2 is less than $t3, then set $t1 to 1 else set $t1 to 0",
    },
    Basic {
        example: "sltu $t1,$t2,$t3",
        mask: "000000ssssstttttfffff00000101011",
        format: Format::R,
        help: "Set less than unsigned : If $t2 is less than $t3 using unsigned comparision, then set $t1 to 1 else set $t1 to 0",
    },
    Basic {
        example: "slti $t1,$t2,-100",
        mask: "001010sssssffffftttttttttttttttt",
        format: Format::I,
        help: "Set less than immediate : If $t2 is less than sign-extended 16-bit immediate, then set $t1 to 1 else set $t1 to 0",
    },
    Basic {
        example: "sltiu $t1,$t2,-100",
        mask: "001011sssssffffftttttttttttttttt",
        format: Format::I,
        help: "Set less than immediate unsigned : If $t2 is less than  sign-extended 16-bit immediate using unsigned comparison, then set $t1 to 1 else set $t1 to 0",
    },
    Basic {
        example: "movn $t1,$t2,$t3",
        mask: "000000ssssstttttfffff00000001011",
        format: Format::R,
        help: "Move conditional not zero : Set $t1 to $t2 if $t3 is not zero",
    },
    Basic {
        example: "movz $t1,$t2,$t3",
        mask: "000000ssssstttttfffff00000001010",
        format: Format::R,
        help: "Move conditional zero : Set $t1 to $t2 if $t3 is zero",
    },
    Basic {
        example: "break 100",
        mask: "000000ffffffffffffffffffff001101",
        format: Format::R,
        help: "Break execution with code : Terminate program execution with specified exception code",
    },
    Basic {
        example: "break",
        mask: "00000000000000000000000000001101",
        format: Format::R,
        help: "Break execution : Terminate program execution with exception",
    },
    Basic {
        example: "syscall",
        mask: "000000cccccccccccccccccccc001100",
        format: Format::R,
        help: "Issue a system call : Execute the system call specified by value in $v0",
    },
    Basic {
        example: "j target",
        mask: "000010ffffffffffffffffffffffffff",
        format: Format::J,
        help: "Jump unconditionally : Jump to statement at target address",
    },
    Basic {
        example: "jr $t1",
        mask: "000000fffff000000000000000001000",
        format: Format::R,
        help: "Jump register unconditionally : Jump to statement whose address is in $t1",
    },
    Basic {
        example: "jal target",
        mask: "000011ffffffffffffffffffffffffff",
        format: Format::J,
        help: "Jump and link : Set $ra to Program Counter (return address) then jump to statement at target address",
    },
    Basic {
        example: "jalr $t1,$t2",
        mask: "000000sssss00000fffff00000001001",
        format: Format::R,
        help: "Jump and link register : Set $t1 to Program Counter (return address) then jump to statement whose address is in $t2",
    },
    Basic {
        example: "jalr $t1",
        mask: "000000fffff000001111100000001001",
        format: Format::R,
        help: "Jump and link register : Set $ra to Program Counter (return address) then jump to statement whose address is in $t1",
    },
    Basic {
        example: "lb $t1,-100($t2)",
        mask: "100000tttttfffffssssssssssssssss",
        format: Format::I,
        help: "Load byte : Set $t1 to sign-extended 8-bit value from effective memory byte address",
    },
    Basic {
        example: "lh $t1,-100($t2)",
        mask: "100001tttttfffffssssssssssssssss",
        format: Format::I,
        help: "Load halfword : Set $t1 to sign-extended 16-bit value from effective memory halfword address",
    },
    Basic {
        example: "lhu $t1,-100($t2)",
        mask: "100101tttttfffffssssssssssssssss",
        format: Format::I,
        help: "Load halfword unsigned : Set $t1 to zero-extended 16-bit value from effective memory halfword address",
    },
    Basic {
        example: "lbu $t1,-100($t2)",
        mask: "100100tttttfffffssssssssssssssss",
        format: Format::I,
        help: "Load byte unsigned : Set $t1 to zero-extended 8-bit value from effective memory byte address",
    },
    Basic {
        example: "sb $t1,-100($t2)",
        mask: "101000tttttfffffssssssssssssssss",
        format: Format::I,
        help: "Store byte : Store the low-order 8 bits of $t1 into the effective memory byte address",
    },
    Basic {
        example: "sh $t1,-100($t2)",
        mask: "101001tttttfffffssssssssssssssss",
        format: Format::I,
        help: "Store halfword : Store the low-order 16 bits of $t1 into the effective memory halfword address",
    },
    Basic {
        example: "clo $t1,$t2",
        mask: "011100sssss00000fffff00000100001",
        format: Format::R,
        help: "Count number of leading ones : Set $t1 to the count of leading one bits in $t2 starting at most significant bit position",
    },
    Basic {
        example: "clz $t1,$t2",
        mask: "011100sssss00000fffff00000100000",
        format: Format::R,
        help: "Count number of leading zeroes : Set $t1 to the count of leading zero bits in $t2 starting at most significant bit positio",
    },
    Basic {
        example: "mfc0 $t1,$8",
        mask: "01000000000fffffsssss00000000000",
        format: Format::R,
        help: "Move from Coprocessor 0 : Set $t1 to the value stored in Coprocessor 0 register $8",
    },
    Basic {
        example: "mtc0 $t1,$8",
        mask: "01000000100fffffsssss00000000000",
        format: Format::R,
        help: "Move to Coprocessor 0 : Set Coprocessor 0 register $8 to value stored in $t1",
    },
    Basic {
        example: "teq $t1,$t2",
        mask: "000000fffffssssscccccccccc110100",
        format: Format::R,
        help: "Trap if equal : Trap if $t1 is equal to $t2",
    },
    Basic {
        example: "teqi $t1,-100",
        mask: "000001fffff01100ssssssssssssssss",
        format: Format::I,
        help: "Trap if equal to immediate : Trap if $t1 is equal to sign-extended 16 bit immediate",
    },
    Basic {
        example: "tne $t1,$t2",
        mask: "000000fffffssssscccccccccc110110",
        format: Format::R,
        help: "Trap if not equal : Trap if $t1 is not equal to $t2",
    },
    Basic {
        example: "tnei $t1,-100",
        mask: "000001fffff01110ssssssssssssssss",
        format: Format::I,
        help: "Trap if not equal to immediate : Trap if $t1 is not equal to sign-extended 16 bit immediate",
    },
    Basic {
        example: "tge $t1,$t2",
        mask: "000000fffffssssscccccccccc110000",
        format: Format::R,
        help: "Trap if greater or equal : Trap if $t1 is greater than or equal to $t2",
    },
    Basic {
        example: "tgeu $t1,$t2",
        mask: "000000fffffssssscccccccccc110001",
        format: Format::R,
        help: "Trap if greater or equal unsigned : Trap if $t1 is greater than or equal to $t2 using unsigned comparision",
    },
    Basic {
        example: "tgei $t1,-100",
        mask: "000001fffff01000ssssssssssssssss",
        format: Format::I,
        help: "Trap if greater than or equal to immediate : Trap if $t1 greater than or equal to sign-extended 16 bit immediate",
    },
    Basic {
        example: "tgeiu $t1,-100",
        mask: "000001fffff01001ssssssssssssssss",
        format: Format::I,
        help: "Trap if greater or equal to immediate unsigned : Trap if $t1 greater than or equal to sign-extended 16 bit immediate, unsigned comparison",
    },
    Basic {
        example: "tlt $t1,$t2",
        mask: "000000fffffssssscccccccccc110010",
        format: Format::R,
        help: "Trap if less than: Trap if $t1 less than $t2",
    },
    Basic {
        example: "tltu $t1,$t2",
        mask: "000000fffffssssscccccccccc110011",
        format: Format::R,
        help: "Trap if less than unsigned : Trap if $t1 less than $t2, unsigned comparison",
    },
    Basic {
        example: "tlti $t1,-100",
        mask: "000001fffff01010ssssssssssssssss",
        format: Format::I,
        help: "Trap if less than immediate : Trap if $t1 less than sign-extended 16-bit immediate",
    },
    Basic {
        example: "tltiu $t1,-100",
        mask: "000001fffff01011ssssssssssssssss",
        format: Format::I,
        help: "Trap if less than immediate unsigned : Trap if $t1 less than sign-extended 16-bit immediate, unsigned comparison",
    },
    Basic {
        example: "eret",
        mask: "01000010000000000000000000011000",
        format: Format::R,
        help: "Exception return : Set Program Counter to Coprocessor 0 EPC register value, set Coprocessor Status register bit 1 (exception level) to zero",
    },
];
