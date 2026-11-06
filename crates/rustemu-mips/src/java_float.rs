//! Java floating-point input syntax and correctly rounded hexadecimal literals for MARS services.

/// Trims as Java's String.trim and removes a numeric type suffix, preserving special values.
fn number(text: &[u8]) -> Option<&str> {
    let t = std::str::from_utf8(text).ok()?.trim_matches(|c: char| c <= ' ');
    let unsigned = t.strip_prefix(['+', '-']).unwrap_or(t);
    if unsigned == "NaN" {
        return Some("NaN");
    }
    if unsigned == "Infinity" {
        return Some(t);
    }
    let t = t.strip_suffix(['f', 'F', 'd', 'D']).unwrap_or(t);
    let unsigned = t.strip_prefix(['+', '-']).unwrap_or(t);
    if unsigned.starts_with("0x") || unsigned.starts_with("0X") {
        return Some(t);
    }
    // Rust accepts other spellings of infinity/NaN, which Java rejects.
    (!t.is_empty()
        && t.bytes().all(|c| c.is_ascii_digit() || matches!(c, b'.' | b'e' | b'E' | b'+' | b'-'))
        && t.bytes().any(|c| c.is_ascii_digit()))
    .then_some(t)
}

/// Parses a hexadecimal significand and binary exponent directly into IEEE bits. Keeping a
/// guard bit and a sticky bit avoids double rounding (particularly for float and subnormals).
fn hex_bits(text: &str, fraction: u32, bias: i64) -> Option<u64> {
    let negative = text.starts_with('-');
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    let hex = unsigned.strip_prefix("0x").or_else(|| unsigned.strip_prefix("0X"))?;
    let (mantissa, exponent) = hex.split_once(['p', 'P'])?;
    let exp_digits = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
    if exp_digits.is_empty() || !exp_digits.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut exp = exp_digits.bytes().fold(0_i64, |n, c| n.saturating_mul(10).saturating_add((c - b'0') as i64));
    if exponent.starts_with('-') {
        exp = -exp;
    }
    let (mut dot, mut digits, mut fractional) = (false, false, 0_i64);
    let (mut length, mut kept, mut top, mut sticky) = (0_i64, 0_u32, 0_u64, false);
    for c in mantissa.bytes() {
        if c == b'.' && !dot {
            dot = true;
            continue;
        }
        let nibble = (c as char).to_digit(16)?;
        digits = true;
        fractional += i64::from(dot);
        for bit in (0..4).rev().map(|i| (nibble >> i) & 1) {
            if length == 0 && bit == 0 {
                continue;
            }
            length += 1;
            if kept < fraction + 2 {
                top = (top << 1) | bit as u64;
                kept += 1;
            } else {
                sticky |= bit != 0;
            }
        }
    }
    if !digits {
        return None;
    }
    let sign = u64::from(negative) << if fraction == 23 { 31 } else { 63 };
    let infinity = ((bias * 2 + 1) as u64) << fraction;
    let e = exp.saturating_add(length - 1).saturating_sub(fractional * 4);
    let min_normal = 1 - bias;
    let min_subnormal = min_normal - fraction as i64;
    if length == 0 || e < min_subnormal - 1 {
        return Some(sign);
    }
    if e > bias {
        return Some(sign | infinity);
    }
    let precision = (fraction as i64 + 1).min(e - min_subnormal + 1) as u32;
    let rounded = if kept <= precision {
        top << (precision - kept)
    } else {
        let shift = kept - precision;
        let guard = 1_u64 << (shift - 1);
        let truncated = top >> shift;
        let up = top & guard != 0 && (sticky || top & (guard - 1) != 0 || truncated & 1 != 0);
        truncated + u64::from(up)
    };
    // Addition carries into the exponent on rounding overflow, including into infinity.
    let bits = if e < min_normal { rounded } else { (((e + bias) as u64) << fraction) + rounded - (1_u64 << fraction) };
    Some(sign | bits)
}

/// Java Float.parseFloat, including optional suffixes and hexadecimal binary exponents.
pub(crate) fn parse_float(text: &[u8]) -> Option<f32> {
    let t = number(text)?;
    if t.contains(['x', 'X']) { hex_bits(t, 23, 127).map(|b| f32::from_bits(b as u32)) } else { t.parse().ok() }
}

/// Java Double.parseDouble; an f/F suffix does not reduce the precision to float.
pub(crate) fn parse_double(text: &[u8]) -> Option<f64> {
    let t = number(text)?;
    if t.contains(['x', 'X']) { hex_bits(t, 52, 1023).map(f64::from_bits) } else { t.parse().ok() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_matches_java_bits() {
        // Golden results from Java, including long significands, ties, subnormals, huge
        // exponents and malformed input. No Java installation is needed to run this test.
        for line in
            include_str!("../tests/data/java_float.tsv").lines().filter(|l| !l.is_empty() && !l.starts_with('#'))
        {
            let mut fields = line.split('\t');
            let text = fields.next().unwrap();
            let float = fields.next().unwrap();
            let double = fields.next().unwrap();
            assert_eq!(
                parse_float(text.as_bytes()).map(|v| format!("{:08x}", v.to_bits())),
                (float != "invalid").then(|| float.to_string()),
                "float {text:?}"
            );
            assert_eq!(
                parse_double(text.as_bytes()).map(|v| format!("{:016x}", v.to_bits())),
                (double != "invalid").then(|| double.to_string()),
                "double {text:?}"
            );
        }
        for text in [b"".as_slice(), b"\xff", b"\t \n"] {
            assert!(parse_float(text).is_none());
            assert!(parse_double(text).is_none());
        }
        assert_eq!(parse_float(b"\x001.5\x1f"), Some(1.5));
        assert_eq!(parse_double(b"\x000x1.8p0\x1f"), Some(1.5));
    }
}
