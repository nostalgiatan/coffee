//! Memory operations for Coffee compiler code generation
//! 
//! This module handles memory operations for the Coffee compiler, including
//! clone, copy, move, and remove operations. It provides comprehensive
//! lifetime tracking and memory safety checks to prevent common memory
//! management errors such as use-after-free, double-free, and memory leaks.
//! The module implements Rust-like ownership semantics for the Coffee language,
//! tracking variable lifetimes and ensuring proper resource management.

use crate::parser::MemoryOp;
use inkwell::values::PointerValue;
use inkwell::types::BasicTypeEnum;

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
    ///              M009 (leak-rate) — the old "must `rm`" rules, now handled
    ///              by automatic drop.
    /// - `Warning`: everything else (M001, M002, M005, M010, M011, M012, M013).
    pub fn severity(&self) -> DiagSeverity {
        match self.code.as_str() {
            "M006" | "M007" => DiagSeverity::Error,
            "M003" | "M004" | "M009" => DiagSeverity::Silent,
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

/// Context for memory operations with lifetime tracking
/// 
/// The MemoryContext manages the state of variables during compilation with
/// respect to memory safety and ownership. It tracks variable lifetimes,
/// move semantics, and scope information to prevent memory-related errors
/// such as double-free, use-after-free, and memory leaks. The context
/// maintains information about variable states across different scopes
/// and provides functions to analyze and verify memory safety.
pub struct MemoryContext<'ctx> {
    /// Track variable state for move semantics
    pub moved_variables: std::collections::HashSet<String>,
    /// Track dropped/freed variables for double-free detection
    pub dropped_variables: std::collections::HashSet<String>,
    /// Variable lifetime tracking information
    pub lifetimes: std::collections::HashMap<String, LifetimeInfo>,
    /// Name of the current function being processed
    pub current_function: String,
    /// Current line number in the source code
    pub current_line: usize,
    /// Scope stack: tracks variable scope levels
    scope_stack: Vec<usize>,
    /// Current scope ID
    current_scope: usize,
    /// Mapping of variables to their scopes
    variable_scopes: std::collections::HashMap<String, usize>,
    /// Track clean operations: scope -> line numbers
    clean_operations: std::collections::HashMap<usize, Vec<usize>>,
    /// Loop nesting depth for checking variable redeclaration in loops
    pub loop_nesting_depth: usize,
    /// Mapping of variables to their type names (for drop function calls)
    pub variable_types: std::collections::HashMap<String, String>,
    _phantom: std::marker::PhantomData<&'ctx ()>,
}

impl<'ctx> MemoryContext<'ctx> {
    /// Create a new MemoryContext instance
    /// 
    /// Initializes a MemoryContext with default values for tracking variable
    /// lifetimes and memory operations. The context starts with an empty
    /// global scope and no tracked variables.
    /// 
    /// # Returns
    /// 
    /// A new MemoryContext instance with default settings
    pub fn new() -> Self {
        Self {
            moved_variables: std::collections::HashSet::new(),
            dropped_variables: std::collections::HashSet::new(),
            lifetimes: std::collections::HashMap::new(),
            current_function: String::new(),
            current_line: 0,
            scope_stack: vec![0], // Global scope ID is 0
            current_scope: 0,
            variable_scopes: std::collections::HashMap::new(),
            clean_operations: std::collections::HashMap::new(),
            loop_nesting_depth: 0,
            variable_types: std::collections::HashMap::new(),
            _phantom: std::marker::PhantomData,
        }
    }

    /// Enter a new scope
    /// 
    /// Creates a new scope level in the memory context by incrementing the
    /// current scope ID and pushing it onto the scope stack. This is used
    /// when entering blocks, functions, or other constructs that create a
    /// new variable scope in the Coffee language.
    pub fn enter_scope(&mut self) {
        self.current_scope += 1;
        self.scope_stack.push(self.current_scope);
    }

    /// Clear all variable information
    /// 
    /// Clears all variable lifetime information, moved variables, dropped variables,
    /// and scope information. This is used when starting a new function to ensure that
    /// each function has its own scope and can use the same variable names.
    pub fn clear_all(&mut self) {
        self.lifetimes.clear();
        self.moved_variables.clear();
        self.dropped_variables.clear();
        self.variable_scopes.clear();
        self.clean_operations.clear();
        self.variable_types.clear();
        self.current_scope = 0;
        self.scope_stack = vec![0]; // Reset to global scope
    }

    /// Set the type of a variable
    /// 
    /// Associates a type name with a variable for use in drop function calls.
    /// This is used to determine if a variable is a class instance and what
    /// drop function to call when it is removed.
    /// 
    /// # Arguments
    /// 
    /// * `name` - The name of the variable
    /// * `type_name` - The type name of the variable (e.g., "Point", "List")
    pub fn set_variable_type(&mut self, name: String, type_name: String) {
        self.variable_types.insert(name, type_name);
    }

    /// Get the type of a variable
    /// 
    /// Returns the type name associated with a variable, if any.
    /// 
    /// # Arguments
    /// 
    /// * `name` - The name of the variable
    /// 
    /// # Returns
    /// 
    /// * `Some(type_name)` - If the variable has an associated type
    /// * `None` - If the variable has no associated type
    pub fn get_variable_type(&self, name: &str) -> Option<&String> {
        self.variable_types.get(name)
    }

    /// Exit current scope
    /// 
    /// Exits the current scope by popping it from the scope stack and setting
    /// the current scope to the previous one. This is used when exiting blocks,
    /// functions, or other constructs that end a variable scope. It ensures
    /// proper cleanup of scope-specific information.
    pub fn exit_scope(&mut self) {
        if self.scope_stack.len() > 1 {
            let exiting_scope = self.current_scope;
            self.scope_stack.pop();
            self.current_scope = *self.scope_stack.last().unwrap_or(&0);
            
            // Auto-cleanup loop variables when exiting a loop scope
            // This ensures variables declared in loops are properly cleaned up
            let vars_to_remove: Vec<String> = self.lifetimes.iter()
                .filter(|(name, info)| {
                    // Check if variable belongs to the exiting scope and is a loop variable
                    self.variable_scopes.get(*name) == Some(&exiting_scope) && info.is_loop_variable
                })
                .map(|(name, _)| name.clone())
                .collect();
            
            for var_name in vars_to_remove {
                // Mark loop variables as properly cleaned
                if let Some(info) = self.lifetimes.get_mut(&var_name) {
                    info.properly_cleaned = true;
                    info.state = VariableState::Moved; // Mark as moved to prevent use-after-free
                }
            }

            // CRITICAL FIX: Clear dropped_variables for variables in the exiting scope
            // This allows variables to be redeclared in new scopes (e.g., in loops)
            let vars_to_undrop: Vec<String> = self.dropped_variables.iter()
                .filter(|name| {
                    // Check if variable belongs to the exiting scope
                    self.variable_scopes.get(*name) == Some(&exiting_scope)
                })
                .cloned()
                .collect();
            
            for var_name in vars_to_undrop {
                self.dropped_variables.remove(&var_name);
            }
        }
    }

    /// Register a variable in the current scope
    /// 
    /// Associates a variable name with the current scope ID in the variable
    /// scope mapping. This allows the memory context to track which variables
    /// belong to which scopes, enabling proper scope-based memory management
    /// and cleanup operations.
    /// 
    /// # Arguments
    /// 
    /// * `var_name` - The name of the variable to register in the current scope
    pub fn register_variable(&mut self, var_name: String) {
        self.variable_scopes.insert(var_name, self.current_scope);
    }

    /// Check if a variable is in the current scope
    /// 
    /// Determines whether a given variable name is associated with the current
    /// scope. This is used to validate variable access and ensure that
    /// operations are performed only on variables that are currently in scope.
    /// 
    /// # Arguments
    /// 
    /// * `var_name` - The name of the variable to check
    /// 
    /// # Returns
    /// 
    /// * `true` - If the variable is in the current scope
    /// * `false` - If the variable is not in the current scope
    pub fn is_in_current_scope(&self, var_name: &str) -> bool {
        self.variable_scopes.get(var_name) == Some(&self.current_scope)
    }

    /// Get all variables in the current scope
    /// 
    /// Returns a list of variable names that are in the current scope and have
    /// not been dropped yet. This is used for operations like 'clean out' that
    /// need to identify all variables in a scope for cleanup.
    /// 
    /// # Arguments
    /// 
    /// * `all_vars` - A slice of all variable names to check against the current scope
    /// 
    /// # Returns
    /// 
    /// A vector of variable names that are in the current scope and not dropped
    pub fn get_current_scope_variables(&self, all_vars: &[String]) -> Vec<String> {
        all_vars.iter()
            .filter(|v| self.is_in_current_scope(v))
            .filter(|v| !self.is_dropped(v))  // Exclude already dropped variables
            .cloned()
            .collect()
    }

    /// Check if a variable has been moved
    /// 
    /// Determines whether a variable's ownership has been transferred (moved)
    /// according to the Coffee language's ownership system. Moved variables
    /// should not be accessed again without explicit re-initialization.
    /// 
    /// # Arguments
    /// 
    /// * `var_name` - The name of the variable to check
    /// 
    /// # Returns
    /// 
    /// * `true` - If the variable has been moved
    /// * `false` - If the variable has not been moved
    #[allow(dead_code)]
    pub fn is_moved(&self, var_name: &str) -> bool {
        self.moved_variables.contains(var_name)
    }

    /// Check if a variable has been dropped
    /// 
    /// Determines whether a variable has been explicitly released or freed
    /// (dropped) in the Coffee language. Dropped variables are no longer valid
    /// and should not be accessed to prevent use-after-free errors.
    /// 
    /// # Arguments
    /// 
    /// * `var_name` - The name of the variable to check
    /// 
    /// # Returns
    /// 
    /// * `true` - If the variable has been dropped
    /// * `false` - If the variable has not been dropped
    pub fn is_dropped(&self, var_name: &str) -> bool {
        self.dropped_variables.contains(var_name)
    }

    /// Mark a variable as moved
    /// 
    /// Updates the memory context to indicate that a variable's ownership has
    /// been transferred. This marks the variable as moved in the moved variables
    /// set and updates its lifetime information to reflect the move operation.
    /// After a variable is moved, it should not be accessed without re-initialization.
    /// 
    /// # Arguments
    /// 
    /// * `var_name` - The name of the variable that has been moved
    pub fn mark_moved(&mut self, var_name: String) {
        self.moved_variables.insert(var_name.clone());
        if let Some(info) = self.lifetimes.get_mut(&var_name) {
            info.state = VariableState::Moved;
            info.last_use_line = Some(self.current_line);
            info.properly_cleaned = true; // mv操作自动清理源变量
        }
    }

    /// Mark a variable as dropped/freed
    /// 
    /// Updates the memory context to indicate that a variable has been explicitly
    /// released or freed. This marks the variable as dropped in the dropped
    /// variables set and updates its lifetime information to reflect the cleanup.
    /// After a variable is dropped, it should not be accessed to prevent
    /// use-after-free errors.
    /// 
    /// # Arguments
    /// 
    /// * `var_name` - The name of the variable that has been dropped
    pub fn mark_dropped(&mut self, var_name: String) {
        self.dropped_variables.insert(var_name.clone());
        if let Some(info) = self.lifetimes.get_mut(&var_name) {
            info.state = VariableState::Dropped;
            info.death_line = Some(self.current_line);
            info.properly_cleaned = true;
        }
    }

    /// Mark a pointer variable as heap-allocated
    /// 
    /// Updates the lifetime information to indicate that a pointer variable
    /// points to heap-allocated memory (e.g., created with malloc). This
    /// information is used during cleanup to determine whether to call free()
    /// on the pointer when the variable is removed.
    /// 
    /// # Arguments
    /// 
    /// * `var_name` - The name of the pointer variable
    pub fn mark_heap_allocated(&mut self, var_name: &str) {
        if let Some(info) = self.lifetimes.get_mut(var_name) {
            info.is_heap_allocated = true;
        }
    }

    /// Unmark a variable as moved (for reassignment)
    /// 
    /// Updates the memory context to indicate that a moved variable has been
    /// reassigned and is no longer in a moved state. This is used when a
    /// variable that was previously moved gets a new value assigned to it,
    /// restoring its usability. The variable's state is updated to Initialized.
    /// 
    /// # Arguments
    /// 
    /// * `var_name` - The name of the variable to unmark as moved
    #[allow(dead_code)]
    pub fn unmark_moved(&mut self, var_name: &str) {
        self.moved_variables.remove(var_name);
        if let Some(info) = self.lifetimes.get_mut(var_name) {
            info.state = VariableState::Initialized;
        }
    }

    /// Set the current function
    /// 
    /// Initializes the memory context for a new function by clearing all
    /// tracking information and setting the current function name. This
    /// prepares the context for analyzing variable lifetimes and memory
    /// operations within the specified function.
    /// 
    /// # Arguments
    /// 
    /// * `func_name` - The name of the function to set as current
    pub fn set_function(&mut self, func_name: String) {
        self.current_function = func_name;
        self.lifetimes.clear();
        self.moved_variables.clear();
        self.dropped_variables.clear();
        self.scope_stack = vec![0]; // Reset scope stack
        self.current_scope = 0;
        self.variable_scopes.clear();
        self.clean_operations.clear();
    }

    /// Set the current line number
    /// 
    /// Updates the current line number in the memory context. This is used
    /// for tracking where operations occur in the source code for diagnostic
    /// and lifetime analysis purposes.
    /// 
    /// # Arguments
    /// 
    /// * `line` - The line number to set as current
    pub fn set_line(&mut self, line: usize) {
        self.current_line = line;
    }

    /// Record a clean operation
    /// 
    /// Records when a 'clean out' operation occurs in the current scope at
    /// the current line. This information is used during lifetime analysis
    /// to detect potential issues with cleaning operations, such as cleaning
    /// an empty scope or consecutive cleaning operations.
    pub fn record_clean(&mut self) {
        self.clean_operations
            .entry(self.current_scope)
            .or_insert_with(Vec::new)
            .push(self.current_line);
    }

    /// Register variable birth
    /// 
    /// Records the creation of a new variable in the memory context by
    /// creating and storing its initial lifetime information. This establishes
    /// the variable's birth line and initial state, and also registers it
    /// in the current scope for proper scope-based memory management.
    /// 
    /// # Arguments
    /// 
    /// * `var_name` - The name of the variable being born
    pub fn register_birth(&mut self, var_name: String) {
        // Check if variable already exists
        if let Some(info) = self.lifetimes.get_mut(&var_name) {
            // If variable exists and is in Moved state, reset it for reuse
            // This allows variables to be redeclared in loops
            if info.state == VariableState::Moved {
                info.state = VariableState::Uninitialized;
                info.last_use_line = None;
                info.properly_cleaned = false;
            }
            return;
        }
        
        // Variable doesn't exist, create new entry
        let mut info = LifetimeInfo::new(var_name.clone(), self.current_line);
        info.loop_depth = self.loop_nesting_depth; // Record loop depth at declaration
        info.is_loop_variable = self.loop_nesting_depth > 0; // Mark if declared in loop
        self.lifetimes.insert(var_name.clone(), info);
        // Also register in the current scope
        self.register_variable(var_name);
    }

    /// Mark variable as initialized
    /// 
    /// Updates the state of a variable to indicate that it has been initialized
    /// and is ready for use. This transition from Uninitialized to Initialized
    /// state is important for lifetime analysis to ensure that variables are
    /// properly initialized before use.
    /// 
    /// # Arguments
    /// 
    /// * `var_name` - The name of the variable to mark as initialized
    pub fn mark_initialized(&mut self, var_name: &str) {
        if let Some(info) = self.lifetimes.get_mut(var_name) {
            info.state = VariableState::Initialized;
        }
    }

    /// Record variable use
    /// 
    /// Updates the lifetime information for a variable to record that it was
    /// used at the current line. This information is crucial for lifetime
    /// analysis, helping to determine when variables are last used and
    /// whether they are properly cleaned up after their final use.
    /// 
    /// # Arguments
    /// 
    /// * `var_name` - The name of the variable being used
    pub fn record_use(&mut self, var_name: &str) {
        if let Some(info) = self.lifetimes.get_mut(var_name) {
            info.last_use_line = Some(self.current_line);
        }
    }

    /// Run lifetime checks
    /// 
    /// Performs comprehensive lifetime analysis on all tracked variables to
    /// detect potential memory safety issues. This includes checking for:
    /// - Uninitialized variables
    /// - Unused variables
    /// - Moved but not cleaned up variables
    /// - Variables with improper lifetimes
    /// - Use after free or move
    /// - Potential memory leaks
    /// 
    /// The method follows a "better to reject a safe program than to accept
    /// an unsafe one" philosophy, potentially generating warnings for edge cases.
    /// 
    /// Returns discovered errors (following 'better to reject a safe program than let an unsafe one pass' principle)
    /// 
    /// # Returns
    /// 
    /// * `Ok(Vec<LifetimeDiagnostic>)` - A list of detected lifetime issues
    /// * `Err(String)` - If there was an error during analysis
    pub fn run_lifetime_checks(&mut self) -> Result<Vec<LifetimeDiagnostic>, String> {
        let mut diagnostics = Vec::new();

        for (name, info) in &self.lifetimes {
            // 验证：info.name 应该与键名一致
            debug_assert_eq!(name, &info.name, "LifetimeInfo name mismatch");

            // M001: 检查未初始化就死亡的变量
            if info.state == VariableState::Uninitialized {
                diagnostics.push(LifetimeDiagnostic {
                    code: "M001".to_string(),
                    message: format!(
                        "variable '{}' declared but never initialized",
                        name
                    ),
                    var_name: name.clone(),
                    hint: format!(
                        "initialize variable '{}' or remove it if not needed",
                        name
                    ),
                });
            }

            // M002: 检查初始化但从未使用的变量
            // Skip this check for loop variables, as they are used in each iteration
            if info.state == VariableState::Initialized && info.last_use_line.is_none() && !info.is_loop_variable {
                diagnostics.push(LifetimeDiagnostic {
                    code: "M002".to_string(),
                    message: format!(
                        "variable '{}' initialized but never used",
                        name
                    ),
                    var_name: name.clone(),
                    hint: format!(
                        "use variable '{}' or remove it if not needed - Coffee requires explicit resource management",
                        name
                    ),
                });
            }

            // M003: 检查已移动但未清理的变量
            if info.state == VariableState::Moved && !info.properly_cleaned {
                diagnostics.push(LifetimeDiagnostic {
                    code: "M003".to_string(),
                    message: format!(
                        "variable '{}' was moved but not properly cleaned up",
                        name
                    ),
                    var_name: name.clone(),
                    hint: format!(
                        "after moving '{}', add 'rm {}' to properly clean up the original variable - moved variables must be explicitly released",
                        name, name
                    ),
                });
                
            }

            // M004: 检查已初始化但未清理的变量（潜在资源泄漏）
            // 如果变量被使用过（last_use_line被设置），就不报M004错误
            // Skip this check for loop variables, as they are automatically cleaned at loop end
            let was_used = info.last_use_line.is_some();
            if info.state == VariableState::Initialized && !info.properly_cleaned && !was_used && !info.is_loop_variable {
                diagnostics.push(LifetimeDiagnostic {
                    code: "M004".to_string(),
                    message: format!(
                        "variable '{}' was initialized but never explicitly removed",
                        name
                    ),
                    var_name: name.clone(),
                    hint: format!(
                        "add 'rm {}' before function end to explicitly release resources - Coffee requires explicit cleanup, no implicit destructors",
                        name
                    ),
                });
            }

            // M005: 检查变量生命周期异常短（可能误用）
            if let Some(length) = info.lifetime_length() {
                if length == 0 && info.state != VariableState::Dropped {
                    diagnostics.push(LifetimeDiagnostic {
                        code: "M005".to_string(),
                        message: format!(
                            "variable '{}' has zero-length lifetime (born and died at same line)",
                            name
                        ),
                        var_name: name.clone(),
                        hint: format!(
                            "this may indicate incorrect memory management - verify that '{}' is used correctly",
                            name
                        ),
                    });
                    
                }
            }

            // M006: 检查已释放的变量（双重释放已经在compile_remove中检查）
            if info.state == VariableState::Dropped && info.last_use_line.is_some() {
                // 如果在释放后还有使用记录，这是严重错误
                if let Some(death_line) = info.death_line {
                    if let Some(last_use) = info.last_use_line {
                        if last_use > death_line {
                            diagnostics.push(LifetimeDiagnostic {
                                code: "M006".to_string(),
                                message: format!(
                                    "variable '{}' was used after being freed",
                                    name
                                ),
                                var_name: name.clone(),
                                hint: format!(
                                    "use-after-free detected at line {} - this is undefined behavior, prevented by Coffee",
                                    last_use
                                ),
                            });
                            
                        }
                    }
                }
            }

            // M007: 检查移动后使用（use-after-move）
                    // Skip this check if the variable was declared in a loop
                    // This allows variables to be redeclared in loops
                    // A variable is considered redeclared if it was declared in a loop and moved
                    let is_redeclared_in_loop = info.loop_depth > 0 && 
                        info.last_use_line.is_some() && 
                        info.birth_line < self.current_line &&
                        info.last_use_line.unwrap() < self.current_line;
                    
                    if info.state == VariableState::Moved && !is_redeclared_in_loop {
                        if let Some(last_use) = info.last_use_line {
                            diagnostics.push(LifetimeDiagnostic {
                                code: "M007".to_string(),
                                message: format!(
                                    "variable '{}' was used after being moved",
                                    name
                                ),
                                var_name: name.clone(),
                                hint: format!(
                                    "use-after-move detected at line {:?} - this is undefined behavior, prevented by Coffee",
                                    last_use
                                ),
                            });
                            
                        }
                    }        }

        // NOTE: The old leak-rate check (M009) and the "must explicitly `rm`"
        // premise are retired under the RAII/auto-drop memory model — variables
        // are dropped automatically at scope exit, so a leak rate is no longer a
        // meaningful metric. See `LifetimeDiagnostic::severity`.

                    // M013: 检查未使用的函数参数（原 M009 重复编号，已重命名以消除歧义）
                    for (name, info) in &self.lifetimes {
                        if info.state == VariableState::Initialized && info.last_use_line.is_none() {
                            // 可能是函数参数
                            diagnostics.push(LifetimeDiagnostic {
                                code: "M013".to_string(),
                                message: format!(
                                    "variable '{}' (possibly a parameter) is declared but never used",
                                    name
                                ),
                                var_name: name.clone(),
                                hint: format!(
                                    "use '{}' or prefix with '_' to indicate intentionally unused: '_{}'",
                                    name, name
                                ),
                            });

                        }
                    }
        // M010: 检查clean out是否在作用域顶部（无意义行为）
        for (&scope, clean_lines) in &self.clean_operations {
            for &clean_line in clean_lines {
                // 找到该作用域中最早创建的变量
                let mut earliest_birth = None;
                for (name, info) in &self.lifetimes {
                    if self.variable_scopes.get(name) == Some(&scope) {
                        if earliest_birth.is_none() || Some(info.birth_line) < earliest_birth {
                            earliest_birth = Some(info.birth_line);
                        }
                    }
                }

                // 如果clean在所有变量创建之前，这是无意义的
                if let Some(birth) = earliest_birth {
                    if clean_line < birth {
                        diagnostics.push(LifetimeDiagnostic {
                            code: "M010".to_string(),
                            message: format!(
                                "clean out at line {} appears before any variables are created in scope {}",
                                clean_line, scope
                            ),
                            var_name: format!("scope_{}", scope),
                            hint: format!(
                                "move 'clean out' after variable declarations - cleaning an empty scope is meaningless",
                            ),
                        });
                        
                    }
                }
            }

            // M012: 检查连续的clean操作（无意义）
            if clean_lines.len() > 1 {
                for i in 0..clean_lines.len() - 1 {
                    if clean_lines[i + 1] == clean_lines[i] + 1 {
                        diagnostics.push(LifetimeDiagnostic {
                            code: "M012".to_string(),
                            message: format!(
                                "consecutive clean operations at lines {} and {} in scope {}",
                                clean_lines[i], clean_lines[i + 1], scope
                            ),
                            var_name: format!("scope_{}", scope),
                            hint: format!(
                                "remove one of the consecutive 'clean out' statements - cleaning twice in a row is meaningless",
                            ),
                        });
                        
                        break; // Only report once per scope
                    }
                }
            }
        }

        // M011: 检查变量遮蔽（shadowing）
        // 如果有同名变量在作用域中，发出警告
        let mut var_names: Vec<&String> = self.lifetimes.keys().collect();
        var_names.sort();
        for i in 0..var_names.len() {
            for j in (i+1)..var_names.len() {
                if var_names[i] == var_names[j] {
                    diagnostics.push(LifetimeDiagnostic {
                        code: "M011".to_string(),
                        message: format!(
                            "variable '{}' is declared multiple times (variable shadowing)",
                            var_names[i]
                        ),
                        var_name: var_names[i].clone(),
                        hint: format!(
                            "rename one of the variables - Coffee discourages shadowing to prevent confusion",
                        ),
                    });
                }
            }
        }

        Ok(diagnostics)
    }
}

impl<'ctx> Default for MemoryContext<'ctx> {
    fn default() -> Self {
        Self::new()
    }
}

/// Compile a clone operation (deep copy)
/// 
/// Generates LLVM IR for a clone operation, which creates a deep copy of
/// a variable's value. The clone operation duplicates the value from the
/// source variable to the target variable, ensuring that both variables
/// have independent copies of the data.
/// 
/// In Coffee's ownership system, cloning creates a new, independent copy
/// of the data, allowing both the source and target to be used independently
/// without affecting each other.
/// 
/// # Arguments
/// 
/// * `_context` - The LLVM context (not used directly in this implementation)
/// * `builder` - The LLVM builder to use for instruction generation
/// * `variables` - The map of variable names to their LLVM values and types
/// * `source` - The name of the source variable to clone from
/// * `target` - The name of the target variable to clone to
/// 
/// # Returns
/// 
/// * `Ok(())` - If the clone operation was compiled successfully
/// * `Err(String)` - If there was an error during compilation
pub fn compile_clone<'ctx>(
    _context: &'ctx inkwell::context::Context,
    builder: &inkwell::builder::Builder<'ctx>,
    variables: &mut std::collections::HashMap<String, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)>,
    source: &str,
    target: &str,
) -> Result<(), String> {
    if let Some(&(src_ptr, src_type)) = variables.get(source) {
        let value = builder.build_load(src_type, src_ptr, "clone_val")
            .map_err(|e| format!("memory clone: failed to load from '{}': {}", source, e))?;

        let dst_ptr = if let Some(&(existing, _)) = variables.get(target) {
            existing
        } else {
            let alloca = builder.build_alloca(src_type, target)
                .map_err(|e| format!("memory clone: failed to allocate '{}': {}", target, e))?;
            variables.insert(target.to_string(), (alloca, src_type));
            alloca
        };

        builder.build_store(dst_ptr, value)
            .map_err(|e| format!("memory clone: failed to store to '{}': {}", target, e))?;
    } else {
        return Err(format!("memory clone: source variable '{}' not found", source));
    }
    Ok(())
}

