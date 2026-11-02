//! Forms of instructions and how a source statement picks one (MARS's `OperandFormat`), and the
//! expansion of pseudo-instructions (MARS's `ExtendedInstruction.makeTemplateSubstitutions`).

use std::collections::HashMap;
use std::sync::OnceLock;

use super::basic::{BASIC, Basic};
use super::lexer::{IntClass, Kind, Token, parse_int, tokenize};

/// The pseudo-instructions of MARS.
const PSEUDO_OPS: &str = include_str!("../../data/PseudoOps.txt");

/// What an operand of a form must be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand {
    /// A register.
    Reg,
    /// An integer of this class.
    Int(IntClass),
    /// A label.
    Label,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `+`
    Plus,
}

/// How a form is built.
#[derive(Debug, Clone)]
pub enum Def {
    /// One machine instruction.
    Basic(&'static Basic),
    /// Basic instructions made from templates.
    Pseudo {
        /// The templates (MARS syntax, `DBNOP` ones included).
        templates: Vec<String>,
        /// MARS's description.
        help: String,
    },
}

/// One form of an instruction: its operands and how it is built.
#[derive(Debug, Clone)]
pub struct Form {
    /// The example syntax (`li $t1,100000`), for messages.
    pub example: String,
    /// The operands after the mnemonic, parentheses and `+` included.
    pub operands: Vec<Operand>,
    /// Basic or pseudo.
    pub def: Def,
}

impl Form {
    /// Number of machine instructions (delay-slot `nop`s are not generated: no delayed branching).
    pub fn words(&self) -> u32 {
        match &self.def {
            Def::Basic(_) => 1,
            Def::Pseudo { templates, .. } => templates.iter().filter(|t| !t.contains("DBNOP")).count() as u32,
        }
    }
}

/// All forms by lower-case mnemonic: basic ones first, then pseudo ones (MARS's order).
pub struct Table {
    forms: HashMap<String, Vec<Form>>,
}

/// The operand kinds of an example, None when it uses coprocessor 1.
fn example_operands(example: &str) -> Option<(String, Vec<Operand>)> {
    let tokens = tokenize(example).ok()?;
    let (mnemonic, rest) = tokens.split_first()?;
    let ops = rest
        .iter()
        .map(|t| match t.kind {
            Kind::RegName(_) | Kind::RegNum(_) => Some(Operand::Reg),
            Kind::Int(v) => Some(Operand::Int(IntClass::of(v))),
            Kind::Ident => Some(Operand::Label),
            Kind::LParen => Some(Operand::LParen),
            Kind::RParen => Some(Operand::RParen),
            Kind::Plus => Some(Operand::Plus),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some((mnemonic.text.to_ascii_lowercase(), ops))
}

/// Mnemonics of coprocessor 1 (not supported yet).
fn is_fpu(mnemonic: &str) -> bool {
    mnemonic.contains('.')
        || matches!(mnemonic, "bc1f" | "bc1t" | "mfc1" | "mtc1" | "lwc1" | "swc1" | "ldc1" | "sdc1" | "movf" | "movt")
}

impl Table {
    fn build() -> Table {
        let mut forms: HashMap<String, Vec<Form>> = HashMap::new();
        for b in BASIC {
            let (m, operands) = example_operands(b.example).expect("basic examples are valid");
            forms.entry(m).or_default().push(Form { example: b.example.into(), operands, def: Def::Basic(b) });
        }
        for line in PSEUDO_OPS.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let mut parts = line.split('\t').filter(|p| !p.is_empty());
            let Some(spec) = parts.next() else { continue };
            let mut templates = Vec::new();
            let mut help = String::new();
            for p in parts {
                if let Some(h) = p.strip_prefix('#') {
                    help = h.trim().to_string();
                    break;
                }
                // The second template set is for compact memory configurations.
                if p.trim() == "COMPACT" {
                    help = line.split("\t#").nth(1).unwrap_or("").trim().to_string();
                    break;
                }
                // MARS's table forgets the base register of the second instruction of
                // `swr $t1,100` (MARS then emits a nop); the other forms have it.
                let p = if spec == "swr $t1,100" && p.trim() == "swr RG1, 0" { "swr RG1, 0($1)" } else { p.trim() };
                templates.push(p.to_string());
            }
            let Some((m, operands)) = example_operands(spec) else { continue };
            if is_fpu(&m) || templates.iter().any(|t| t.contains("$f") || is_fpu(t.split(' ').next().unwrap_or(""))) {
                continue;
            }
            forms.entry(m).or_default().push(Form {
                example: spec.into(),
                operands,
                def: Def::Pseudo { templates, help },
            });
        }
        Table { forms }
    }

    /// The table (built once).
    pub fn get() -> &'static Table {
        static TABLE: OnceLock<Table> = OnceLock::new();
        TABLE.get_or_init(Table::build)
    }

    /// The forms of a mnemonic (case-insensitive), in the order they are tried.
    pub fn forms(&self, mnemonic: &str) -> Option<&[Form]> {
        self.forms.get(&mnemonic.to_ascii_lowercase()).map(|f| &f[..])
    }

    /// Every mnemonic.
    pub fn mnemonics(&self) -> impl Iterator<Item = &str> {
        self.forms.keys().map(|k| k.as_str())
    }
}

/// The kind of a statement operand for matching. `%hi` / `%lo` stand for integers whose value
/// is known only in the second pass: they match like a small integer.
fn operand_of(t: &Token) -> Option<(Operand, i32)> {
    Some(match &t.kind {
        Kind::RegName(_) | Kind::RegNum(_) => (Operand::Reg, 0),
        Kind::Int(v) => (Operand::Int(IntClass::of(*v)), *v),
        Kind::Reloc { .. } => (Operand::Int(IntClass::I5), 0),
        Kind::Ident => (Operand::Label, 0),
        Kind::LParen => (Operand::LParen, 0),
        Kind::RParen => (Operand::RParen, 0),
        Kind::Plus => (Operand::Plus, 0),
        _ => return None,
    })
}

/// Whether the statement operands `ops` fit `form` (MARS's `tokenOperandMatch`); the error is
/// `(index of the operand, message)`, index None for the operand count.
pub fn check(ops: &[Token], form: &Form) -> Result<(), (Option<usize>, String)> {
    use IntClass::*;
    if ops.len() != form.operands.len() {
        let what = if ops.len() < form.operands.len() { "few" } else { "many" };
        return Err((None, format!("Too {what} or incorrectly formatted operands. Expected: {}", form.example)));
    }
    for (i, (t, &spec)) in ops.iter().zip(&form.operands).enumerate() {
        let err = |m: &str| Err((Some(i), format!("\"{}\": {m}", t.text)));
        let Some((cand, v)) = operand_of(t) else { return err("operand is of incorrect type") };
        let ok = match (spec, cand) {
            (Operand::Int(s), Operand::Int(c)) => match (s, c) {
                _ if s == c => true,
                (I16 | I16U | I32, I5) | (I32, I16U | I16) => true,
                (I16, I16U) if (-32768..=32767).contains(&v) => true,
                (I16U, I16) if (0..=65535).contains(&v) => true,
                _ => return err("operand is out of range"),
            },
            (s, c) => s == c,
        };
        if !ok {
            return err("operand is of incorrect type");
        }
    }
    Ok(())
}

/// The first form of `forms` the operands fit (MARS's first fit), or the error of the first form.
pub fn choose<'a>(ops: &[Token], forms: &'a [Form]) -> Result<&'a Form, (Option<usize>, String)> {
    forms.iter().find(|f| check(ops, f).is_ok()).ok_or_else(|| check(ops, &forms[0]).unwrap_err())
}

