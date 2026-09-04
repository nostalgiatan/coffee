//! C Function Signature Management
//! 
//! Defines structures for representing C function signatures and mapping them
//! to Coffee types. This module provides the core data structures for storing
//! and managing C function declarations that are parsed from .cfc files.
//! 
//! The module includes:
//! 
//! - `CSymbol`: Represents a single C function with its name, parameters, return type, and variadic flag
//! - `CSymbolTable`: A collection of CSymbol instances organized by library name
//! - Methods for converting between Coffee and C type representations
//! 
//! These structures are essential for the Coffee compiler's C integration system,
//! allowing type checking and code generation for calls to external C functions.

use std::collections::HashMap;
use crate::parser::Function;
use crate::parser::function::Parameter;

/// A C function symbol declaration from a .cfc file
/// 
/// This structure represents a single C function declaration parsed from a .cfc file.
/// It contains all the information needed to call the C function from Coffee code,
/// including its name, parameter types and names, return type, and whether it
/// accepts variable arguments (variadic).
/// 
/// The CSymbol is the core representation of a C function in the Coffee compiler's
/// type system and is used for type checking, code generation, and FFI integration.
/// 
/// # Examples
/// 
/// ```
/// use coffee::c::CSymbol;
/// use coffee::parser::function::Parameter;
/// 
/// let symbol = CSymbol {
///     name: "printf".to_string(),
///     parameters: vec![
///         Parameter {
///             name: "format".to_string(),
///             param_type: "string".to_string(),
///             is_variadic: true,
///         }
///     ],
///     return_type: "int".to_string(),
///     is_variadic: true,
/// };
/// 
/// assert_eq!(symbol.name, "printf");
/// assert!(symbol.is_variadic);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct CSymbol {
    /// The name of the C function
    /// This is used to generate the appropriate call in the generated code
    pub name: String,
    /// The list of parameters for this function
    /// Each parameter has a name, type, and optionally indicates if it's variadic
    pub parameters: Vec<Parameter>,
    /// The return type of the function in Coffee type syntax
    /// Common examples include "int", "float", "string", "void"
    pub return_type: String,
    /// Flag indicating if this is a variadic function (accepts variable arguments)
    /// This affects how the function call is generated in the backend
    pub is_variadic: bool,
}

impl CSymbol {
    /// Create a new C symbol with the specified properties
    /// 
    /// This is the primary constructor for creating CSymbol instances. It allows
    /// direct specification of all the function's properties including name,
    /// parameters, return type, and variadic flag.
    /// 
    /// # Arguments
    /// 
    /// * `name` - The name of the C function
    /// * `parameters` - Vector of Parameter instances describing the function's parameters
    /// * `return_type` - The Coffee type of the function's return value
    /// * `is_variadic` - Whether the function accepts variable arguments
    /// 
    /// # Returns
    /// 
    /// A new CSymbol instance with the specified properties
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::c::CSymbol;
    /// use coffee::parser::function::Parameter;
    /// 
    /// let params = vec![
    ///     Parameter {
    ///         name: "x".to_string(),
    ///         param_type: "float".to_string(),
    ///         is_variadic: false,
    ///     }
    /// ];
    /// 
    /// let symbol = CSymbol::new(
    ///     "sqrt".to_string(),
    ///     params,
    ///     "float".to_string(),
    ///     false
    /// );
    /// 
    /// assert_eq!(symbol.name, "sqrt");
    /// ```
    #[allow(dead_code)]
    pub fn new(
        name: String,
        parameters: Vec<Parameter>,
        return_type: String,
        is_variadic: bool,
    ) -> Self {
        Self {
            name,
            parameters,
            return_type,
            is_variadic,
        }
    }