/// Compile a copy operation (shallow copy)
/// 
/// Generates LLVM IR for a copy operation, which creates a shallow copy of
/// a variable's value. The copy operation duplicates the value from the
/// source variable to the target variable.
/// 
/// In Coffee's ownership system, copying creates another reference to the
/// same data without transferring ownership. The behavior is similar to
/// cloning for most types but may differ for types with special semantics.
/// 
/// # Arguments
/// 
/// * `_context` - The LLVM context (not used directly in this implementation)
/// * `builder` - The LLVM builder to use for instruction generation
/// * `variables` - The map of variable names to their LLVM values and types
/// * `source` - The name of the source variable to copy from
/// * `target` - The name of the target variable to copy to
/// 
/// # Returns
/// 
/// * `Ok(())` - If the copy operation was compiled successfully
/// * `Err(String)` - If there was an error during compilation
pub fn compile_copy<'ctx>(
    _context: &'ctx inkwell::context::Context,
    builder: &inkwell::builder::Builder<'ctx>,
    variables: &mut std::collections::HashMap<String, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)>,
    source: &str,
    target: &str,
) -> Result<(), String> {
    if let Some(&(src_ptr, src_type)) = variables.get(source) {
        let value = builder.build_load(src_type, src_ptr, "copy_val")
            .map_err(|e| format!("memory copy: failed to load from '{}': {}", source, e))?;

        let dst_ptr = if let Some(&(existing, _)) = variables.get(target) {
            existing
        } else {
            let alloca = builder.build_alloca(src_type, target)
                .map_err(|e| format!("memory copy: failed to allocate '{}': {}", target, e))?;
            variables.insert(target.to_string(), (alloca, src_type));
            alloca
        };

        builder.build_store(dst_ptr, value)
            .map_err(|e| format!("memory copy: failed to store to '{}': {}", target, e))?;
    } else {
        return Err(format!("memory copy: source variable '{}' not found", source));
    }
    Ok(())
}