/// `Binary.stringToInt` of a token text, 0 when it is not an integer.
fn int(s: &str) -> i32 {
    parse_int(s).unwrap_or(0)
}

/// Bit 15 of `v` (the `+1` of the upper half when the lower half is used sign-extended).
fn bit15(v: i32) -> i32 {
    (v >> 15) & 1
}

/// The instruction a pseudo-instruction template stands for, with the operands of the statement:
/// `tokens` are the statement's tokens, the mnemonic at 0, labels already replaced by their
/// addresses; `label` is the label of the last operand (for `LAB`). None for a template that
/// generates nothing (`DBNOP`: no delayed branching).
pub fn substitute(template: &str, tokens: &[&str], label: &str) -> Option<String> {
    if template.contains("DBNOP") {
        return None;
    }
    let mut s = template.to_string();
    let value = |n: usize| tokens.get(n).copied().unwrap_or("");
    for op in 1..tokens.len() {
        let v = value(op);
        s = s.replace(&format!("RG{op}"), v).replace(&format!("OP{op}"), v);
        if let Some(i) = s.find(&format!("LH{op}P")) {
            let add = s.as_bytes()[i + 4] as i32 - b'0' as i32;
            let a = int(v).wrapping_add(add);
            s = s.replace(&format!("LH{op}P{add}"), &((a >> 16) + bit15(a)).to_string());
        }
        if s.contains(&format!("LH{op}")) {
            let a = int(v);
            s = s.replace(&format!("LH{op}"), &((a >> 16) + bit15(a)).to_string());
        }
        if let Some(i) = s.find(&format!("LL{op}P")) {
            let add = s.as_bytes()[i + 4] as i32 - b'0' as i32;
            let a = int(v).wrapping_add(add);
            s = s.replace(&format!("LL{op}P{add}"), &((a << 16) >> 16).to_string());
        }
        if let Some(i) = s.find(&format!("LL{op}")) {
            let a = int(v);
            if s.as_bytes().get(i + 3) == Some(&b'U') {
                s = s.replace(&format!("LL{op}U"), &(a & 0xFFFF).to_string());
            } else {
                s = s.replace(&format!("LL{op}"), &((a << 16) >> 16).to_string());
            }
        }
        if let Some(i) = s.find(&format!("VHL{op}P")) {
            let add = s.as_bytes()[i + 5] as i32 - b'0' as i32;
            let a = int(v).wrapping_add(add);
            s = s.replace(&format!("VHL{op}P{add}"), &(a >> 16).to_string());
        }
        if let Some(i) = s.find(&format!("VH{op}P")) {
            let add = s.as_bytes()[i + 4] as i32 - b'0' as i32;
            let a = int(v).wrapping_add(add);
            s = s.replace(&format!("VH{op}P{add}"), &((a >> 16) + bit15(a)).to_string());
        }
        if s.contains(&format!("VH{op}")) {
            let a = int(v);
            s = s.replace(&format!("VH{op}"), &((a >> 16) + bit15(a)).to_string());
        }
        if let Some(i) = s.find(&format!("VL{op}P")) {
            let add = s.as_bytes()[i + 4] as i32 - b'0' as i32;
            let a = int(v).wrapping_add(add);
            if s.as_bytes().get(i + 5) == Some(&b'U') {
                s = s.replace(&format!("VL{op}P{add}U"), &(a & 0xFFFF).to_string());
            } else {
                s = s.replace(&format!("VL{op}P{add}"), &((a << 16) >> 16).to_string());
            }
        }
        if let Some(i) = s.find(&format!("VL{op}")) {
            let a = int(v);
            if s.as_bytes().get(i + 3) == Some(&b'U') {
                s = s.replace(&format!("VL{op}U"), &(a & 0xFFFF).to_string());
            } else {
                s = s.replace(&format!("VL{op}"), &((a << 16) >> 16).to_string());
            }
        }
        if s.contains(&format!("VHL{op}")) {
            s = s.replace(&format!("VHL{op}"), &(int(v) >> 16).to_string());
        }
    }
    if s.contains("LHL") {
        s = s.replace("LHL", &(int(value(2)) >> 16).to_string());
    }
    let label_plus = || int(value(2)).wrapping_add(int(value(4)));
    if let Some(i) = s.find("LHPAP") {
        let add = s.as_bytes()[i + 5] as i32 - b'0' as i32;
        let a = label_plus().wrapping_add(add);
        s = s.replace(&format!("LHPAP{add}"), &((a >> 16) + bit15(a)).to_string());
    }
    if s.contains("LHPA") {
        let a = label_plus();
        s = s.replace("LHPA", &((a >> 16) + bit15(a)).to_string());
    }
    if s.contains("LHPN") {
        s = s.replace("LHPN", &(label_plus() >> 16).to_string());
    }
    if let Some(i) = s.find("LLPP") {
        let add = s.as_bytes()[i + 4] as i32 - b'0' as i32;
        let a = label_plus().wrapping_add(add);
        s = s.replace(&format!("LLPP{add}"), &((a << 16) >> 16).to_string());
    }
    if let Some(i) = s.find("LLP") {
        let a = label_plus();
        if s.as_bytes().get(i + 3) == Some(&b'U') {
            s = s.replace("LLPU", &(a & 0xFFFF).to_string());
        } else {
            s = s.replace("LLP", &((a << 16) >> 16).to_string());
        }
    }
    if let Some(i) = s.find("BROFF") {
        // BROFFnm: n without delayed branching (MARS's default), m with it.
        let disabled = s.get(i + 5..i + 6).unwrap_or("0").to_string();
        s = s.replacen(&s[i..i + 7].to_string(), &disabled, 1);
    }
    if s.contains("NR") {
        for op in 1..tokens.len() {
            if let Ok(t) = tokenize(value(op))
                && let [Token { kind: Kind::RegName(r) | Kind::RegNum(r), .. }] = t.as_slice()
            {
                s = s.replace(&format!("NR{op}"), &format!("${}", r + 1));
            }
        }
    }
    if s.contains("S32") {
        s = s.replace("S32", &(32 - int(value(tokens.len() - 1))).to_string());
    }
    if s.contains("LAB") {
        s = s.replacen("LAB", label, 1);
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops(line: &str) -> Vec<Token> {
        tokenize(line).unwrap().split_off(1)
    }

    fn chosen(line: &str) -> String {
        let t = Table::get();
        let m = line.split(' ').next().unwrap();
        choose(&ops(line), t.forms(m).unwrap()).map(|f| f.example.clone()).unwrap_or_else(|e| e.1)
    }

    #[test]
    fn table_has_mars_instructions() {
        let t = Table::get();
        assert_eq!(t.forms("add").unwrap().len(), 3, "basic add and two pseudo forms");
        assert!(t.forms("li").is_some() && t.forms("la").is_some() && t.forms("blt").is_some());
        assert!(t.forms("add.s").is_none(), "coprocessor 1 is left out");
        assert!(t.mnemonics().count() > 120);
    }

    #[test]
    fn first_fit_by_operand_class() {
        assert_eq!(chosen("li $t0, 5"), "li $t1,-100");
        assert_eq!(chosen("li $t0, 40000"), "li $t1,100");
        assert_eq!(chosen("li $t0, 100000"), "li $t1,100000");
        assert_eq!(chosen("addi $t0, $t1, 5"), "addi $t1,$t2,-100");
        assert_eq!(chosen("addi $t0, $t1, 100000"), "addi $t1,$t2,100000");
        assert_eq!(chosen("lw $t0, 4($sp)"), "lw $t1,-100($t2)");
        assert_eq!(chosen("lw $t0, x"), "lw $t1,label");
        assert_eq!(chosen("lw $t0, x+8"), "lw $t1,label+100000");
        assert_eq!(chosen("sll $t0, $t1, 40"), "\"40\": operand is out of range");
        assert_eq!(chosen("add $t0, $t1"), "Too few or incorrectly formatted operands. Expected: add $t1,$t2,$t3");
        assert_eq!(chosen("lui $t0, %hi(x)"), "lui $t1,100");
    }

    #[test]
    fn templates_like_mars() {
        let toks = ["li", "$t0", "100000"];
        assert_eq!(substitute("lui $1, VHL2", &toks, ""), Some("lui $1, 1".into()));
        assert_eq!(substitute("ori RG1, $1, VL2U", &toks, ""), Some("ori $t0, $1, 34464".into()));
        // la $t0, label at 0x10018000: the upper half is not rounded (ori), lw's is (signed offset).
        let toks = ["la", "$t0", &(0x1001_8000u32 as i32).to_string()];
        assert_eq!(substitute("lui $1, LHL", &toks, ""), Some("lui $1, 4097".into()));
        assert_eq!(substitute("lui $1, LH2", &toks, ""), Some("lui $1, 4098".into()));
        assert_eq!(substitute("lw RG1, LL2($1)", &toks, ""), Some("lw $t0, -32768($1)".into()));
        assert_eq!(
            substitute("bne RG3, $0, BROFF12", &["div", "$t0", "$t1", "$t2"], ""),
            Some("bne $t2, $0, 1".into())
        );
        assert_eq!(substitute("DBNOP", &["div"], ""), None);
        assert_eq!(
            substitute("bne $1, $0, LAB", &["blt", "$t0", "$t1", "4194324"], "loop"),
            Some("bne $1, $0, loop".into())
        );
        assert_eq!(substitute("srl $1, RG2, S32", &["rol", "$t0", "$t1", "8"], ""), Some("srl $1, $t1, 24".into()));
    }
}
