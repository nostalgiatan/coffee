// Copyright 2024 Coffee Language Contributors
// Licensed under the MIT License
//
// Diagnostic System for Coffee Compiler
//
// Clean, professional error reporting with colors and clear formatting.

use crate::types;
use std::fmt;
use std::sync::{Arc, RwLock};

//=============================================================================
// Diagnostic Severity Levels
//=============================================================================

/// Severity level of a diagnostic message
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Error - prevents compilation
    Error,
    /// Warning - does not prevent compilation but should be addressed
    Warning,
    /// Hint - suggestion for improvement
    Hint,
}

impl Severity {
    /// Returns the ANSI color code for this severity
    pub fn color(self) -> &'static str {
        match self {
            Severity::Error => "\x1b[31m",    // Red
            Severity::Warning => "\x1b[33m",  // Yellow
            Severity::Hint => "\x1b[36m",    // Cyan
        }
    }

    /// Returns the bold ANSI color code for this severity
    pub fn color_bold(self) -> &'static str {
        match self {
            Severity::Error => "\x1b[1;31m",  // Bold Red
            Severity::Warning => "\x1b[1;33m", // Bold Yellow
            Severity::Hint => "\x1b[1;36m",   // Bold Cyan
        }
    }

    /// ANSI reset code
    pub fn reset() -> &'static str {
        "\x1b[0m"
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
            Severity::Hint => write!(f, "hint"),
        }
    }
}

//=============================================================================
// Source Location
//=============================================================================

/// Source code location information
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct SourceLocation {
    /// File path
    pub file: Option<String>,
    /// Start line (1-based)
    pub line_start: usize,
    /// End line (1-based)
    pub line_end: usize,
    /// Start column (1-based)
    pub column_start: usize,
    /// End column (1-based)
    pub column_end: usize,
    /// Byte offset in source
    pub byte_offset: usize,
    /// Byte length
    pub byte_length: usize,
}

impl SourceLocation {
    /// Create a new source location
    pub fn new(line: usize, column_start: usize, column_end: usize) -> Self {
        SourceLocation {
            file: None,
            line_start: line,
            line_end: line,
            column_start,
            column_end,
            byte_offset: 0,
            byte_length: column_end.saturating_sub(column_start),
        }
    }

    /// Create a new source location with file path
    pub fn with_file(file: impl Into<String>, line: usize, column_start: usize, column_end: usize) -> Self {
        let mut loc = Self::new(line, column_start, column_end);
        loc.file = Some(file.into());
        loc
    }

    /// Check if this location points to a valid position
    pub fn is_valid(&self) -> bool {
        self.line_start > 0 && self.column_start > 0
    }

    /// Format location as string: "file:line:column" or "line:column"
    pub fn format(&self) -> String {
        if let Some(ref file) = self.file {
            format!("{}:{}:{}", file, self.line_start, self.column_start)
        } else {
            format!("line {}, column {}", self.line_start, self.column_start)
        }
    }
}

//=============================================================================
// Error Classification
//=============================================================================

/// Error categories with unique error codes
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    // Parse Errors (E001-E099)
    InvalidSyntax { context: String },
    UnexpectedToken { token: String, expected: Vec<String> },
    IncompleteInput { expected: String },

    // Type Errors (E100-E199)
    TypeMismatch { expected: String, found: String },
    UnknownType { name: String },
    /// Wrong number of call/constructor arguments
    ArityMismatch { expected: usize, found: usize },
    /// Operator not defined for these operand types
    InvalidOperation { op: String, left: String, right: String },
    /// Value used as a function
    NotCallable { ty: String },
    /// Struct/class field does not exist
    FieldNotFound { type_name: String, field_name: String },
    /// Method does not exist on this type
    MethodNotFound { type_name: String, method_name: String },
    /// Enum variant does not exist
    VariantNotFound { enum_name: String, variant_name: String },
    /// Wrong number of generic arguments
    GenericArgCountMismatch { type_name: String, expected: usize, found: usize },
    /// Type string could not be resolved, or a type-level parse/instantiate failure
    InvalidType { name: String, reason: String },
    /// `break` / `continue` outside a loop
    InvalidControlFlow { keyword: String },
    /// Match does not cover all enum variants
    NonExhaustiveMatch { ty: String },

    // Symbol Errors (E200-E299)
    UndefinedSymbol { name: String, symbol_type: SymbolType },
    DuplicateDefinition { name: String, previous: SourceLocation },
    InvalidSymbolAccess { name: String, reason: String },

    // Memory Errors (E300-E399)
    UseAfterMove { name: String },
    UseAfterDrop { name: String },
    InvalidMemoryOperation { operation: String, reason: String },
    BorrowViolation { variable: String, reason: String },

    // Lifetime Errors (E400-E499)
    LifetimeError { variable: String, reason: String },
    BorrowConflict { variable: String },

    // Import Errors (E500-E599)
    ModuleNotFound { module: String },
    CircularImport { path: Vec<String> },
    InvalidImport { import: String, reason: String },
    CfcNotFound { library: String },
    CHeaderNotFound { header: String, searched: String },
    CLibraryNotInstalled { name: String, searched: String },
    CDepCycle { path: Vec<String> },

    // Constraint Errors (E600-E699)
    ConstraintViolation { constraint: String, reason: String },

    // Code Generation Errors (E700-E799)
    CodeGeneration { stage: String, details: String },

    // Verification Errors (E800-E899)
    Verification { phase: String, details: String },

    // Link Errors (E900-E999)
    LinkError { details: String },
}