/// Compile a move operation (transfer ownership)
/// 
/// Generates LLVM IR for a move operation, which transfers ownership of
/// a variable's value from the source to the target. The move operation
/// invalidates the source variable after the transfer, preventing further
/// use without re-initialization.
/// 
/// In Coffee's ownership system, moving transfers ownership from one variable
/// to another, ensuring that there is only one owner of any data at a time.
/// After a move, the source variable is zeroed out to prevent information
/// leaks and marked as moved in the memory context.
/// 
/// # Arguments
/// 
/// * `_context` - The LLVM context (not used directly in this implementation)
/// * `builder` - The LLVM builder to use for instruction generation
/// * `variables` - The map of variable names to their LLVM values and types
/// * `memory_ctx` - The memory context to track ownership changes
/// * `source` - The name of the source variable to move from
/// * `target` - The name of the target variable to move to
/// 
/// # Returns
/// 
/// * `Ok(())` - If the move operation was compiled successfully
/// * `Err(String)` - If there was an error during compilation
pub fn compile_move<'ctx>(
    _context: &'ctx inkwell::context::Context,
    builder: &inkwell::builder::Builder<'ctx>,
    variables: &mut std::collections::HashMap<String, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)>,
    memory_ctx: &mut MemoryContext<'ctx>,
    source: &str,
    target: &str,
) -> Result<(), String> {
    if let Some(&(src_ptr, src_type)) = variables.get(source) {
        let value = builder.build_load(src_type, src_ptr, "move_val")
            .map_err(|e| format!("memory move: failed to load from '{}': {}", source, e))?;

        let dst_ptr = if let Some(&(existing, _)) = variables.get(target) {
            existing
        } else {
            let alloca = builder.build_alloca(src_type, target)
                .map_err(|e| format!("memory move: failed to allocate '{}': {}", target, e))?;
            variables.insert(target.to_string(), (alloca, src_type));
            alloca
        };

        builder.build_store(dst_ptr, value)
            .map_err(|e| format!("memory move: failed to store to '{}': {}", target, e))?;

        // CRITICAL-9 FIX: Zero the source after move to prevent information leaks
        // Store zeros to invalidate the source data
        
        match src_type {
            BasicTypeEnum::IntType(t) => {
                let zero = t.const_zero();
                builder.build_store(src_ptr, zero)
                    .map_err(|e| format!("memory move: failed to invalidate '{}': {}", source, e))?;
            }
            BasicTypeEnum::FloatType(t) => {
                let zero = t.const_zero();
                builder.build_store(src_ptr, zero)
                    .map_err(|e| format!("memory move: failed to invalidate '{}': {}", source, e))?;
            }
            BasicTypeEnum::PointerType(t) => {
                let zero = t.const_zero();
                builder.build_store(src_ptr, zero)
                    .map_err(|e| format!("memory move: failed to invalidate '{}': {}", source, e))?;
            }
            _ => {
                // For other types, try to zero them generically
                // This is a best-effort approach
            }
        }

        // Mark source as moved
        memory_ctx.mark_moved(source.to_string());
    } else {
        return Err(format!("memory move: source variable '{}' not found", source));
    }
    Ok(())
}

