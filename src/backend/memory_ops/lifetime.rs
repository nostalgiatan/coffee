//! Lifetime diagnostics and variable state for memory operations.

/// Lifetime diagnostic error
/// 
/// Represents an error detected during lifetime analysis of variables in the
/// Coffee compiler. These diagnostics help identify potential memory safety
/// issues such as uninitialized variables, unused variables, moved variables,
/// or use-after-free errors. Each diagnostic includes a code, message, and
/// hint for resolving the issue.
#[derive(Debug, Clone)]
pub struct LifetimeDiagnostic {
    /// Error code for the diagnostic (e.g., M001, M002, etc.)
    pub code: String,
    /// Detailed error message describing the issue
    pub message: String,
    /// Name of the variable involved in the diagnostic
    pub var_name: String,
    /// Suggestion for fixing the issue
    pub hint: String,
}

/// Severity of a lifetime diagnostic under the RAII/auto-drop memory model.
///
/// Historically every M-code was a hard compile error. With automatic
/// scope-exit drop, the "must explicitly `rm`" premise is gone, so most codes
/// are downgraded. Only genuine undefined behavior remains a hard error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagSeverity {
    /// Hard error — compilation fails (use-after-free, use-after-move).
    Error,
    /// Printed as a yellow warning, does not block compilation.
    Warning,
    /// Retired/suppressed — no longer relevant under auto-drop (moved-not-cleaned,
    /// never-removed, leak-rate). Fully silenced.
    Silent,
}

impl LifetimeDiagnostic {
    /// Classify this diagnostic by its M-code under the new memory model.
    ///
    /// - `Error`:   M006 (use-after-free), M007 (use-after-move) — real UB.
    /// - `Silent`:  M003 (moved-not-cleaned), M004 (never-removed),
    ///              M005 (zero-length lifetime; MIR has no per-statement lines),
    ///              M009 (leak-rate) — not user-facing under auto-drop / MIR.
    /// - `Warning`: everything else (M001, M002, M010, M011, M012, M013).
    pub fn severity(&self) -> DiagSeverity {
        match self.code.as_str() {
            "M006" | "M007" => DiagSeverity::Error,
            // M005: MIR does not advance source lines, so birth==use is normal.
            "M003" | "M004" | "M005" | "M009" => DiagSeverity::Silent,
            _ => DiagSeverity::Warning,
        }
    }
}

/// Variable lifetime state
/// 
/// Represents the current state of a variable during its lifetime in the
/// Coffee compiler's memory management system. This enum tracks the
/// ownership and validity of variables to prevent memory safety issues.
#[derive(Debug, Clone, PartialEq)]
pub enum VariableState {
    /// Variable has been declared but not yet initialized
    Uninitialized,
    /// Variable has been initialized and can be used
    Initialized,
    /// Variable's ownership has been transferred (moved)
    Moved,
    /// Variable has been released/freed and is no longer valid
    Dropped,
}

/// Variable lifetime tracking information
/// 
/// Contains comprehensive information about a variable's lifetime in the
/// Coffee compiler. This includes its current state, creation location,
/// usage patterns, and cleanup status. The information is used by the
/// lifetime analysis system to detect potential memory safety issues.
#[derive(Debug, Clone)]
pub struct LifetimeInfo {
    /// Name of the variable
    pub name: String,
    /// Current state of the variable in its lifetime
    pub state: VariableState,
    /// Line number where the variable was created (birth)
    pub birth_line: usize,
    /// Optional line number of the last use of the variable
    pub last_use_line: Option<usize>,
    /// Optional line number where the variable was destroyed (death)
    pub death_line: Option<usize>,
    /// Whether the variable was properly cleaned up before going out of scope
    pub properly_cleaned: bool,
    /// Loop nesting depth when the variable was declared
    pub loop_depth: usize,
    /// Whether the variable is declared in a loop body
    pub is_loop_variable: bool,
    /// Whether the pointer points to heap-allocated memory (needs free)
    pub is_heap_allocated: bool,
}

impl LifetimeInfo {
    /// Create a new LifetimeInfo instance
    /// 
    /// Initializes a LifetimeInfo with the given variable name and birth line.
    /// The new instance starts in the Uninitialized state with no usage or
    /// cleanup information recorded yet.
    /// 
    /// # Arguments
    /// 
    /// * `name` - The name of the variable being tracked
    /// * `birth_line` - The line number where the variable is created
    /// 
    /// # Returns
    /// 
    /// A new LifetimeInfo instance initialized with the provided information
    pub fn new(name: String, birth_line: usize) -> Self {
        Self {
            name,
            state: VariableState::Uninitialized,
            birth_line,
            last_use_line: None,
            death_line: None,
            properly_cleaned: false,
            loop_depth: 0, // Will be set by register_birth
            is_loop_variable: false, // Will be set by register_birth
            is_heap_allocated: false, // Default to stack allocation
        }
    }

    /// Calculate the length of the variable's lifetime
    /// 
    /// Computes the number of lines between the variable's birth and its
    /// last meaningful event (either last use or death). This helps in
    /// analyzing the scope of variables and detecting potential issues
    /// such as unnecessarily long-lived variables.
    /// 
    /// # Returns
    /// 
    /// * `Some(usize)` - The number of lines in the variable's lifetime
    /// * `None` - If the lifetime cannot be calculated (no usage or death recorded)
    pub fn lifetime_length(&self) -> Option<usize> {
        match (self.last_use_line, self.death_line) {
            (Some(_), Some(death_line)) => Some(death_line - self.birth_line),
            (Some(use_line), None) => Some(use_line - self.birth_line),
            (None, Some(death_line)) => Some(death_line - self.birth_line),
            _ => None,
        }
    }
}