/// Symbol type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolType {
    Variable,
    Function,
    Module,
}

impl fmt::Display for SymbolType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SymbolType::Variable => write!(f, "variable"),
            SymbolType::Function => write!(f, "function"),
            SymbolType::Module => write!(f, "module"),
        }
    }
}

impl ErrorKind {
    /// Get the error code for this error kind
    pub fn error_code(&self) -> &'static str {
        match self {
            ErrorKind::InvalidSyntax { .. } => "E001",
            ErrorKind::UnexpectedToken { .. } => "E002",
            ErrorKind::IncompleteInput { .. } => "E003",
            ErrorKind::TypeMismatch { .. } => "E100",
            ErrorKind::UnknownType { .. } => "E101",
            ErrorKind::ArityMismatch { .. } => "E103",
            ErrorKind::InvalidOperation { .. } => "E104",
            ErrorKind::NotCallable { .. } => "E105",
            ErrorKind::FieldNotFound { .. } => "E106",
            ErrorKind::MethodNotFound { .. } => "E107",
            ErrorKind::VariantNotFound { .. } => "E108",
            ErrorKind::GenericArgCountMismatch { .. } => "E109",
            ErrorKind::InvalidType { .. } => "E110",
            ErrorKind::InvalidControlFlow { .. } => "E111",
            ErrorKind::NonExhaustiveMatch { .. } => "E112",
            ErrorKind::UndefinedSymbol { .. } => "E200",
            ErrorKind::DuplicateDefinition { .. } => "E201",
            ErrorKind::InvalidSymbolAccess { .. } => "E202",
            ErrorKind::UseAfterMove { .. } => "E300",
            ErrorKind::UseAfterDrop { .. } => "E301",
            ErrorKind::InvalidMemoryOperation { .. } => "E302",
            ErrorKind::BorrowViolation { .. } => "E303",
            ErrorKind::LifetimeError { .. } => "E400",
            ErrorKind::BorrowConflict { .. } => "E401",
            ErrorKind::ModuleNotFound { .. } => "E500",
            ErrorKind::CircularImport { .. } => "E501",
            ErrorKind::InvalidImport { .. } => "E502",
            ErrorKind::CfcNotFound { .. } => "E503",
            ErrorKind::CHeaderNotFound { .. } => "E504",
            ErrorKind::CLibraryNotInstalled { .. } => "E505",
            ErrorKind::CDepCycle { .. } => "E506",
            ErrorKind::ConstraintViolation { .. } => "E600",
            ErrorKind::CodeGeneration { .. } => "E700",
            ErrorKind::Verification { .. } => "E800",
            ErrorKind::LinkError { .. } => "E900",
        }
    }

    /// Get the error category description
    pub fn category(&self) -> &'static str {
        match self {
            ErrorKind::InvalidSyntax { .. } |
            ErrorKind::UnexpectedToken { .. } |
            ErrorKind::IncompleteInput { .. } => "syntax",

            ErrorKind::TypeMismatch { .. } |
            ErrorKind::UnknownType { .. } |
            ErrorKind::ArityMismatch { .. } |
            ErrorKind::InvalidOperation { .. } |
            ErrorKind::NotCallable { .. } |
            ErrorKind::FieldNotFound { .. } |
            ErrorKind::MethodNotFound { .. } |
            ErrorKind::VariantNotFound { .. } |
            ErrorKind::GenericArgCountMismatch { .. } |
            ErrorKind::InvalidType { .. } |
            ErrorKind::InvalidControlFlow { .. } |
            ErrorKind::NonExhaustiveMatch { .. } => "type",

            ErrorKind::UndefinedSymbol { .. } |
            ErrorKind::DuplicateDefinition { .. } |
            ErrorKind::InvalidSymbolAccess { .. } => "symbol",

            ErrorKind::UseAfterMove { .. } |
            ErrorKind::UseAfterDrop { .. } |
            ErrorKind::InvalidMemoryOperation { .. } |
            ErrorKind::BorrowViolation { .. } => "memory",

            ErrorKind::LifetimeError { .. } |
            ErrorKind::BorrowConflict { .. } => "lifetime",

            ErrorKind::ModuleNotFound { .. } |
            ErrorKind::CircularImport { .. } |
            ErrorKind::InvalidImport { .. } |
            ErrorKind::CfcNotFound { .. } |
            ErrorKind::CHeaderNotFound { .. } |
            ErrorKind::CDepCycle { .. } => "import",

            ErrorKind::CLibraryNotInstalled { .. } => "link",

            ErrorKind::ConstraintViolation { .. } => "constraint",

            ErrorKind::CodeGeneration { .. } => "codegen",

            ErrorKind::Verification { .. } => "verification",

            ErrorKind::LinkError { .. } => "link",
        }
    }

    /// Get a user-friendly description of this error
    pub fn description(&self) -> String {
        match self {
            ErrorKind::InvalidSyntax { context } => {
                format!("Invalid syntax: {}", context)
            }
            ErrorKind::UnexpectedToken { token, expected } => {
                if expected.is_empty() {
                    format!("Unexpected token '{}'", token)
                } else {
                    format!("Unexpected token '{}'; expected one of: {}", token, expected.join(", "))
                }
            }
            ErrorKind::IncompleteInput { expected } => {
                format!("Incomplete input: expected {}", expected)
            }
            ErrorKind::TypeMismatch { expected, found } => {
                format!("Type mismatch: expected {}, found {}", expected, found)
            }
            ErrorKind::UnknownType { name } => {
                format!("Unknown type '{}'", name)
            }
            ErrorKind::ArityMismatch { expected, found } => {
                format!("Wrong number of arguments: expected {}, found {}", expected, found)
            }
            ErrorKind::InvalidOperation { op, left, right } => {
                format!("Invalid operation: {} {} {}", left, op, right)
            }
            ErrorKind::NotCallable { ty } => {
                format!("Type '{}' is not callable", ty)
            }
            ErrorKind::FieldNotFound { type_name, field_name } => {
                format!("Field '{}' not found on type '{}'", field_name, type_name)
            }
            ErrorKind::MethodNotFound { type_name, method_name } => {
                format!("Method '{}' not found on type '{}'", method_name, type_name)
            }
            ErrorKind::VariantNotFound { enum_name, variant_name } => {
                format!("Variant '{}' not found on enum '{}'", variant_name, enum_name)
            }
            ErrorKind::GenericArgCountMismatch { type_name, expected, found } => {
                format!("Type '{}' expected {} generic arguments, found {}", type_name, expected, found)
            }
            ErrorKind::InvalidType { name, reason } => {
                format!("Invalid type '{}': {}", name, reason)
            }
            ErrorKind::InvalidControlFlow { keyword } => {
                format!("'{}' used outside of a loop", keyword)
            }
            ErrorKind::NonExhaustiveMatch { ty } => {
                format!("non-exhaustive match on '{}'", ty)
            }
            ErrorKind::CodeGeneration { stage, details } => {
                format!("Code generation error in {}: {}", stage, details)
            }
            ErrorKind::Verification { phase, details } => {
                format!("Verification error in {}: {}", phase, details)
            }
            ErrorKind::LinkError { details } => {
                format!("Link error: {}", details)
            }
            ErrorKind::CfcNotFound { library } => {
                format!(
                    ".cfc not found for library '{library}'. Searched `.`, `lib/`, and `target/cfc`. \
                     Hint: generate one with `coffee -c <header.h> -o lib{library}.cfc`, \
                     or declare `[dependencies.c_libraries.{library}]` with `headers = [...]`."
                )
            }
            ErrorKind::CHeaderNotFound { header, searched } => {
                format!(
                    "cannot open header '{header}'; searched: {searched}. \
                     Hint: pass `-I` / set include_paths, or install the C library that provides this header."
                )
            }
            ErrorKind::CLibraryNotInstalled { name, searched } => {
                format!(
                    "cannot find lib{name}.so or lib{name}.a; searched: {searched}. \
                     Hint: install the package that provides lib{name}, or set PREFIX so the linker can find it."
                )
            }
            ErrorKind::CDepCycle { path } => {
                format!("C library dependency cycle: {}", path.join(" -> "))
            }
            _ => format!("{:?}", self)
        }
    }
}