/// Compile a remove operation (deallocate/invalidate)
/// 
/// Generates LLVM IR for a remove operation, which invalidates a variable
/// and prevents its further use. The remove operation zeros out the variable's
/// value to prevent information leaks and marks it as dropped in the memory
/// context to prevent double-free errors.
/// 
/// In Coffee's explicit resource management system, variables must be
/// explicitly removed when no longer needed. This operation handles the
/// safe invalidation of stack-allocated variables. For heap-allocated
/// objects, explicit free() calls should be used before remove.
/// 
/// # Arguments
/// 
/// * `_context` - The LLVM context (not used directly in this implementation)
/// * `builder` - The LLVM builder to use for instruction generation
/// * `variables` - The map of variable names to their LLVM values and types
/// * `memory_ctx` - The memory context to track the removal
/// * `target` - The name of the variable to remove
/// 
/// # Returns
/// 
/// * `Ok(())` - If the remove operation was compiled successfully
/// * `Err(String)` - If there was an error during compilation (e.g., double-free)
pub fn compile_remove<'ctx>(
    context: &'ctx inkwell::context::Context,
    builder: &inkwell::builder::Builder<'ctx>,
    variables: &mut std::collections::HashMap<String, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)>,
    memory_ctx: &mut MemoryContext<'ctx>,
    functions: &std::collections::HashMap<String, inkwell::values::FunctionValue<'ctx>>,
    target: &str,
) -> Result<(), String> {
    // Check for double-free
    if memory_ctx.is_dropped(target) {
        return Err(format!("memory remove: double-free detected - variable '{}' was already removed", target));
    }

    if let Some(&(ptr, var_type)) = variables.get(target) {
        // Check if this is a class instance (struct type)
        let is_class = matches!(var_type, BasicTypeEnum::StructType(_));
        
        // For class instances, call the drop function first
        if is_class {
            // Try to get the drop function: ClassName__drop
            let class_name = memory_ctx.get_variable_type(target);
            if let Some(class_name) = class_name {
                let drop_func_name = format!("{}__drop", class_name);
                if let Some(&drop_func) = functions.get(&drop_func_name) {
                    // Call the drop function with self pointer
                    builder.build_call(drop_func, &[ptr.into()], &format!("{}_drop_call", target))
                        .map_err(|e| format!("memory remove: failed to call drop function: {}", e))?;
                } else {
                    // Drop function not found - this is an error for full classes
                    return Err(format!("memory remove: drop function '{}' not found for class instance '{}'\n  = note: this indicates the class was not properly initialized or the drop function was not generated\n  = help: ensure the class has a constructor (fn new()) defined", drop_func_name, target));
                }
            }
        }
        
        // For pointer types, free the heap memory before invalidating
        if let BasicTypeEnum::PointerType(_) = var_type {
            // Only free if the pointer is heap-allocated
            let should_free = memory_ctx.lifetimes.get(target)
                .map(|info| info.is_heap_allocated)
                .unwrap_or(false);
            
            if should_free {
                // Load the pointer value
                let loaded_ptr = builder.build_load(var_type, ptr, &format!("{}_load", target))
                    .map_err(|e| format!("memory remove: failed to load pointer '{}': {}", target, e))?;
                
                // Only free if free function is available (means user used malloc)
                if let Some(&free_fn) = functions.get("free") {
                    // Cast pointer to i8* for free
                    let i8_ptr_type = context.ptr_type(inkwell::AddressSpace::default());
                    let casted_ptr = builder.build_bit_cast(
                        loaded_ptr.into_pointer_value(),
                        i8_ptr_type,
                        &format!("{}_cast", target)
                    ).map_err(|e| format!("memory remove: failed to cast pointer: {}", e))?;
                    
                    // Call free
                    builder.build_call(free_fn, &[casted_ptr.into()], &format!("{}_free_call", target))
                        .map_err(|e| format!("memory remove: failed to call free: {}", e))?;
                }
            }
        }
        
        // CRITICAL-9 FIX: Zero the allocation to prevent information leaks
        match var_type {
            BasicTypeEnum::IntType(t) => {
                let zero = t.const_zero();
                builder.build_store(ptr, zero)
                    .map_err(|e| format!("memory remove: failed to invalidate '{}': {}", target, e))?;
            }
            BasicTypeEnum::FloatType(t) => {
                let zero = t.const_zero();
                builder.build_store(ptr, zero)
                    .map_err(|e| format!("memory remove: failed to invalidate '{}': {}", target, e))?;
            }
            BasicTypeEnum::PointerType(t) => {
                let zero = t.const_zero();
                builder.build_store(ptr, zero)
                    .map_err(|e| format!("memory remove: failed to invalidate '{}': {}", target, e))?;
            }
            BasicTypeEnum::StructType(_) => {
                // For structs, we've already called the drop function if available
                // Just mark as dropped
            }
            _ => {
                // For other types, best effort
            }
        }

        // Mark as dropped to prevent double-free
        memory_ctx.mark_dropped(target.to_string());
        
        // Remove from variables map
        variables.remove(target);
    }
    Ok(())
}

