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
    /// Note - additional information
    Note,
}

impl Severity {
    /// Returns the ANSI color code for this severity
    pub fn color(self) -> &'static str {
        match self {
            Severity::Error => "\x1b[31m",    // Red
            Severity::Warning => "\x1b[33m",  // Yellow
            Severity::Hint => "\x1b[36m",    // Cyan
            Severity::Note => "\x1b[90m",    // Gray
        }
    }

    /// Returns the bold ANSI color code for this severity
    pub fn color_bold(self) -> &'static str {
        match self {
            Severity::Error => "\x1b[1;31m",  // Bold Red
            Severity::Warning => "\x1b[1;33m", // Bold Yellow
            Severity::Hint => "\x1b[1;36m",   // Bold Cyan
            Severity::Note => "\x1b[1;90m",   // Bold Gray
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
            Severity::Note => write!(f, "note"),
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
    #[allow(dead_code)]
    InvalidSyntax { context: String },
    #[allow(dead_code)]
    UnexpectedToken { token: String, expected: Vec<String> },
    #[allow(dead_code)]
    IncompleteInput { expected: String },

    // Type Errors (E100-E199)
    #[allow(dead_code)]
    TypeMismatch { expected: String, found: String },
    #[allow(dead_code)]
    UnknownType { name: String },
    #[allow(dead_code)]
    IncompatibleTypes { ty1: String, ty2: String },

    // Symbol Errors (E200-E299)
    #[allow(dead_code)]
    UndefinedSymbol { name: String, symbol_type: SymbolType },
    #[allow(dead_code)]
    DuplicateDefinition { name: String, previous: SourceLocation },
    #[allow(dead_code)]
    InvalidSymbolAccess { name: String, reason: String },

    // Memory Errors (E300-E399)
    #[allow(dead_code)]
    UseAfterMove { name: String },
    #[allow(dead_code)]
    UseAfterDrop { name: String },
    #[allow(dead_code)]
    InvalidMemoryOperation { operation: String, reason: String },
    #[allow(dead_code)]
    BorrowViolation { variable: String, reason: String },

    // Lifetime Errors (E400-E499)
    #[allow(dead_code)]
    LifetimeError { variable: String, reason: String },
    #[allow(dead_code)]
    BorrowConflict { variable: String },

    // Import Errors (E500-E599)
    #[allow(dead_code)]
    ModuleNotFound { module: String },
    #[allow(dead_code)]
    CircularImport { path: Vec<String> },
    #[allow(dead_code)]
    InvalidImport { import: String, reason: String },

    // Constraint Errors (E600-E699)
    #[allow(dead_code)]
    ConstraintViolation { constraint: String, reason: String },

    // Code Generation Errors (E700-E799)
    #[allow(dead_code)]
    CodeGeneration { stage: String, details: String },

    // Verification Errors (E800-E899)
    #[allow(dead_code)]
    Verification { phase: String, details: String },

    // Link Errors (E900-E999)
    #[allow(dead_code)]
    LinkError { details: String },
}

/// Symbol type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolType {
    Variable,
    Function,
    Type,
    Module,
    Lifetime,
    Constant,
    Method,
    Field,
}

impl fmt::Display for SymbolType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SymbolType::Variable => write!(f, "variable"),
            SymbolType::Function => write!(f, "function"),
            SymbolType::Type => write!(f, "type"),
            SymbolType::Module => write!(f, "module"),
            SymbolType::Lifetime => write!(f, "lifetime"),
            SymbolType::Constant => write!(f, "constant"),
            SymbolType::Method => write!(f, "method"),
            SymbolType::Field => write!(f, "field"),
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
            ErrorKind::IncompatibleTypes { .. } => "E102",
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
            ErrorKind::IncompatibleTypes { .. } => "type",

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
            ErrorKind::InvalidImport { .. } => "import",

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
            ErrorKind::CodeGeneration { stage, details } => {
                format!("Code generation error in {}: {}", stage, details)
            }
            ErrorKind::Verification { phase, details } => {
                format!("Verification error in {}: {}", phase, details)
            }
            ErrorKind::LinkError { details } => {
                format!("Link error: {}", details)
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
    #[allow(dead_code)]
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
        let prefix = "caused by";
        format!("{}{}{} {}: {}\n", gray, prefix, reset, self.location.format(), self.message)
    }
}

/// Relation type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationType {
    CausedBy,
}

//=============================================================================
// Diagnostic Emitter
//=============================================================================

/// Diagnostic emitter - collects and formats diagnostics
#[derive(Debug, Clone)]
pub struct DiagnosticEmitter {
    /// Collected diagnostics
    diagnostics: Arc<RwLock<Vec<Diagnostic>>>,
    /// Configuration
    #[allow(dead_code)]
    config: EmitterConfig,
}