//=============================================================================
// Diagnostic Message
//=============================================================================

/// Calculate Levenshtein distance between two strings
pub fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();

    if a_len == 0 { return b_len; }
    if b_len == 0 { return a_len; }

    let mut matrix = vec![vec![0usize; b_len + 1]; a_len + 1];

    for i in 0..=a_len { matrix[i][0] = i; }
    for j in 0..=b_len { matrix[0][j] = j; }

    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }

    matrix[a_len][b_len]
}

/// Find similar names from a list of candidates
pub fn find_similar_names(target: &str, candidates: &[String], max_distance: usize, max_results: usize) -> Vec<String> {
    let mut similar: Vec<(String, usize)> = candidates
        .iter()
        .filter_map(|name| {
            let distance = levenshtein_distance(target, name);
            if distance <= max_distance && distance > 0 {
                Some((name.clone(), distance))
            } else {
                None
            }
        })
        .collect();

    similar.sort_by_key(|(_, d)| *d);
    similar.truncate(max_results);
    similar.into_iter().map(|(name, _)| name).collect()
}

/// A complete diagnostic message with error chain support
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// Severity level
    pub severity: Severity,
    /// Error kind
    pub kind: ErrorKind,
    /// Primary message
    pub message: String,
    /// Source location
    pub location: SourceLocation,
    /// Error code
    pub code: String,
    /// Code snippets showing the error
    pub snippets: Vec<CodeSnippet>,
    /// Suggestions for fixing the error
    pub suggestions: Vec<Suggestion>,
    /// Related diagnostics
    pub related: Vec<RelatedDiagnostic>,
    /// Error chain - the cause of this error
    pub cause: Option<Box<Diagnostic>>,
    /// Similar names for undefined symbol errors
    pub similar_names: Vec<String>,
}

impl Diagnostic {
    /// Create a new diagnostic
    pub fn new(severity: Severity, kind: ErrorKind, message: impl Into<String>) -> Self {
        let code = kind.error_code().to_string();
        Diagnostic {
            severity,
            kind,
            message: message.into(),
            location: SourceLocation::default(),
            code,
            snippets: Vec::new(),
            suggestions: Vec::new(),
            related: Vec::new(),
            cause: None,
            similar_names: Vec::new(),
        }
    }

    /// Set the source location
    pub fn with_location(mut self, location: SourceLocation) -> Self {
        self.location = location;
        self
    }

    /// Add a code snippet
    pub fn with_snippet(mut self, snippet: CodeSnippet) -> Self {
        self.snippets.push(snippet);
        self
    }