/// Compile any memory operation
/// 
/// Dispatches and compiles the appropriate memory operation based on the
/// provided MemoryOp enum. This function serves as a central entry point
/// for compiling all types of memory operations in the Coffee language,
/// including clone, copy, move, remove, and clean operations.
/// 
/// The function handles the execution of the specific memory operation
/// and updates the memory context accordingly, ensuring proper lifetime
/// tracking and ownership management.
/// 
/// # Arguments
/// 
/// * `op` - The memory operation to compile
/// * `context` - The LLVM context
/// * `builder` - The LLVM builder to use for instruction generation
/// * `variables` - The map of variable names to their LLVM values and types
/// * `memory_ctx` - The memory context to track the operation's effects
/// 
/// # Returns
/// 
/// * `Ok(())` - If the memory operation was compiled successfully
/// * `Err(String)` - If there was an error during compilation
pub fn compile_memory_op<'ctx>(
    op: &MemoryOp,
    context: &'ctx inkwell::context::Context,
    builder: &inkwell::builder::Builder<'ctx>,
    variables: &mut std::collections::HashMap<String, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)>,
    memory_ctx: &mut MemoryContext<'ctx>,
    functions: &std::collections::HashMap<String, inkwell::values::FunctionValue<'ctx>>,
) -> Result<(), String> {
    match op {
        MemoryOp::Clone { source, target } => {
            compile_clone(context, builder, variables, source, target)
        }
        MemoryOp::Copy { source, target } => {
            compile_copy(context, builder, variables, source, target)
        }
        MemoryOp::Move { source, target } => {
            compile_move(context, builder, variables, memory_ctx, source, target)
        }
        MemoryOp::Remove { target } => {
            compile_remove(context, builder, variables, memory_ctx, functions, target)
        }
        MemoryOp::RemoveMultiple { targets } => {
            compile_remove_multiple(context, builder, variables, memory_ctx, functions, targets)
        }
        MemoryOp::CleanOut { targets, except_mode } => {
            // 记录clean操作
            memory_ctx.record_clean();

            // clean out 是作用域感知的批量rm
            // 只清理当前作用域内的变量
            let all_vars: Vec<String> = variables.keys().cloned().collect();

            // 获取当前作用域的所有变量
            let current_scope_vars = memory_ctx.get_current_scope_variables(&all_vars);

            let to_remove: Vec<String> = if *except_mode {
                // clean out except x, y: 清理当前作用域除x,y外的所有变量
                if let Some(except_list) = targets {
                    current_scope_vars.into_iter()
                        .filter(|v| !except_list.contains(v))
                        .collect()
                } else {
                    current_scope_vars
                }
            } else {
                // clean out x, y, z: 只清理指定的变量（必须在当前作用域）
                if let Some(target_list) = targets {
                    current_scope_vars.into_iter()
                        .filter(|v| target_list.contains(v))
                        .collect()
                } else {
                    // clean out (无参数): 清理当前作用域所有变量
                    current_scope_vars
                }
            };

            compile_remove_multiple(context, builder, variables, memory_ctx, functions, &to_remove)
        }
    }
}