    /// Create a CSymbol from a Coffee Function (c fn)
    /// 
    /// This function creates a CSymbol instance from a Coffee Function AST node.
    /// It's primarily used when converting parsed `c fn` declarations from .cfc
    /// files into CSymbol instances for the symbol table.
    /// 
    /// The function checks if any of the parameters are variadic and sets the
    /// is_variadic flag accordingly.
    /// 
    /// # Arguments
    /// 
    /// * `func` - Reference to a Coffee Function AST node
    /// 
    /// # Returns
    /// 
    /// A new CSymbol instance representing the C function
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::c::CSymbol;
    /// use coffee::parser::{Function, FunctionBody};
    /// use coffee::parser::function::Parameter;
    /// 
    /// let func = Function {
    ///     name: "abs".to_string(),
    ///     parameters: vec![
    ///         Parameter {
    ///             name: "x".to_string(),
    ///             param_type: "int(4)+".to_string(),
    ///             is_variadic: false,
    ///         }
    ///     ],
    ///     return_type: "int(4)+".to_string(),
    ///     body: FunctionBody::External,
    ///     is_c: true,
    /// };
    /// 
    /// let symbol = CSymbol::from_function(&func);
    /// assert_eq!(symbol.name, "abs");
    /// ```
    #[allow(dead_code)]
    pub fn from_function(func: &Function) -> Self {
        let is_variadic = func.parameters.iter().any(|p| p.is_variadic);

        Self {
            name: func.name.clone(),
            parameters: func.parameters.clone(),
            return_type: func.return_type.clone(),
            is_variadic,
        }
    }

    /// Convert to LLVM-style function signature string
    /// 
    /// This method generates a human-readable string representation of the
    /// function signature in a format similar to LLVM's function type notation.
    /// The signature includes the function name, parameter types (excluding
    /// variadic markers), and return type.
    /// 
    /// This is primarily useful for debugging, logging, or displaying function
    /// information in error messages.
    /// 
    /// # Returns
    /// 
    /// A formatted string representing the function signature
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::c::CSymbol;
    /// use coffee::parser::function::Parameter;
    /// 
    /// let params = vec![
    ///     Parameter {
    ///         name: "x".to_string(),
    ///         param_type: "int".to_string(),
    ///         is_variadic: false,
    ///     },
    ///     Parameter {
    ///         name: "y".to_string(),
    ///         param_type: "int".to_string(),
    ///         is_variadic: false,
    ///     }
    /// ];
    /// 
    /// let symbol = CSymbol::new(
    ///     "add".to_string(),
    ///     params,
    ///     "int".to_string(),
    ///     false
    /// );
    /// 
    /// let sig = symbol.signature();
    /// assert!(sig.contains("add"));
    /// assert!(sig.contains("int"));
    /// assert!(sig.contains("->"));
    /// ```
    #[allow(dead_code)]
    pub fn signature(&self) -> String {
        let param_types: Vec<String> = self.parameters
            .iter()
            .filter(|p| !p.is_variadic)
            .map(|p| p.param_type.clone())
            .collect();

        format!("{}({}) -> {}", self.name, param_types.join(", "), self.return_type)
    }
}

/// A table of C symbols from a .cfc file
/// 
/// This structure represents a collection of C function symbols from a single
/// .cfc file or library. It maps function names to their corresponding CSymbol
/// instances and provides methods for looking up and managing these symbols.
/// 
/// The CSymbolTable is the primary data structure used by the Coffee compiler
/// to store and access C function declarations for type checking and code generation.
/// Each table corresponds to a single library or module of C functions.
/// 
/// # Examples
/// 
/// ```
/// use std::collections::HashMap;
/// use coffee::c::{CSymbolTable, CSymbol};
/// use coffee::parser::function::Parameter;
/// 
/// let mut table = CSymbolTable::new("math".to_string());
/// 
/// let symbol = CSymbol::new(
///     "sqrt".to_string(),
///     vec![Parameter {
///         name: "x".to_string(),
///         param_type: "float".to_string(),
///         is_variadic: false,
///     }],
///     "float".to_string(),
///     false
/// );
/// 
/// table.add(symbol);
/// 
/// assert!(!table.is_empty());
/// assert_eq!(table.len(), 1);
/// assert!(table.contains("sqrt"));
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct CSymbolTable {
    /// Library/module name (e.g., "hello" from libhello.cfc)
    /// This helps organize symbols by their source library
    pub library: String,
    /// Map of function name to symbol
    /// Provides O(1) lookup for C function declarations by name
    pub symbols: HashMap<String, CSymbol>,
}