    /// Add a suggestion
    pub fn with_suggestion(mut self, suggestion: Suggestion) -> Self {
        self.suggestions.push(suggestion);
        self
    }

    /// Add a related diagnostic
    pub fn with_related(mut self, related: RelatedDiagnostic) -> Self {
        self.related.push(related);
        self
    }

    /// Format the diagnostic for display (with colors)
    pub fn format(&self) -> String {
        let mut output = String::new();
        let reset = Severity::reset();
        let cyan = "\x1b[36m";
        let gray = "\x1b[90m";

        // Header line: "error[E200]: message"
        output.push_str(&format!("{}[{}]:{} {}{}{}\n",
            self.severity.color_bold(),
            self.code,
            reset,
            self.severity.color(),
            self.message,
            reset,
        ));

        // Location: "   --> file:line:col"
        if self.location.is_valid() {
            output.push_str(&format!("   --> {}\n", self.location.format()));
        }

        // Category hint
        output.push_str(&format!("    | this is a {} error\n", self.kind.category()));

        // Code snippets
        for snippet in &self.snippets {
            output.push_str(&snippet.format_colored());
        }

        // Similar names (did you mean?)
        if !self.similar_names.is_empty() {
            output.push_str(&format!("    {}|{} did you mean: ", gray, reset));
            let suggestions: Vec<String> = self.similar_names.iter()
                .map(|name| format!("{}`{}`{}", cyan, name, reset))
                .collect();
            output.push_str(&suggestions.join(", "));
            output.push('\n');
        }

        // Suggestions
        for suggestion in &self.suggestions {
            output.push_str(&suggestion.format());
        }

        // Related diagnostics
        for related in &self.related {
            output.push_str(&related.format());
        }

        // Error chain (cause)
        if let Some(ref cause) = self.cause {
            output.push_str(&format!("\n{}caused by:{}\n", gray, reset));
            // Indent the cause
            for line in cause.format().lines() {
                output.push_str(&format!("    {}\n", line));
            }
        }

        output
    }

    /// Format without colors
    pub fn format_plain(&self) -> String {
        let mut output = String::new();

        output.push_str(&format!("{}[{}]: {}\n", self.severity, self.code, self.message));

        if self.location.is_valid() {
            output.push_str(&format!("   --> {}\n", self.location.format()));
        }

        for snippet in &self.snippets {
            output.push_str(&snippet.format_plain());
        }

        // Similar names
        if !self.similar_names.is_empty() {
            output.push_str(&format!("    | did you mean: {}\n", self.similar_names.join(", ")));
        }

        for suggestion in &self.suggestions {
            output.push_str(&suggestion.format_plain());
        }

        // Error chain
        if let Some(ref cause) = self.cause {
            output.push_str("\ncaused by:\n");
            for line in cause.format_plain().lines() {
                output.push_str(&format!("    {}\n", line));
            }
        }

        output
    }
}

//=============================================================================
// Code Snippets
//=============================================================================

/// A snippet of source code with highlighting
#[derive(Debug, Clone)]
pub struct CodeSnippet {
    /// The source code lines
    pub lines: Vec<String>,
    /// Line number of the first line
    pub line_start: usize,
    /// Highlight ranges (line_index, column_start, column_end)
    pub highlights: Vec<(usize, usize, usize)>,
}

impl CodeSnippet {
    /// Create a new code snippet
    pub fn new(lines: Vec<String>, line_start: usize) -> Self {
        CodeSnippet {
            lines,
            line_start,
            highlights: Vec::new(),
        }
    }

    /// Format the snippet with colors
    fn format_colored(&self) -> String {
        let mut output = String::new();
        let red = "\x1b[31m";
        let red_bold = "\x1b[1;31m";
        let gray = "\x1b[90m";
        let reset = "\x1b[0m";

        for (i, line) in self.lines.iter().enumerate() {
            let line_num = self.line_start + i;
            output.push_str(&format!("{}{:4} |{} {}\n", gray, line_num, reset, line));

            // Add highlight line for this line
            for (hl_line, start, end) in &self.highlights {
                if i == *hl_line {
                    output.push_str(&format!("{}     |{} ", gray, reset));
                    output.push_str(&" ".repeat(*start));
                    output.push_str(&format!("{}{}{}{}\n",
                        red_bold, "^".repeat(end.saturating_sub(*start).max(1)), red, reset));
                }
            }
        }

        output
    }

    /// Format without colors
    fn format_plain(&self) -> String {
        let mut output = String::new();

        for (i, line) in self.lines.iter().enumerate() {
            let line_num = self.line_start + i;
            output.push_str(&format!("{:4} | {}\n", line_num, line));

            for (hl_line, start, end) in &self.highlights {
                if i == *hl_line {
                    output.push_str(&format!("     | {}", " ".repeat(*start)));
                    let caret = "^".repeat(end.saturating_sub(*start).max(1));
                    output.push_str(&format!("{}\n", caret));
                }
            }
        }

        output
    }
}

//=============================================================================
// Suggestions
//=============================================================================

/// A suggestion for fixing the issue
#[derive(Debug, Clone)]
pub struct Suggestion {
    /// Suggestion message
    pub message: String,
}

impl Suggestion {
    /// Create a new suggestion
    pub fn new(message: impl Into<String>) -> Self {
        Suggestion {
            message: message.into(),
        }
    }

