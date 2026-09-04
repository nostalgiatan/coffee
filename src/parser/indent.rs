// Copyright 2024 Coffee Language Contributors
// Licensed under the MIT License

//! Unified indent management with caching

use std::collections::HashMap;

/// Cached indent information for a line
#[derive(Debug, Clone, Copy)]
pub struct IndentInfo {
    /// The line number (1-based)
    pub line_number: usize,
    /// Leading whitespace count
    pub indent_level: usize,
    /// Whether line uses tabs (false = uses spaces)
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

    /// Check if this indent matches another
    pub fn matches(&self, other: &IndentInfo) -> bool {
        self.indent_level == other.indent_level
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

    /// Get cached indent info without computing
    pub fn get_cached(&self, line_number: usize) -> Option<IndentInfo> {
        self.cache.get(&line_number).copied()
    }

    /// Check if indentation has been inconsistent
    pub fn has_inconsistent_indent(&self) -> bool {
        self.has_inconsistent
    }

    /// Get detected indent style (true = spaces, false = tabs)
    pub fn indent_style(&self) -> Option<bool> {
        self.indent_style
    }

    /// Get detected indent size
    pub fn indent_size(&self) -> Option<usize> {
        self.indent_size
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Pre-compute indent info for all lines
    pub fn precompute_lines<'a>(&mut self, lines: impl Iterator<Item = (usize, &'a str)>) {
        for (line_num, line) in lines {
            if !line.trim().is_empty() {
                self.get_line_indent(line_num, line);
            }
        }
    }

    /// Batch get indent info for multiple lines
    pub fn get_line_indents<'a>(&mut self, lines: impl Iterator<Item = (usize, &'a str)>) -> Vec<IndentInfo> {
        lines.map(|(line_num, line)| self.get_line_indent(line_num, line))
            .collect()
    }

    /// Validate indent consistency across all cached lines
    pub fn validate_consistency(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.has_inconsistent {
            errors.push("Mixed tabs and spaces in indentation".to_string());
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
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
}
