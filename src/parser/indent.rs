// Copyright 2024 Coffee Language Contributors
// Licensed under the MIT License

//! Unified indent management with caching

use std::collections::HashMap;

/// Cached indent information for a line
#[derive(Debug, Clone, Copy)]
pub struct IndentInfo {
    #[allow(dead_code)] // stored in the cache; tests read these fields
    pub line_number: usize,
    pub indent_level: usize,
    #[allow(dead_code)] // stored in the cache; tests read this field
    pub uses_tabs: bool,
}

impl IndentInfo {
    /// Create new indent info
    pub fn new(line_number: usize, indent_level: usize, uses_tabs: bool) -> Self {
        IndentInfo {
            line_number,
            indent_level,
            uses_tabs,
        }
    }
}

/// Unified indent cache and manager
#[derive(Debug, Clone)]
pub struct IndentCache {
    /// Cache of line indent information
    cache: HashMap<usize, IndentInfo>,
    /// Detected indent style (some = spaces, none = tabs, None = unknown)
    indent_style: Option<bool>, // true = spaces, false = tabs
    /// Detected indent size (2, 4, or 8)
    indent_size: Option<usize>,
    /// Inconsistent indentation detected
    has_inconsistent: bool,
}

impl IndentCache {
    /// Create a new indent cache
    pub fn new() -> Self {
        IndentCache {
            cache: HashMap::new(),
            indent_style: None,
            indent_size: None,
            has_inconsistent: false,
        }
    }

    /// Get or compute indent info for a line
    pub fn get_line_indent(&mut self, line_number: usize, line: &str) -> IndentInfo {
        // Check cache first
        if let Some(&info) = self.cache.get(&line_number) {
            return info;
        }

        // Compute indent info
        let trimmed = line.trim_start();
        let indent_level = line.len() - trimmed.len();
        let uses_tabs = line.starts_with('\t');

        let info = IndentInfo::new(line_number, indent_level, uses_tabs);

        // Detect indent style on first non-empty line
        if indent_level > 0 && self.indent_style.is_none() {
            self.indent_style = Some(!uses_tabs);
        }

        // Detect indent size
        if !uses_tabs && indent_level > 0 {
            // Try to detect if it's 2, 4, or 8 spaces
            if indent_level % 8 == 0 {
                self.indent_size = Some(8);
            } else if indent_level % 4 == 0 {
                self.indent_size = Some(4);
            } else if indent_level % 2 == 0 {
                self.indent_size = Some(2);
            }
        }

        // Check for inconsistency
        if let Some(style) = self.indent_style {
            let expected_tabs = !style;
            if uses_tabs != expected_tabs && indent_level > 0 {
                self.has_inconsistent = true;
            }
        }

        // Cache and return
        self.cache.insert(line_number, info);
        info
    }

    #[cfg(test)]
    pub fn get_cached(&self, line_number: usize) -> Option<IndentInfo> {
        self.cache.get(&line_number).copied()
    }

    #[cfg(test)]
    pub fn has_inconsistent_indent(&self) -> bool {
        self.has_inconsistent
    }

    #[cfg(test)]
    pub fn indent_style(&self) -> Option<bool> {
        self.indent_style
    }

    /// Exclusive end index of the indented block starting at `start`.
    ///
    /// Sequential indent scan only: empty lines are included; the first
    /// non-empty line at indent `<=` the starter line ends the span.
    pub fn indented_block_end(&mut self, lines: &[&str], start: usize) -> usize {
        if start >= lines.len() {
            return start;
        }
        let base = self.get_line_indent(start + 1, lines[start]).indent_level;
        let mut j = start + 1;
        while j < lines.len() {
            if lines[j].trim().is_empty() {
                j += 1;
                continue;
            }
            let indent = self.get_line_indent(j + 1, lines[j]).indent_level;
            if indent > base {
                j += 1;
                continue;
            }
            break;
        }
        j
    }
}

impl Default for IndentCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Byte length of the char literal (`'a'`, `'\n'`, `'\''`) at the start of `b`, or 0.
fn char_literal_len(b: &[u8]) -> usize {
    match (b.get(1), b.get(2), b.get(3)) {
        (Some(b'\\'), Some(_), Some(b'\'')) => 4,
        (Some(c), Some(b'\''), _) if *c != b'\\' && *c != b'\'' => 3,
        _ => 0,
    }
}