    /// Format with colors
    fn format(&self) -> String {
        let cyan = "\x1b[36m";
        let reset = "\x1b[0m";
        format!("    {}: {}help: {}{}\n", cyan, reset, cyan, self.message)
    }

    /// Format without colors
    fn format_plain(&self) -> String {
        format!("    help: {}\n", self.message)
    }
}

//=============================================================================
// Related Diagnostics
//=============================================================================

/// Related diagnostic information
#[derive(Debug, Clone)]
pub struct RelatedDiagnostic {
    /// Relation type
    pub relation: RelationType,
    /// Location
    pub location: SourceLocation,
    /// Message
    pub message: String,
}

impl RelatedDiagnostic {
    /// Create a new related diagnostic
    pub fn new(relation: RelationType, location: SourceLocation, message: impl Into<String>) -> Self {
        RelatedDiagnostic {
            relation,
            location,
            message: message.into(),
        }
    }

    /// Format with colors
    fn format(&self) -> String {
        let gray = "\x1b[90m";
        let reset = "\x1b[0m";
        format!("{}{}{} {}: {}\n", gray, self.relation, reset, self.location.format(), self.message)
    }
}

/// Relation type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationType {
    CausedBy,
}

impl fmt::Display for RelationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RelationType::CausedBy => write!(f, "caused by"),
        }
    }
}

//=============================================================================
// Diagnostic Emitter
//=============================================================================

/// Diagnostic emitter - collects and formats diagnostics
#[derive(Debug, Clone)]
pub struct DiagnosticEmitter {
    /// Collected diagnostics
    diagnostics: Arc<RwLock<Vec<Diagnostic>>>,
}

impl DiagnosticEmitter {
    /// Create a new diagnostic emitter
    pub fn new() -> Self {
        DiagnosticEmitter {
            diagnostics: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Emit a diagnostic
    pub fn emit(&self, diagnostic: Diagnostic) {
        if let Ok(mut diagnostics) = self.diagnostics.write() {
            diagnostics.push(diagnostic);
        }
    }

    /// Get all diagnostics
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        if let Ok(diagnostics) = self.diagnostics.read() {
            diagnostics.clone()
        } else {
            Vec::new()
        }
    }

    /// Clear all diagnostics
    pub fn clear(&self) {
        if let Ok(mut diagnostics) = self.diagnostics.write() {
            diagnostics.clear();
        }
    }
}

impl Default for DiagnosticEmitter {
    fn default() -> Self {
        Self::new()
    }
}

//=============================================================================
// Conversion from Space Errors
//=============================================================================

impl From<types::TypeSystemError> for Diagnostic {
    fn from(error: types::TypeSystemError) -> Self {
        let message = error.to_string();
        let similar_names = error.similar_names();
        let extra_suggestions = error.diagnostic_suggestions();
        let mut diagnostic = match error {
            types::TypeSystemError::TypeMismatch { expected, found, .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::TypeMismatch {
                    expected: format!("{:?}", expected),
                    found: format!("{:?}", found),
                }, format!("type mismatch: expected {}, found {:?}", expected, found))
            }

            types::TypeSystemError::UndefinedVariable { name, .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::UndefinedSymbol {
                    name: name.clone(),
                    symbol_type: SymbolType::Variable,
                }, format!("undefined variable: '{}'", name))
            }

            types::TypeSystemError::UndefinedFunction { name, .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::UndefinedSymbol {
                    name: name.clone(),
                    symbol_type: SymbolType::Function,
                }, format!("undefined function: '{}'", name))
            }