/// Compile batch remove operation
/// 
/// Generates LLVM IR for removing multiple variables in a single operation.
/// This function iterates through the list of target variables and performs
/// a remove operation on each one, invalidating them and preventing further use.
/// 
/// The batch operation is commonly used in 'clean out' operations that
/// remove multiple variables at once, such as cleaning all variables in
/// a scope or cleaning specific variables as directed by the source code.
/// 
/// # Arguments
/// 
/// * `_context` - The LLVM context (not used directly in this implementation)
/// * `builder` - The LLVM builder to use for instruction generation
/// * `variables` - The map of variable names to their LLVM values and types
/// * `memory_ctx` - The memory context to track the removals
/// * `targets` - A slice of variable names to remove
/// 
/// # Returns
/// 
/// * `Ok(())` - If all remove operations were compiled successfully
/// * `Err(String)` - If there was an error during compilation of any removal
pub fn compile_remove_multiple<'ctx>(
    context: &'ctx inkwell::context::Context,
    builder: &inkwell::builder::Builder<'ctx>,
    variables: &mut std::collections::HashMap<String, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)>,
    memory_ctx: &mut MemoryContext<'ctx>,
    functions: &std::collections::HashMap<String, inkwell::values::FunctionValue<'ctx>>,
    targets: &[String],
) -> Result<(), String> {
    for target in targets {
        compile_remove(context, builder, variables, memory_ctx, functions, target)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_context_creation() {
        let ctx = MemoryContext::new();
        assert!(!ctx.is_moved("test_var"));
    }

    #[test]
    fn test_memory_context_track_moved() {
        let mut ctx = MemoryContext::new();
        ctx.mark_moved("my_var".to_string());
        assert!(ctx.is_moved("my_var"));
        assert!(!ctx.is_moved("other_var"));
    }

    #[test]
    fn test_memory_context_unmark() {
        let mut ctx = MemoryContext::new();
        ctx.mark_moved("my_var".to_string());
        ctx.unmark_moved("my_var");
        assert!(!ctx.is_moved("my_var"));
    }

    #[test]
    fn test_memory_context_default() {
        let ctx = MemoryContext::default();
        assert!(!ctx.is_moved("anything"));
    }
}