impl CSymbolTable {
    /// Create a new empty symbol table
    /// 
    /// This function creates an empty CSymbolTable for the specified library.
    /// The library name is used for organization and identification purposes.
    /// 
    /// # Arguments
    /// 
    /// * `library` - The name of the library this table represents
    /// 
    /// # Returns
    /// 
    /// A new CSymbolTable instance with no symbols
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::c::CSymbolTable;
    /// 
    /// let table = CSymbolTable::new("math".to_string());
    /// assert_eq!(table.library, "math");
    /// assert!(table.is_empty());
    /// ```
    pub fn new(library: String) -> Self {
        Self {
            library,
            symbols: HashMap::new(),
        }
    }

    /// Add a symbol to the table
    /// 
    /// This method adds a CSymbol to the table, using the symbol's name as the key.
    /// If a symbol with the same name already exists in the table, it will be replaced.
    /// 
    /// # Arguments
    /// 
    /// * `symbol` - The CSymbol to add to the table
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::c::{CSymbolTable, CSymbol};
    /// use coffee::parser::function::Parameter;
    /// 
    /// let mut table = CSymbolTable::new("testlib".to_string());
    /// 
    /// let symbol = CSymbol::new(
    ///     "func".to_string(),
    ///     vec![],
    ///     "int".to_string(),
    ///     false
    /// );
    /// 
    /// table.add(symbol);
    /// assert_eq!(table.len(), 1);
    /// ```
    pub fn add(&mut self, symbol: CSymbol) {
        self.symbols.insert(symbol.name.clone(), symbol);
    }

    /// Get a symbol by name
    /// 
    /// This method looks up a CSymbol by its name in the table.
    /// 
    /// # Arguments
    /// 
    /// * `name` - The name of the function to look up
    /// 
    /// # Returns
    /// 
    /// * `Some(&CSymbol)` - Reference to the symbol if found
    /// * `None` - If no symbol with that name exists in the table
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::c::{CSymbolTable, CSymbol};
    /// use coffee::parser::function::Parameter;
    /// 
    /// let mut table = CSymbolTable::new("math".to_string());
    /// let symbol = CSymbol::new(
    ///     "sqrt".to_string(),
    ///     vec![Parameter {
    ///         name: "x".to_string(),
    ///         param_type: "float".to_string(),
    ///         is_variadic: false,
    ///     }],
    ///     "float".to_string(),
    ///     false
    /// );
    /// table.add(symbol);
    /// 
    /// let found = table.get("sqrt");
    /// assert!(found.is_some());
    /// assert_eq!(found.unwrap().name, "sqrt");
    /// 
    /// let not_found = table.get("nonexistent");
    /// assert!(not_found.is_none());
    /// ```
    pub fn get(&self, name: &str) -> Option<&CSymbol> {
        self.symbols.get(name)
    }

    /// Check if a symbol exists in the table
    /// 
    /// This method checks whether a function with the given name exists in the table.
    /// 
    /// # Arguments
    /// 
    /// * `name` - The name of the function to check for
    /// 
    /// # Returns
    /// 
    /// `true` if a symbol with that name exists, `false` otherwise
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::c::{CSymbolTable, CSymbol};
    /// use coffee::parser::function::Parameter;
    /// 
    /// let mut table = CSymbolTable::new("testlib".to_string());
    /// 
    /// let symbol = CSymbol::new(
    ///     "func".to_string(),
    ///     vec![],
    ///     "int".to_string(),
    ///     false
    /// );
    /// 
    /// table.add(symbol);
    /// 
    /// assert!(table.contains("func"));
    /// assert!(!table.contains("other"));
    /// ```
    #[allow(dead_code)]
    pub fn contains(&self, name: &str) -> bool {
        self.symbols.contains_key(name)
    }