/// Configuration for diagnostic emitter
#[derive(Debug, Clone)]
pub struct EmitterConfig {
    /// Show colors in output
    #[allow(dead_code)]
    pub show_colors: bool,
}

impl Default for EmitterConfig {
    fn default() -> Self {
        EmitterConfig {
            show_colors: true,
        }
    }
}

impl DiagnosticEmitter {
    /// Create a new diagnostic emitter
    pub fn new() -> Self {
        DiagnosticEmitter {
            diagnostics: Arc::new(RwLock::new(Vec::new())),
            config: EmitterConfig::default(),
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
        match error {
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
                Diagnostic::new(Severity::Error, ErrorKind::UndefinedSymbol {
                    name: name.clone(),
                    symbol_type: SymbolType::Type,
                }, format!("undefined type: '{}'", name))
            }

            types::TypeSystemError::ArityMismatch { expected, found, .. } => {
                // Create the main error
                let msg = format!("arity mismatch: expected {} arguments, found {}", expected, found);

                // Create a note with additional information
                let _note = Diagnostic::new(Severity::Note, ErrorKind::InvalidSyntax {
                    context: format!("expected {} {}", expected, if expected == 1 { "argument" } else { "arguments" }),
                }, format!("note: function expects {} {}", expected, if expected == 1 { "argument" } else { "arguments" }));

                Diagnostic::new(Severity::Error, ErrorKind::InvalidSyntax {
                    context: msg.clone(),
                }, msg)
            }

            types::TypeSystemError::InvalidOperation { op, left, right, .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::InvalidSyntax {
                    context: format!("invalid operation: {} {} {}", left, op, right),
                }, format!("invalid operation: {} {} {}", left, op, right))
            }

            types::TypeSystemError::NotCallable { ty, .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::InvalidSyntax {
                    context: format!("type '{}' is not callable", ty),
                }, format!("type '{}' is not callable", ty))
            }

            types::TypeSystemError::FieldNotFound { type_name, field_name, .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::InvalidSyntax {
                    context: format!("field '{}' not found in type '{}'", field_name, type_name),
                }, format!("field '{}' not found in type '{}'", field_name, type_name))
            }

            types::TypeSystemError::GenericArgCountMismatch { type_name, expected, found, .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::InvalidSyntax {
                    context: format!("type '{}' expected {} generic arguments, found {}", type_name, expected, found),
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
                Diagnostic::new(Severity::Error, ErrorKind::ConstraintViolation {
                    constraint: constraint.clone(),
                    reason: reason.clone(),
                }, format!("constraint violation: {}", reason))
            }

            types::TypeSystemError::VisibilityError { .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::UndefinedSymbol {
                    name: "field".to_string(),
                    symbol_type: SymbolType::Field,
                }, "field access error: field is not visible or does not exist")
            }

            types::TypeSystemError::ParseError { type_str, reason } => {
                Diagnostic::new(Severity::Error, ErrorKind::InvalidSyntax {
                    context: format!("parse error for type '{}': {}", type_str, reason),
                }, format!("parse error for type '{}': {}", type_str, reason))
            }

            types::TypeSystemError::InstantiationError { type_name, reason, .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::InvalidSyntax {
                    context: format!("failed to instantiate type '{}': {}", type_name, reason),
                }, format!("failed to instantiate type '{}': {}", type_name, reason))
            }

            types::TypeSystemError::MethodNotFound { type_name, method_name, .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::UndefinedSymbol {
                    name: method_name.clone(),
                    symbol_type: SymbolType::Method,
                }, format!("method '{}' not found in type '{}'", method_name, type_name))
            }

            types::TypeSystemError::VariantNotFound { enum_name, variant_name, .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::UndefinedSymbol {
                    name: variant_name.clone(),
                    symbol_type: SymbolType::Constant,
                }, format!("variant '{}' not found in enum '{}'", variant_name, enum_name))
            }

            types::TypeSystemError::LifetimeError { reason, .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::UndefinedSymbol {
                    name: "lifetime".to_string(),
                    symbol_type: SymbolType::Lifetime,
                }, format!("lifetime error: {}", reason))
            }

            types::TypeSystemError::OwnershipError { reason, .. } => {
                Diagnostic::new(Severity::Error, ErrorKind::BorrowViolation {
                    variable: "unknown".to_string(),
                    reason: reason.clone(),
                }, format!("ownership error: {}", reason))
            }
        }
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
            ParseError::InvalidTypeSyntax { type_str, reason, .. } => ErrorKind::InvalidSyntax {
                context: format!("type '{}': {}", type_str, reason),
            },
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