            types::TypeSystemError::UndefinedType { name, .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::UnknownType {
                    name: name.clone(),
                }, format!("undefined type: '{}'", name))
            }

            types::TypeSystemError::ArityMismatch { expected, found, .. } => {
                let msg = format!("arity mismatch: expected {} arguments, found {}", expected, found);
                Diagnostic::new(Severity::Error, ErrorKind::ArityMismatch { expected, found }, msg)
            }

            types::TypeSystemError::InvalidOperation { op, left, right, .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::InvalidOperation {
                    op: op.clone(),
                    left: left.to_string(),
                    right: right.to_string(),
                }, format!("invalid operation: {} {} {}", left, op, right))
            }

            types::TypeSystemError::NotCallable { ty, .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::NotCallable {
                    ty: ty.to_string(),
                }, format!("type '{}' is not callable", ty))
            }

            types::TypeSystemError::FieldNotFound { type_name, field_name, .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::FieldNotFound {
                    type_name: type_name.clone(),
                    field_name: field_name.clone(),
                }, format!("field '{}' not found in type '{}'", field_name, type_name))
            }

            types::TypeSystemError::GenericArgCountMismatch { type_name, expected, found, .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::GenericArgCountMismatch {
                    type_name: type_name.clone(),
                    expected,
                    found,
                }, format!("type '{}' expected {} generic arguments, found {}", type_name, expected, found))
            }

            types::TypeSystemError::NotFound { name, kind } => {
                let (kind, message) = match kind {
                    types::SpaceKind::Scope => (
                        ErrorKind::UndefinedSymbol {
                            name: name.clone(),
                            symbol_type: SymbolType::Variable,
                        },
                        format!("undefined variable: '{}'", name)
                    ),
                    types::SpaceKind::Type => (
                        ErrorKind::UnknownType {
                            name: name.clone(),
                        },
                        format!("unknown type: '{}'", name)
                    ),
                    types::SpaceKind::Symbol => (
                        ErrorKind::UndefinedSymbol {
                            name: name.clone(),
                            symbol_type: SymbolType::Variable,
                        },
                        format!("undefined symbol: '{}'", name)
                    ),
                    types::SpaceKind::Lifetime => (
                        ErrorKind::LifetimeError {
                            variable: name.clone(),
                            reason: "lifetime not found".to_string(),
                        },
                        format!("lifetime not found: '{}'", name)
                    ),
                };

                Diagnostic::new(Severity::Error, kind, message)
            }

            types::TypeSystemError::Duplicate { name, existing, new: _ } => {
                // Create main error with helpful message
                let message = format!(
                    "duplicate definition of '{}': \
                    the name '{}' is defined multiple times in this scope",
                    name, name
                );

                // Try to create location information from spans
                // Note: Span only has byte offsets, not line numbers
                // For better error messages, we'd need to track line numbers in Span
                let prev_loc = if existing.start > 0 || existing.end > 0 {
                    SourceLocation {
                        line_start: 1,  // Default to showing line 1 (we don't have line info)
                        column_start: existing.start,
                        line_end: 1,
                        column_end: existing.end,
                        byte_offset: existing.start,
                        byte_length: existing.end - existing.start,
                        file: None,
                    }
                } else {
                    SourceLocation::default()
                };

                let mut diagnostic = Diagnostic::new(
                    Severity::Error,
                    ErrorKind::DuplicateDefinition {
                        name: name.clone(),
                        previous: prev_loc,
                    },
                    message
                );

                // Add helpful suggestion
                diagnostic = diagnostic.with_suggestion(Suggestion::new(
                    "consider renaming one of the definitions or removing the duplicate"
                ));

                diagnostic
            }

            types::TypeSystemError::Cycle { path } => {
                Diagnostic::new(Severity::Error, ErrorKind::CircularImport {
                    path: path.clone(),
                }, format!("circular import detected: {}", path.join(" -> ")))
            }

            types::TypeSystemError::ConstraintViolation { constraint, reason, .. } => {
                if constraint.contains("exhaustive") {
                    Diagnostic::new(
                        Severity::Error,
                        ErrorKind::NonExhaustiveMatch { ty: reason.clone() },
                        format!("non-exhaustive match: {}", reason),
                    )
                } else {
                    Diagnostic::new(Severity::Error, ErrorKind::ConstraintViolation {
                        constraint: constraint.clone(),
                        reason: reason.clone(),
                    }, format!("constraint violation: {}", reason))
                }
            }

            types::TypeSystemError::VisibilityError { name, required, actual } => {
                Diagnostic::new(Severity::Error, ErrorKind::InvalidSymbolAccess {
                    name: name.clone(),
                    reason: format!("required {}, actual {}", required, actual),
                }, format!("field access error: '{}' is not visible", name))
            }

            types::TypeSystemError::Internal { reason } => {
                Diagnostic::new(Severity::Error, ErrorKind::CodeGeneration {
                    stage: "type system".to_string(),
                    details: reason.clone(),
                }, format!("internal type-system error: {}", reason))
            }

            types::TypeSystemError::ParseError { type_str, reason } => {
                if type_str == "break" || type_str == "continue" {
                    Diagnostic::new(
                        Severity::Error,
                        ErrorKind::InvalidControlFlow { keyword: type_str.clone() },
                        format!("{} used outside of a loop", type_str),
                    )
                } else {
                    Diagnostic::new(Severity::Error, ErrorKind::InvalidType {
                        name: type_str.clone(),
                        reason: reason.clone(),
                    }, format!("type error in '{}': {}", type_str, reason))
                }
            }

            types::TypeSystemError::InstantiationError { type_name, reason, .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::InvalidType {
                    name: type_name.clone(),
                    reason: reason.clone(),
                }, format!("failed to instantiate type '{}': {}", type_name, reason))
            }

            types::TypeSystemError::MethodNotFound { type_name, method_name, .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::MethodNotFound {
                    type_name: type_name.clone(),
                    method_name: method_name.clone(),
                }, format!("method '{}' not found in type '{}'", method_name, type_name))
            }

            types::TypeSystemError::VariantNotFound { enum_name, variant_name, .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::VariantNotFound {
                    enum_name: enum_name.clone(),
                    variant_name: variant_name.clone(),
                }, format!("variant '{}' not found in enum '{}'", variant_name, enum_name))
            }

            types::TypeSystemError::LifetimeError { reason, .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::LifetimeError {
                    variable: String::new(),
                    reason: reason.clone(),
                }, format!("lifetime error: {}", reason))
            }

            types::TypeSystemError::OwnershipError { reason, .. } => {
                let lowered = reason.to_ascii_lowercase();
                let kind = if lowered.contains("moved") {
                    ErrorKind::UseAfterMove { name: String::new() }
                } else if lowered.contains("dropped") {
                    ErrorKind::UseAfterDrop { name: String::new() }
                } else {
                    ErrorKind::InvalidMemoryOperation {
                        operation: "ownership".to_string(),
                        reason: reason.clone(),
                    }
                };
                Diagnostic::new(Severity::Error, kind, format!("ownership error: {}", reason))
            }

            types::TypeSystemError::BorrowViolation { variable, reason, .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::BorrowViolation {
                    variable: variable.clone(),
                    reason: reason.clone(),
                }, reason.clone())
            }

            types::TypeSystemError::BorrowConflict { variable, reason, .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::BorrowConflict {
                    variable: variable.clone(),
                }, reason.clone())
            }
        };
        diagnostic.message = message;
        diagnostic.similar_names = similar_names;
        for suggestion in extra_suggestions {
            diagnostic = diagnostic.with_suggestion(Suggestion::new(suggestion));
        }
        diagnostic
    }
}