    /// Get all symbol names in the table
    /// 
    /// This method returns a vector containing the names of all symbols in the table.
    /// This is useful for iterating over all functions in a library or for debugging.
    /// 
    /// # Returns
    /// 
    /// A vector of string slices representing all symbol names in the table
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::c::{CSymbolTable, CSymbol};
    /// use coffee::parser::function::Parameter;
    /// 
    /// let mut table = CSymbolTable::new("testlib".to_string());
    /// 
    /// let symbol1 = CSymbol::new(
    ///     "func1".to_string(),
    ///     vec![],
    ///     "int".to_string(),
    ///     false
    /// );
    /// 
    /// let symbol2 = CSymbol::new(
    ///     "func2".to_string(),
    ///     vec![],
    ///     "void".to_string(),
    ///     false
    /// );
    /// 
    /// table.add(symbol1);
    /// table.add(symbol2);
    /// 
    /// let names = table.symbol_names();
    /// assert_eq!(names.len(), 2);
    /// assert!(names.contains(&"func1"));
    /// assert!(names.contains(&"func2"));
    /// ```
    #[allow(dead_code)]
    pub fn symbol_names(&self) -> Vec<&str> {
        self.symbols.keys().map(|s| s.as_str()).collect()
    }

    /// Get the number of symbols in the table
    /// 
    /// This method returns the count of symbols in the table.
    /// 
    /// # Returns
    /// 
    /// The number of CSymbol instances in the table
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::c::{CSymbolTable, CSymbol};
    /// use coffee::parser::function::Parameter;
    /// 
    /// let mut table = CSymbolTable::new("testlib".to_string());
    /// assert_eq!(table.len(), 0);
    /// 
    /// let symbol = CSymbol::new(
    ///     "func".to_string(),
    ///     vec![],
    ///     "int".to_string(),
    ///     false
    /// );
    /// 
    /// table.add(symbol);
    /// assert_eq!(table.len(), 1);
    /// ```
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Check if the table is empty
    /// 
    /// This method returns true if the table contains no symbols.
    /// 
    /// # Returns
    /// 
    /// `true` if there are no symbols in the table, `false` otherwise
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::c::CSymbolTable;
    /// 
    /// let table = CSymbolTable::new("testlib".to_string());
    /// assert!(table.is_empty());
    /// 
    /// // After adding symbols, it would return false
    /// ```
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_c_symbol_creation() {
        let symbol = CSymbol::new(
            "test".to_string(),
            vec![],
            "void".to_string(),
            false,
        );

        assert_eq!(symbol.name, "test");
        assert_eq!(symbol.return_type, "void");
        assert!(!symbol.is_variadic);
    }

    #[test]
    fn test_symbol_table() {
        let mut table = CSymbolTable::new("testlib".to_string());
        assert_eq!(table.library, "testlib");
        assert!(table.is_empty());

        let symbol = CSymbol::new(
            "func".to_string(),
            vec![],
            "int".to_string(),
            false,
        );

        table.add(symbol);
        assert!(!table.is_empty());
        assert_eq!(table.len(), 1);
        assert!(table.contains("func"));
    }

    #[test]
    fn test_signature_string() {
        let symbol = CSymbol::new(
            "add".to_string(),
            vec![
                Parameter {
                    name: "a".to_string(),
                    param_type: "int(4)".to_string(),
                    is_variadic: false,
                },
                Parameter {
                    name: "b".to_string(),
                    param_type: "int(4)".to_string(),
                    is_variadic: false,
                },
            ],
            "int(4)".to_string(),
            false,
        );

        let sig = symbol.signature();
        assert!(sig.contains("add"));
        assert!(sig.contains("int(4)"));
    }
}
