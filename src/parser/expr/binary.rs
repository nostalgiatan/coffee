//! Binary-operator scanning helpers for the expression parser.

/// SYNTAX.md, low → high. Smaller number is lower precedence (split first).
fn op_prec(op: &str) -> Option<u8> {
    Some(match op {
        "||" => 0,
        "&&" => 1,
        "|" => 2,
        "^" => 3,
        "&" => 4,
        "==" | "!=" => 5,
        "<" | ">" | "<=" | ">=" => 6,
        "<<" | ">>" => 7,
        "+" | "-" => 8,
        "*" | "/" | "%" => 9,
        _ => return None,
    })
}

/// Longest-first so `&&` / `<<` / `<=` win over the one-character prefixes.
const OPS_LONGEST_FIRST: &[&str] = &[
    "||", "&&", "==", "!=", "<=", ">=", "<<", ">>", "|", "^", "&", "<", ">", "+", "-", "*", "/",
    "%",
];

fn op_at(bytes: &[u8], i: usize) -> Option<&'static str> {
    for op in OPS_LONGEST_FIRST {
        let ob = op.as_bytes();
        if i + ob.len() <= bytes.len() && &bytes[i..i + ob.len()] == ob {
            return Some(*op);
        }
    }
    None
}

fn can_be_unary(op: &str) -> bool {
    matches!(op, "+" | "-" | "&" | "*" | "~" | "!")
}

/// True when `op` at `pos` sits after an operand (`a & b`), not as a prefix (`a + &b`).
fn binary_context(expr: &str, pos: usize) -> bool {
    let before = expr[..pos].trim_end();
    let Some(c) = before.chars().last() else {
        return false;
    };
    c.is_ascii_alphanumeric() || matches!(c, '_' | ')' | ']' | '"' | '\'')
}

fn walk_bin_ops(expr: &str, mut visit: impl FnMut(usize, &'static str) -> bool) {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape_next = false;
    let expr_bytes = expr.as_bytes();
    let mut i = 0;
    while i < expr_bytes.len() {
        let ch = expr_bytes[i] as char;
        if escape_next {
            escape_next = false;
            i += 1;
        } else if ch == '\\' {
            escape_next = true;
            i += 1;
        } else if ch == '"' && !in_string {
            in_string = true;
            i += 1;
        } else if ch == '"' && in_string {
            in_string = false;
            i += 1;
        } else if !in_string && (ch == '(' || ch == '[' || ch == '{') {
            depth += 1;
            i += 1;
        } else if !in_string && (ch == ')' || ch == ']' || ch == '}') {
            depth -= 1;
            i += 1;
        } else if !in_string && depth == 0 {
            if let Some(op) = op_at(expr_bytes, i) {
                if !can_be_unary(op) || binary_context(expr, i) {
                    if visit(i, op) {
                        return;
                    }
                }
                i += op.len();
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
}

/// Rightmost operator of the lowest precedence still present at depth 0.
pub(super) fn find_lowest_prec_split(expr: &str) -> Option<(usize, &'static str)> {
    let mut best: Option<(usize, &'static str, u8)> = None;
    walk_bin_ops(expr, |i, op| {
        let Some(prec) = op_prec(op) else {
            return false;
        };
        match best {
            None => best = Some((i, op, prec)),
            Some((_, _, bp)) if prec <= bp => best = Some((i, op, prec)),
            _ => {}
        }
        false
    });
    best.map(|(i, op, _)| (i, op))
}
