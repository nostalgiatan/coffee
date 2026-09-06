// Copyright 2024 Coffee Language Contributors
// Licensed under the MIT License

//! Block nesting tracker using stack-based approach
//!
//! Provides robust tracking of nested code blocks with conflict detection.

use std::fmt;

/// Block type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockType {
    /// Function definition
    Function,
    /// Class definition
    Class,
    /// Enum definition
    Enum,
    /// If statement (includes elif/else)
    IfStatement,
    /// While loop
    WhileLoop,
    /// For loop
    ForLoop,
    /// Match expression
    MatchExpr,
    /// Multiline expression (e.g., struct literal)
    Expression,
}

impl BlockType {
    /// Check if this block can contain nested blocks of the same type
    pub fn allows_self_nesting(&self) -> bool {
        matches!(self, BlockType::IfStatement | BlockType::WhileLoop | BlockType::ForLoop)
    }

    /// Independent top-level units (`fn` / `class` / `enum`) that do not nest in each other.
    pub fn is_independent_top_level(self) -> bool {
        matches!(self, BlockType::Function | BlockType::Class | BlockType::Enum)
    }

    /// Get the keyword that starts this block
    pub fn keyword(&self) -> &str {
        match self {
            BlockType::Function => "fn",
            BlockType::Class => "class",
            BlockType::Enum => "enum",
            BlockType::IfStatement => "if",
            BlockType::WhileLoop => "while",
            BlockType::ForLoop => "for",
            BlockType::MatchExpr => "match",
            BlockType::Expression => "expr",
        }
    }
}

impl fmt::Display for BlockType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.keyword())
    }
}

/// A single block in the nesting stack
#[derive(Debug, Clone)]
pub struct Block {
    /// Block type
    pub block_type: BlockType,
    /// Indentation level (number of leading whitespace characters)
    pub indent_level: usize,
    /// Line number where this block starts
    pub start_line: usize,
    /// Whether this block expects a body (ends with ':')
    pub expects_body: bool,
}

impl Block {
    /// Create a new block
    pub fn new(block_type: BlockType, indent_level: usize, start_line: usize, expects_body: bool) -> Self {
        Block {
            block_type,
            indent_level,
            start_line,
            expects_body,
        }
    }
}

/// Conflict types detected by the tracker
#[derive(Debug, Clone, PartialEq)]
pub enum Conflict {
    /// Unclosed block at end of input
    UnclosedBlock {
        block_type: BlockType,
        start_line: usize,
        indent_level: usize,
    },
    /// Inconsistent indentation (e.g., mixed spaces and tabs)
    InconsistentIndentation {
        line: usize,
        expected_indent: usize,
        found_indent: usize,
    },
    /// Block closed at wrong indent level
    WrongIndentLevel {
        block_type: BlockType,
        start_line: usize,
        expected_level: usize,
        found_level: usize,
    },
    /// Nested block violates nesting rules
    InvalidNesting {
        outer: BlockType,
        inner: BlockType,
        line: usize,
    },
}

impl fmt::Display for Conflict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Conflict::UnclosedBlock { block_type, start_line, .. } => {
                write!(f, "unclosed '{}' block starting at line {}", block_type, start_line)
            }
            Conflict::InconsistentIndentation { line, expected_indent, found_indent } => {
                write!(f, "inconsistent indentation at line {}: expected {}, found {}",
                    line, expected_indent, found_indent)
            }
            Conflict::WrongIndentLevel { block_type, start_line, expected_level, found_level } => {
                write!(f, "'{}' block starting at line {} closed at wrong indent: expected {}, found {}",
                    block_type, start_line, expected_level, found_level)
            }
            Conflict::InvalidNesting { outer, inner, line } => {
                write!(f, "'{}' cannot be nested inside '{}' at line {}", inner, outer, line)
            }
        }
    }
}

/// Stack-based block nesting tracker
#[derive(Debug, Clone)]
pub struct BlockTracker {
    /// Stack of nested blocks
    stack: Vec<Block>,
    /// Detected conflicts
    conflicts: Vec<Conflict>,
}

impl BlockTracker {
    /// Create a new tracker
    pub fn new() -> Self {
        BlockTracker {
            stack: Vec::new(),
            conflicts: Vec::new(),
        }
    }

    /// Check if the tracker is empty (no open blocks)
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Get the number of open blocks
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Get the current (innermost) block if any
    pub fn current_block(&self) -> Option<&Block> {
        self.stack.last()
    }

    /// Get the base (outermost) block if any
    pub fn base_block(&self) -> Option<&Block> {
        self.stack.first()
    }

    /// Check if we're inside a specific block type
    pub fn is_inside(&self, block_type: BlockType) -> bool {
        self.stack.iter().any(|b| b.block_type == block_type)
    }

    /// Get the nesting depth of if statements
    #[cfg(test)]
    pub fn if_nesting_depth(&self) -> usize {
        self.stack.iter()
            .filter(|b| b.block_type == BlockType::IfStatement)
            .count()
    }

    /// Push a new block onto the stack
    pub fn push_block(&mut self, block_type: BlockType, indent_level: usize, start_line: usize, expects_body: bool) {
        let block = Block::new(block_type, indent_level, start_line, expects_body);

        // Check for invalid nesting
        if let Some(current) = self.current_block().cloned() {
            // Check if inner block is properly nested (greater indent)
            if indent_level <= current.indent_level {
                self.conflicts.push(Conflict::WrongIndentLevel {
                    block_type,
                    start_line,
                    expected_level: current.indent_level + 1, // At least one level deeper
                    found_level: indent_level,
                });
            }

            // Check for nesting rule violations
            if !current.block_type.allows_self_nesting() && current.block_type == block_type {
                self.conflicts.push(Conflict::InvalidNesting {
                    outer: current.block_type,
                    inner: block_type,
                    line: start_line,
                });
            }
        }

        self.stack.push(block);
    }