/// Code part of `line` (strings kept, `/#/` comment dropped) and the bracket depth change.
/// `in_block_comment` carries `/#* … *#/` state across lines.
pub(crate) fn scan_code(line: &str, in_block_comment: &mut bool) -> (usize, i32) {
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut in_str = false;
    let mut depth = 0i32;
    while i < bytes.len() {
        if *in_block_comment {
            if bytes[i..].starts_with(b"*#/") {
                *in_block_comment = false;
                i += 3;
            } else {
                i += 1;
            }
            continue;
        }
        let c = bytes[i];
        if in_str {
            if c == b'\\' {
                i += 1;
            } else if c == b'"' {
                in_str = false;
            }
        } else if bytes[i..].starts_with(b"/#*") {
            *in_block_comment = true;
            i += 3;
            continue;
        } else if bytes[i..].starts_with(b"/#/") {
            return (i, depth);
        } else if c == b'\'' && char_literal_len(&bytes[i..]) > 0 {
            // `'"'` and `'('` are chars, not a string start or a bracket
            i += char_literal_len(&bytes[i..]);
            continue;
        } else {
            match c {
                b'"' => in_str = true,
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                _ => {}
            }
        }
        i += 1;
    }
    (bytes.len(), depth)
}

/// True when `code` ends in an operator that cannot end a statement.
fn ends_with_continuation_operator(code: &str) -> bool {
    let t = code.trim_end();
    if t.ends_with("=>") {
        return false;
    }
    // `int(4)+` / `int(2)-` are types, not a dangling operator.
    if let Some(head) = t.strip_suffix('+').or_else(|| t.strip_suffix('-')) {
        if let Some(inner) = head.strip_suffix(')') {
            if let Some(open) = inner.rfind('(') {
                let digits = &inner[open + 1..];
                if !digits.is_empty()
                    && digits.chars().all(|c| c.is_ascii_digit())
                    && inner[..open].ends_with("int")
                {
                    return false;
                }
            }
        }
    }
    ["&&", "||", "==", "!=", "<=", ">=", "<<", ">>", "+", "-", "*", "/", "%", "&", "|", "^", "=", ",", "\\"]
        .iter()
        .any(|op| t.ends_with(op))
}

/// Join logical lines that span several physical ones: an unclosed `(` `[` `{`, or a trailing
/// binary operator. Newlines become spaces and the comment text on joined lines is blanked, so
/// every byte offset stays valid; only line numbers change, so the returned map gives the
/// original 0-based line of each joined line. `None` when nothing needs joining.
pub fn join_continuation_lines(source: &str) -> Option<(String, Vec<usize>)> {
    let lines: Vec<&str> = source.split('\n').collect();
    let mut out = String::with_capacity(source.len());
    let mut map: Vec<usize> = Vec::new();
    let mut joined_any = false;
    let mut in_block_comment = false;
    let mut i = 0;
    while i < lines.len() {
        let start_in_comment = in_block_comment;
        let (code_end, depth0) = scan_code(lines[i], &mut in_block_comment);
        let first_code = &lines[i][..code_end];
        let wants_more = !start_in_comment
            && !in_block_comment
            && (depth0 > 0 || (!first_code.trim().is_empty() && ends_with_continuation_operator(first_code)));
        if !wants_more || i + 1 >= lines.len() {
            map.push(i);
            out.push_str(lines[i]);
            if i + 1 < lines.len() {
                out.push('\n');
            }
            i += 1;
            continue;
        }
        // Gather the continuation; give up (keep the lines apart) if it never closes.
        let mut depth = depth0;
        let mut group = vec![i];
        let mut j = i;
        let mut bc = in_block_comment;
        let mut last_code = first_code;
        let mut closed = false;
        while j + 1 < lines.len() {
            j += 1;
            group.push(j);
            let (end, d) = scan_code(lines[j], &mut bc);
            depth += d;
            last_code = &lines[j][..end];
            if depth <= 0 && !(bc || (!last_code.trim().is_empty() && ends_with_continuation_operator(last_code))) {
                closed = true;
                break;
            }
            if lines[j].trim().is_empty() && depth <= 0 {
                break;
            }
        }
        let _ = last_code;
        if !closed {
            map.push(i);
            out.push_str(lines[i]);
            out.push('\n');
            i += 1;
            continue;
        }
        joined_any = true;
        map.push(i);
        let mut bc2 = in_block_comment;
        for (k, &idx) in group.iter().enumerate() {
            let line = lines[idx];
            let (end, _) = scan_code(line, &mut bc2);
            let last = k + 1 == group.len();
            if last {
                out.push_str(line);
            } else {
                out.push_str(&line[..end]);
                out.extend(std::iter::repeat(' ').take(line.len() - end));
                out.push(' ');
            }
        }
        in_block_comment = bc;
        if j + 1 < lines.len() {
            out.push('\n');
        }
        i = j + 1;
    }
    joined_any.then_some((out, map))
}