// Also implement for references
impl From<&types::TypeSystemError> for Diagnostic {
    fn from(error: &types::TypeSystemError) -> Self {
        Diagnostic::from(error.clone())
    }
}

//=============================================================================
// Conversion from Parse Errors
//=============================================================================

impl From<crate::parser::ParseError> for Diagnostic {
    fn from(error: crate::parser::ParseError) -> Self {
        use crate::parser::ParseError;

        let line = error.line();
        let kind = match &error {
            ParseError::UnexpectedKeyword { keyword, expected, .. } => {
                ErrorKind::UnexpectedToken {
                    token: keyword.clone(),
                    expected: expected.clone(),
                }
            }
            ParseError::MissingColon { function_name, .. } => ErrorKind::IncompleteInput {
                expected: format!("':' after function '{}'", function_name),
            },
            ParseError::MissingReturnType { function_name, .. } => ErrorKind::IncompleteInput {
                expected: format!("return type after '=>' for function '{}'", function_name),
            },
            ParseError::MissingClosingParen { context, .. } => ErrorKind::IncompleteInput {
                expected: format!("')' to close parenthesis in '{}'", context.trim()),
            },
            ParseError::MissingBlockBody { statement_type, .. } => ErrorKind::IncompleteInput {
                expected: format!("indented block body after '{}:'", statement_type),
            },
            ParseError::MissingParameterName { function_name, .. } => ErrorKind::IncompleteInput {
                expected: format!("parameter name in function '{}'", function_name),
            },
            ParseError::MissingTypeAnnotation { parameter_name, .. } => ErrorKind::IncompleteInput {
                expected: format!("type annotation for parameter '{}'", parameter_name),
            },
            ParseError::WrongCommentSyntax { wrong_syntax, correct_syntax, .. } => {
                ErrorKind::InvalidSyntax {
                    context: format!(
                        "comment syntax '{}'; Coffee uses '{}' (not '//')",
                        wrong_syntax, correct_syntax
                    ),
                }
            }
            ParseError::InvalidUseOfBraces { context, brace_type, .. } => {
                ErrorKind::InvalidSyntax {
                    context: format!(
                        "brace '{}' in '{}'; Coffee is indentation-based and does not use braces",
                        brace_type,
                        context.trim()
                    ),
                }
            }
            ParseError::GenericSyntaxError { hint, .. } => ErrorKind::InvalidSyntax {
                context: hint.clone(),
            },
            ParseError::InvalidFunctionSyntax { context, expected, .. } => {
                ErrorKind::InvalidSyntax {
                    context: format!("{} (expected {})", context.trim(), expected),
                }
            }
        };

        let mut diagnostic = Diagnostic::new(Severity::Error, kind, error.to_message());
        diagnostic.location = SourceLocation::new(line, 1, 100);
        for suggestion in error.suggestions() {
            diagnostic = diagnostic.with_suggestion(Suggestion::new(suggestion));
        }
        diagnostic
    }
}

impl From<&crate::parser::ParseError> for Diagnostic {
    fn from(error: &crate::parser::ParseError) -> Self {
        Diagnostic::from(error.clone())
    }
}

#[cfg(test)]
mod type_error_kind_tests {
    use super::*;
    use crate::types::TypeSystemError;
    use crate::types::definition::{Span, Type};