    /// Pop blocks from the stack until we reach the given indent level
    pub fn pop_to_indent(&mut self, indent_level: usize, _line_num: usize) -> Vec<Block> {
        let mut popped = Vec::new();

        // Pop blocks that are at or above the target indent
        while let Some(top) = self.current_block() {
            if top.indent_level >= indent_level {
                popped.push(self.stack.pop().unwrap());
            } else {
                break;
            }
        }

        popped
    }

    /// Validate that all blocks are properly closed
    pub fn validate_closure(&mut self) -> Result<(), Vec<Conflict>> {
        let unclosed: Vec<Conflict> = self.stack.iter()
            .filter(|b| b.expects_body)
            .map(|b| Conflict::UnclosedBlock {
                block_type: b.block_type.clone(),
                start_line: b.start_line,
                indent_level: b.indent_level,
            })
            .collect();

        if !unclosed.is_empty() {
            self.conflicts.extend(unclosed.clone());
            Err(unclosed)
        } else {
            Ok(())
        }
    }

    /// Detect if a line starts a new block
    pub fn detect_block_type(line: &str) -> Option<BlockType> {
        let trimmed = line.trim();

        // Check for block-starting keywords
        // Handle both "fn" and "c fn"
        if trimmed.starts_with("c fn") || trimmed.starts_with("fn ") || trimmed == "fn" {
            Some(BlockType::Function)
        } else if trimmed.starts_with("packed class ") || trimmed.starts_with("class ") || trimmed == "class" {
            Some(BlockType::Class)
        } else if trimmed.starts_with("enum ") || trimmed == "enum" {
            Some(BlockType::Enum)
        } else if trimmed.starts_with("if ") {
            Some(BlockType::IfStatement)
        } else if trimmed.starts_with("elif ") || trimmed.starts_with("else:") {
            // elif/else are part of the if statement
            Some(BlockType::IfStatement)
        } else if trimmed.starts_with("while ") {
            Some(BlockType::WhileLoop)
        } else if trimmed.starts_with("for ") {
            Some(BlockType::ForLoop)
        } else if trimmed.starts_with("match ") {
            Some(BlockType::MatchExpr)
        } else {
            None
        }
    }

    /// Check if a line continues the current if/elif/else block
    pub fn is_if_continuation(&self, trimmed: &str) -> bool {
        if !self.is_inside(BlockType::IfStatement) {
            return false;
        }

        trimmed.starts_with("elif ") || trimmed.starts_with("else:")
    }

    /// Process a line and update the block stack
    pub fn process_line(&mut self, line: &str, line_num: usize) {
        let indent_level = line.len() - line.trim_start().len();
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with("/#/") {
            return;
        }

        // Check if this line starts a new block
        if let Some(block_type) = Self::detect_block_type(line) {
            let expects_body = trimmed.ends_with(':');

            // Check if this is an elif/else continuation
            if self.is_if_continuation(trimmed) {
                // Don't push a new block, just validate indent
                if let Some(_base) = self.base_block() {
                    if let Some(current) = self.current_block() {
                        // elif/else should be at same level as the if
                        if indent_level != current.indent_level {
                            self.conflicts.push(Conflict::InconsistentIndentation {
                                line: line_num,
                                expected_indent: current.indent_level,
                                found_indent: indent_level,
                            });
                        }
                    }
                }
            } else {
                // Regular block start
                self.push_block(block_type, indent_level, line_num, expects_body);
            }
        } else {
            // Not a block start - check if we need to pop blocks
            // Pop blocks that are at or above this indent
            self.pop_to_indent(indent_level, line_num);
        }
    }
}

impl Default for BlockTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for BlockTracker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "BlockTracker (depth: {})", self.depth())?;
        for (i, block) in self.stack.iter().enumerate() {
            writeln!(f, "  [{}]: {} at indent {}, line {}",
                i, block.block_type, block.indent_level, block.start_line)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_tracker() {
        let tracker = BlockTracker::new();
        assert!(tracker.is_empty());
        assert_eq!(tracker.depth(), 0);
    }

    #[test]
    fn test_simple_nesting() {
        let mut tracker = BlockTracker::new();
        tracker.push_block(BlockType::Function, 0, 1, true);
        assert_eq!(tracker.depth(), 1);
        assert!(tracker.is_inside(BlockType::Function));
    }

    #[test]
    fn test_if_nesting_depth() {
        let mut tracker = BlockTracker::new();
        tracker.push_block(BlockType::IfStatement, 0, 1, true);
        tracker.push_block(BlockType::IfStatement, 4, 5, true);
        assert_eq!(tracker.if_nesting_depth(), 2);
    }

    #[test]
    fn test_unclosed_block_detection() {
        let mut tracker = BlockTracker::new();
        tracker.push_block(BlockType::Function, 0, 1, true);
        let result = tracker.validate_closure();
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_block_type() {
        assert_eq!(BlockTracker::detect_block_type("fn test()"), Some(BlockType::Function));
        assert_eq!(BlockTracker::detect_block_type("if x > 0:"), Some(BlockType::IfStatement));
        assert_eq!(BlockTracker::detect_block_type("while true:"), Some(BlockType::WhileLoop));
        assert_eq!(BlockTracker::detect_block_type("let x = 5"), None);
        assert_eq!(BlockTracker::detect_block_type("packed class P:"), Some(BlockType::Class));
        assert_eq!(BlockTracker::detect_block_type("c fn foo():"), Some(BlockType::Function));
        assert!(BlockType::Function.is_independent_top_level());
        assert!(!BlockType::IfStatement.is_independent_top_level());
    }
}