/// First line whose indentation falls between two open block levels (a dedent that matches no
/// enclosing block). Without this the parser silently drops or reparents the line.
/// Returns the 1-based line number. Lines inside brackets and comments are continuations.
pub fn first_inconsistent_dedent(source: &str) -> Option<usize> {
    let mut levels: Vec<usize> = vec![0];
    let mut depth: i32 = 0;
    let mut in_block_comment = false;
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let starts_inside = depth > 0 || in_block_comment;
        let blank = trimmed.is_empty() || (trimmed.starts_with("/#/") && !in_block_comment);
        if !starts_inside && !blank && !trimmed.starts_with("/#*") {
            let indent = line.len() - trimmed.len();
            let top = *levels.last().unwrap_or(&0);
            if indent > top {
                levels.push(indent);
            } else if indent < top {
                while levels.last().is_some_and(|&l| l > indent) {
                    levels.pop();
                }
                if levels.last() != Some(&indent) {
                    return Some(idx + 1);
                }
            }
        }
        let bytes = line.as_bytes();
        let mut i = 0;
        let mut in_str = false;
        while i < bytes.len() {
            if in_block_comment {
                if bytes[i..].starts_with(b"*#/") {
                    in_block_comment = false;
                    i += 3;
                } else {
                    i += 1;
                }
                continue;
            }
            let c = bytes[i];
            if in_str {
                if c == b'\\' {
                    i += 1;
                } else if c == b'"' {
                    in_str = false;
                }
            } else if bytes[i..].starts_with(b"/#*") {
                in_block_comment = true;
                i += 3;
                continue;
            } else if bytes[i..].starts_with(b"/#/") {
                break;
            } else {
                match c {
                    b'"' => in_str = true,
                    b'(' | b'[' | b'{' => depth += 1,
                    b')' | b']' | b'}' => depth = (depth - 1).max(0),
                    _ => {}
                }
            }
            i += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_indent_computation() {
        let mut cache = IndentCache::new();

        // Test space indentation
        let info1 = cache.get_line_indent(1, "    line");
        assert_eq!(info1.indent_level, 4);
        assert_eq!(info1.uses_tabs, false);

        // Test tab indentation
        let info2 = cache.get_line_indent(2, "\t\tline");
        assert_eq!(info2.indent_level, 2);
        assert_eq!(info2.uses_tabs, true);
    }

    #[test]
    fn test_cache_hit() {
        let mut cache = IndentCache::new();

        cache.get_line_indent(1, "  line");
        let info = cache.get_cached(1).unwrap();
        assert_eq!(info.indent_level, 2);
    }

    #[test]
    fn test_indent_style_detection() {
        let mut cache = IndentCache::new();

        cache.get_line_indent(1, "  line");
        assert_eq!(cache.indent_style(), Some(true)); // spaces

        let mut cache2 = IndentCache::new();
        cache2.get_line_indent(1, "\tline");
        assert_eq!(cache2.indent_style(), Some(false)); // tabs
    }

    #[test]
    fn test_inconsistent_detection() {
        let mut cache = IndentCache::new();

        cache.get_line_indent(1, "  line");
        cache.get_line_indent(2, "\tline");

        assert!(cache.has_inconsistent_indent());
    }

    #[test]
    fn indented_block_end_does_not_cross_sibling() {
        let lines = [
            "fn a() => int:",
            "    return 1",
            "",
            "fn b() => int:",
            "    return 2",
        ];
        let mut cache = IndentCache::new();
        assert_eq!(cache.indented_block_end(&lines, 0), 3);
        assert_eq!(cache.indented_block_end(&lines, 3), 5);
    }
}

/// `else if cond:` at the start of a line is `elif cond:`; same byte length, so offsets hold.
pub(crate) fn rewrite_else_if(input: &str) -> Option<String> {
    let mut changed = false;
    let mut out = String::with_capacity(input.len());
    for line in input.split_inclusive('\n') {
        let body = line.trim_start_matches([' ', '\t']);
        let indent = line.len() - body.len();
        let is_else_if = body.starts_with("else if")
            && matches!(body.as_bytes().get(7), Some(b' ') | Some(b'('));
        if is_else_if {
            changed = true;
            out.push_str(&line[..indent]);
            out.push_str("elif   ");
            out.push_str(&body[7..]);
        } else {
            out.push_str(line);
        }
    }
    changed.then_some(out)
}
