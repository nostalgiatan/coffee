//! Compilation Statistics Module
//! 
//! This module handles collection and management of compilation statistics.
//! The statistics provide detailed information about the structure and content
//! of Coffee source code being compiled, including counts of various language
//! constructs such as functions, classes, control flow statements, and more.
//! 
//! The statistics are useful for:
//! - Analyzing code complexity and structure
//! - Measuring compilation performance
//! - Debugging and profiling the compiler
//! - Providing insights into code metrics
//! - Monitoring compilation progress

use crate::parser;

/// Compilation statistics
/// 
/// Tracks and stores various metrics about the Coffee source code being compiled.
/// This structure provides detailed information about the composition of the source
/// code, including counts of different language constructs, which can be useful
/// for analysis, debugging, and performance monitoring.
/// 
/// # Examples
/// 
/// ```rust
/// use crate::compiler::statistics::CompilationStatistics;
/// 
/// let mut stats = CompilationStatistics::new();
/// 
/// // Process statements and update stats
/// // stats.collect_from_statement(&some_statement);
/// 
/// println!("Functions declared: {}", stats.functions_declared);
/// println!("Total entities: {}", stats.total_entities_declared());
/// ```
#[derive(Debug, Clone, Default)]
pub struct CompilationStatistics {
    /// Number of lines parsed
    /// Total number of lines in the source code being compiled
    pub lines_parsed: usize,
    /// Number of statements parsed
    /// Total number of statements in the source code
    pub statements_parsed: usize,
    /// Number of imports processed
    /// Count of import statements processed during compilation
    pub imports_processed: usize,
    /// Number of modules loaded
    /// Count of modules loaded during compilation
    pub modules_loaded: usize,
    /// Number of functions declared
    /// Count of function declarations in the source code
    pub functions_declared: usize,
    /// Number of classes declared
    /// Count of class declarations in the source code
    pub classes_declared: usize,
    /// Number of enums declared
    /// Count of enum declarations in the source code
    pub enums_declared: usize,
    /// Number of variables declared
    /// Count of variable declarations in the source code
    pub variables_declared: usize,
    /// Number of return statements
    /// Count of return statements in the source code
    pub return_statements: usize,
    /// Number of if expressions
    /// Count of if expressions in the source code
    pub if_expressions: usize,
    /// Number of while loops
    /// Count of while loops in the source code
    pub while_loops: usize,
    /// Number of for loops
    /// Count of for loops in the source code
    pub for_loops: usize,
    /// Number of match expressions
    /// Count of match expressions in the source code
    pub match_expressions: usize,
    /// Number of break statements
    /// Count of break statements in the source code
    pub break_statements: usize,
    /// Number of continue statements
    /// Count of continue statements in the source code
    pub continue_statements: usize,
    /// Number of memory operations
    /// Count of memory operation statements in the source code
    pub memory_operations: usize,
    /// Number of raise statements
    /// Count of raise statements in the source code
    pub raise_statements: usize,
    /// Number of comments
    /// Count of comment statements in the source code
    pub comments_count: usize,
    /// Number of expression statements (standalone function calls, etc.)
    /// Count of expression statements in the source code
    pub expression_statements: usize,
}

impl CompilationStatistics {
    /// Create a new statistics object with default values
    /// 
    /// Initializes a new CompilationStatistics instance with all counters set to zero.
    /// This is equivalent to calling the Default::default() implementation.
    /// 
    /// # Returns
    /// 
    /// A new CompilationStatistics instance with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Collect statistics from a statement
    /// 
    /// Updates the statistics counters based on the type of statement provided.
    /// Different statement types increment different counters (e.g., a Function
    /// statement increments the functions_declared counter).
    /// 
    /// # Arguments
    /// 
    /// * `statement` - A reference to a parsed statement to collect statistics from
    pub fn collect_from_statement(&mut self, statement: &parser::Statement) {
        match statement {
            parser::Statement::Import(_) => self.imports_processed += 1,
            parser::Statement::Function(_) => self.functions_declared += 1,
            parser::Statement::Main(_) => {} // Main entry point - no specific stat needed
            parser::Statement::Class(_) => self.classes_declared += 1,
            parser::Statement::Enum(_) => self.enums_declared += 1,
            parser::Statement::VariableDecl(_) => self.variables_declared += 1,
            parser::Statement::Return(_) => self.return_statements += 1,
            parser::Statement::If(_) => self.if_expressions += 1,
            parser::Statement::While(_) => self.while_loops += 1,
            parser::Statement::For(_) => self.for_loops += 1,
            parser::Statement::Match(_) => self.match_expressions += 1,
            parser::Statement::Break(_) => self.break_statements += 1,
            parser::Statement::Continue(_) => self.continue_statements += 1,
            parser::Statement::MemoryOp(_) => self.memory_operations += 1,
            parser::Statement::Raise(_) => self.raise_statements += 1,
            parser::Statement::SingleLineComment(_) | parser::Statement::MultiLineComment(_) => {
                self.comments_count += 1
            }
            parser::Statement::Expr(_) => {
                self.expression_statements += 1
            }
        }
    }

    /// Update statement count based on parsed statements
    /// 
    /// Sets the statements_parsed counter to the length of the provided statements slice.
    /// This is typically called after parsing a source file to update the total count.
    /// 
    /// # Arguments
    /// 
    /// * `statements` - A slice of parsed statements to count
    pub fn update_statement_count(&mut self, statements: &[parser::Statement]) {
        self.statements_parsed = statements.len();
    }

    /// Update line count
    /// 
    /// Calculates and sets the lines_parsed counter based on the number of lines
    /// in the provided source string. This helps track the size of the source code.
    /// 
    /// # Arguments
    /// 
    /// * `source` - The source code string to count lines in
    pub fn update_line_count(&mut self, source: &str) {
        self.lines_parsed = source.lines().count();
    }

    /// Get total entities declared
    /// 
    /// Calculates the total number of declared entities in the source code,
    /// including functions, classes, enums, and variables.
    /// 
    /// # Returns
    /// 
    /// The sum of functions, classes, enums, and variables declared
    pub fn total_entities_declared(&self) -> usize {
        self.functions_declared + 
        self.classes_declared + 
        self.enums_declared + 
        self.variables_declared
    }

    /// Get total control flow statements
    /// 
    /// Calculates the total number of control flow statements in the source code,
    /// including if expressions, while loops, for loops, and match expressions.
    /// 
    /// # Returns
    /// 
    /// The sum of all control flow statements
    pub fn total_control_flow_statements(&self) -> usize {
        self.if_expressions + self.while_loops + self.for_loops + self.match_expressions
    }

    /// Get total flow control statements
    /// 
    /// Calculates the total number of flow control statements in the source code,
    /// including return, break, and continue statements.
    /// 
    /// # Returns
    /// 
    /// The sum of all flow control statements
    pub fn total_flow_control_statements(&self) -> usize {
        self.return_statements + self.break_statements + self.continue_statements
    }
}