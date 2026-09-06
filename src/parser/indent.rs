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