    fn code(err: TypeSystemError) -> &'static str {
        Diagnostic::from(err).kind.error_code()
    }

    #[test]
    fn mismatch_is_e100() {
        assert_eq!(
            code(TypeSystemError::type_mismatch(Type::int(), Type::bool(), Span::new(0, 1))),
            "E100"
        );
    }

    #[test]
    fn unknown_type_is_e101() {
        assert_eq!(
            code(TypeSystemError::undefined_type("Nope", Span::new(0, 1))),
            "E101"
        );
    }

    #[test]
    fn arity_is_e103() {
        assert_eq!(
            code(TypeSystemError::arity_mismatch(2, 1, Span::new(0, 1))),
            "E103"
        );
    }

    #[test]
    fn invalid_operation_is_e104() {
        assert_eq!(
            code(TypeSystemError::invalid_operation("!", Type::int(), Type::unit(), Span::new(0, 1))),
            "E104"
        );
    }

    #[test]
    fn not_callable_is_e105() {
        assert_eq!(
            code(TypeSystemError::not_callable(Type::int(), Span::new(0, 1))),
            "E105"
        );
    }

    #[test]
    fn field_not_found_is_e106() {
        assert_eq!(
            code(TypeSystemError::field_not_found("Point", "z", Span::new(0, 1))),
            "E106"
        );
    }

    #[test]
    fn method_not_found_is_e107() {
        let err = TypeSystemError::MethodNotFound {
            type_name: "Point".into(),
            method_name: "nope".into(),
            span: Span::new(0, 1),
            similar: Vec::new(),
        };
        assert_eq!(code(err), "E107");
    }

    #[test]
    fn variant_not_found_is_e108() {
        let err = TypeSystemError::VariantNotFound {
            enum_name: "Color".into(),
            variant_name: "Purple".into(),
            span: Span::new(0, 1),
            similar: Vec::new(),
        };
        assert_eq!(code(err), "E108");
    }

    #[test]
    fn invalid_type_parse_is_e110() {
        let err = TypeSystemError::ParseError {
            type_str: "int[".into(),
            reason: "unclosed bracket".into(),
        };
        assert_eq!(code(err.clone()), "E110");
        let text = err.to_string();
        assert!(text.contains("failed to parse type"));
        assert!(text.contains("type syntax is invalid"));
    }

    #[test]
    fn internal_is_e700_not_type_syntax() {
        let err = TypeSystemError::internal("type registry lock poisoned");
        assert_eq!(code(err.clone()), "E700");
        let text = err.to_string();
        assert!(text.contains("internal type-system error"));
        assert!(!text.contains("failed to parse type"));
        assert!(!text.contains("type syntax is invalid"));
    }

    #[test]
    fn not_a_class_is_e104() {
        let err = TypeSystemError::invalid_operation(
            "member access",
            Type::NamedType { name: "Color".into() },
            Type::unit(),
            Span::new(0, 1),
        );
        assert_eq!(code(err), "E104");
    }

    #[test]
    fn borrow_non_place_is_e303() {
        let err = TypeSystemError::BorrowViolation {
            variable: String::new(),
            reason: "can only borrow a local variable".into(),
            span: Span::new(0, 1),
        };
        assert_eq!(code(err), "E303");
    }

    #[test]
    fn ownership_moved_is_e300() {
        let err = TypeSystemError::OwnershipError {
            reason: "Cannot clone 'x' - value was moved".into(),
            span: Span::new(0, 1),
        };
        assert_eq!(code(err), "E300");
    }

    #[test]
    fn break_outside_loop_is_e111() {
        let err = TypeSystemError::ParseError {
            type_str: "break".into(),
            reason: "break used outside of a loop".into(),
        };
        assert_eq!(code(err), "E111");
    }

    #[test]
    fn non_exhaustive_match_is_e112() {
        let err = TypeSystemError::ConstraintViolation {
            constraint: "exhaustive match".into(),
            reason: "missing variants: None".into(),
            span: Span::new(0, 1),
        };
        assert_eq!(code(err), "E112");
    }
}

#[cfg(test)]
mod parse_error_diagnostic_tests {
    use super::*;
    use crate::parser::ParseError;

    #[test]
    fn parse_error_suggestions_appear_on_diagnostic() {
        let err = ParseError::detect_error(1, "import math");
        let hints = err.suggestions();
        assert!(!hints.is_empty());
        let diag = Diagnostic::from(err);
        assert_eq!(diag.suggestions.len(), hints.len());
        let rendered = diag.format_plain();
        assert!(rendered.contains("use"), "{rendered}");
        assert!(hints.iter().any(|h| rendered.contains(h)), "{rendered}");
    }

    #[test]
    fn try_keyword_diagnostic_mentions_raise() {
        let err = ParseError::detect_error(1, "try:");
        let rendered = Diagnostic::from(err).format_plain();
        assert!(rendered.contains("raise"), "{rendered}");
    }
}

#[cfg(test)]
mod c_error_kind_tests {
    use super::*;

    fn diag(kind: ErrorKind) -> Diagnostic {
        let message = kind.description();
        Diagnostic::new(Severity::Error, kind, message)
    }

    #[test]
    fn cfc_not_found_is_e503_import_with_hint() {
        let kind = ErrorKind::CfcNotFound {
            library: "ssl".into(),
        };
        assert_eq!(kind.error_code(), "E503");
        assert_eq!(kind.category(), "import");
        let rendered = diag(kind).format_plain();
        assert!(
            rendered.contains("hint")
                || rendered.contains("help")
                || rendered.contains("coffee -c"),
            "{rendered}"
        );
        assert!(rendered.contains("coffee -c"), "{rendered}");
        assert!(rendered.contains("ssl"), "{rendered}");
    }

    #[test]
    fn c_header_not_found_is_e504_and_lists_searched() {
        let searched = "., /usr/include, /usr/local/include";
        let kind = ErrorKind::CHeaderNotFound {
            header: "openssl/ssl.h".into(),
            searched: searched.into(),
        };
        assert_eq!(kind.error_code(), "E504");
        assert_eq!(kind.category(), "import");
        let rendered = diag(kind).format_plain();
        assert!(rendered.contains(searched), "{rendered}");
        assert!(rendered.contains("openssl/ssl.h"), "{rendered}");
    }

    #[test]
    fn c_library_not_installed_is_e505_link_and_lists_searched() {
        let searched = "., ./lib, /usr/lib";
        let kind = ErrorKind::CLibraryNotInstalled {
            name: "ssl".into(),
            searched: searched.into(),
        };
        assert_eq!(kind.error_code(), "E505");
        assert_eq!(kind.category(), "link");
        let rendered = diag(kind).format_plain();
        assert!(rendered.contains(searched), "{rendered}");
        assert!(rendered.contains("libssl.so") || rendered.contains("libssl"), "{rendered}");
    }

    #[test]
    fn c_dep_cycle_is_e506_import() {
        let kind = ErrorKind::CDepCycle {
            path: vec!["a".into(), "b".into(), "a".into()],
        };
        assert_eq!(kind.error_code(), "E506");
        assert_eq!(kind.category(), "import");
        let desc = kind.description();
        assert!(desc.contains("a -> b -> a"), "{desc}");
        assert!(!desc.contains("CDepCycle"), "{desc}");
    }
}
