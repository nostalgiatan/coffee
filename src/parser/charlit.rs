//! Character literals: `'a'`, `'\n'`, `'\x41'` are integer literals (the byte or code point).
//!
//! They are rewritten to decimal text before parsing, padded with spaces so every byte offset
//! in the source stays valid. Strings and `/#/` comments are left alone.

fn decode_escape(rest: &[u8]) -> Option<(u32, usize)> {
    // `rest` starts after the backslash; returns (value, bytes consumed).
    match *rest.first()? {
        b'n' => Some((10, 1)),
        b't' => Some((9, 1)),
        b'r' => Some((13, 1)),
        b'0' => Some((0, 1)),
        b'\\' => Some((92, 1)),
        b'\'' => Some((39, 1)),
        b'"' => Some((34, 1)),
        b'x' => {
            let hex = std::str::from_utf8(rest.get(1..3)?).ok()?;
            Some((u32::from_str_radix(hex, 16).ok()?, 3))
        }
        _ => None,
    }
}

/// Source with every character literal replaced by its decimal value, or `None` if there is none.
pub fn rewrite_char_literals(source: &str) -> Option<String> {
    if !source.contains('\'') {
        return None;
    }
    let bytes = source.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut changed = false;
    let mut in_str = false;
    let mut in_block_comment = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if in_block_comment {
            if bytes[i..].starts_with(b"*#/") {
                out.extend_from_slice(b"*#/");
                i += 3;
                in_block_comment = false;
            } else {
                out.push(c);
                i += 1;
            }
            continue;
        }
        if in_str {
            out.push(c);
            if c == b'\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1]);
                i += 2;
                continue;
            }
            if c == b'"' || c == b'\n' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if bytes[i..].starts_with(b"/#*") {
            out.extend_from_slice(b"/#*");
            i += 3;
            in_block_comment = true;
            continue;
        }
        if bytes[i..].starts_with(b"/#/") {
            while i < bytes.len() && bytes[i] != b'\n' {
                out.push(bytes[i]);
                i += 1;
            }
            continue;
        }
        if c == b'"' {
            in_str = true;
            out.push(c);
            i += 1;
            continue;
        }
        if c == b'\'' {
            let body = &bytes[i + 1..];
            let parsed = if body.first() == Some(&b'\\') {
                decode_escape(&body[1..]).map(|(v, n)| (v, 1 + n))
            } else {
                std::str::from_utf8(&body[..body.len().min(4)])
                    .ok()
                    .or_else(|| {
                        (1..=3).rev().find_map(|n| body.get(..n).and_then(|b| std::str::from_utf8(b).ok()))
                    })
                    .and_then(|s| s.chars().next())
                    .filter(|ch| *ch != '\'' && *ch != '\n')
                    .map(|ch| (ch as u32, ch.len_utf8()))
            };
            if let Some((value, used)) = parsed {
                if body.get(used) == Some(&b'\'') {
                    let total = used + 2;
                    let digits = value.to_string();
                    if digits.len() <= total {
                        out.extend_from_slice(digits.as_bytes());
                        out.extend(std::iter::repeat(b' ').take(total - digits.len()));
                        i += total;
                        changed = true;
                        continue;
                    }
                }
            }
        }
        out.push(c);
        i += 1;
    }
    if !changed {
        return None;
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::rewrite_char_literals;

    #[test]
    fn rewrites_plain_escape_and_hex_literals_keeping_length() {
        let src = "let a: int = 'a'\nlet b: int = '\\n'\nlet c: int = '\\x41'\n";
        let out = rewrite_char_literals(src).unwrap();
        assert_eq!(out.len(), src.len());
        assert!(out.contains("97 "));
        assert!(out.contains("10  "));
        assert!(out.contains("65"));
    }

    #[test]
    fn leaves_strings_and_comments_alone() {
        assert!(rewrite_char_literals("let s: str = \"it's 'a'\"\n").is_none());
        assert!(rewrite_char_literals("/#/ 'a'\nlet x: int = 1\n").is_none());
    }

    #[test]
    fn operator_characters_become_numbers() {
        let out = rewrite_char_literals("let p: int = '+'\n").unwrap();
        assert!(!out.contains('+'));
    }
}
