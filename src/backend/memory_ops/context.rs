//! Memory context: scope, ownership, and lifetime tracking.

use super::lifetime::{LifetimeDiagnostic, LifetimeInfo, VariableState};

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
    /// `.cfc` memcpy types: no Coffee `__drop`.
    pub c_value_names: std::collections::HashSet<String>,
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
            c_value_names: std::collections::HashSet::new(),
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

    /// True if the variable was born in a scope still on the stack (not a sibling branch).
    pub fn is_in_live_scope(&self, var_name: &str) -> bool {
        match self.variable_scopes.get(var_name) {
            Some(scope) => self.scope_stack.contains(scope),
            None => true,
        }
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
            info.death_line = Some(self.current_line);
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

    /// Sibling CFG arms reuse names (`let s` in if/elif). A prior arm's `rm`
    /// must not make the next arm's `rm` look like a double-free.
    pub fn unmark_dropped(&mut self, var_name: &str) {
        self.dropped_variables.remove(var_name);
        if let Some(info) = self.lifetimes.get_mut(var_name) {
            info.state = VariableState::Initialized;
            info.death_line = None;
            info.properly_cleaned = false;
        }
    }

    /// MIR blocks are not a single path. `rm` in an exit block must not
    /// poison later-compiled loop-body blocks that still use the alloca.
    pub fn unmark_all_dropped(&mut self) {
        let names: Vec<String> = self.dropped_variables.iter().cloned().collect();
        for name in names {
            self.unmark_dropped(&name);
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
    #[cfg(test)]
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
                        if let (Some(last_use), Some(death_line)) =
                            (info.last_use_line, info.death_line)
                        {
                            if last_use > death_line {
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
