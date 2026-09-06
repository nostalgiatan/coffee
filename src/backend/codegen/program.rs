use crate::c;
use crate::coffee_debug;
use crate::parser::{Program, Statement};

use super::CodeGenerator;

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    /// Dead trap API. Does not compile a program.
    ///
    /// Previously this called `compile_program_with_hir(..., Vec::new())`, so every
    /// function hit `missing MIR` as if empty MIR were a real compile. Drivers must
    /// use [`Self::compile_program_with_hir`] with frontend MIR.
    #[allow(dead_code)]
    pub fn compile_program(&mut self, _program: &Program, _c_imports: &[String], _cfc_symbols: std::collections::HashMap<String, c::CSymbolTable>) -> Result<(), String> {
        Err(self.error(
            "compile_program",
            "compile_program cannot compile function bodies from the AST; use compile_program_with_hir",
        ))
    }

    /// Compile using frontend MIR when a function lowered completely.
    pub fn compile_program_with_hir(
        &mut self,
        program: &Program,
        c_imports: &[String],
        cfc_symbols: std::collections::HashMap<String, c::CSymbolTable>,
        hir_fns: Vec<crate::hir::MirFn>,
    ) -> Result<(), String> {
        self.hir_fns = hir_fns
            .into_iter()
            .map(|f| (f.name.clone(), f))
            .collect();
        self.compile_program_with_imports(program, c_imports, cfc_symbols, &std::collections::HashMap::new(), &std::collections::HashMap::new())
    }

    /// Compile a program with imported modules
    ///
    /// This is the main entry point for compiling a Coffee program. It processes
    /// all statements in the program and generates LLVM IR code for them.
    ///
    /// # Arguments
    ///
    /// * `program` - The parsed Coffee program to compile
    /// * `c_imports` - List of C library imports
    /// * `cfc_symbols` - C function symbol tables from .cfc files
    /// * `imported_modules` - Map of module name to parsed module from imports
    /// * `imported_symbols` - Map of module name to list of imported symbols (empty = all)
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the program was compiled successfully
    /// * `Err(String)` - If there was an error during compilation
    pub fn compile_program_with_imports(&mut self,
        program: &Program,
        c_imports: &[String],
        cfc_symbols: std::collections::HashMap<String, c::CSymbolTable>,
        imported_modules: &std::collections::HashMap<String, crate::compiler::import_resolver::ParsedModule>,
        imported_symbols: &std::collections::HashMap<String, Vec<String>>,
    ) -> Result<(), String> {
        // MEDIUM-5 FIX: Limit number of functions to prevent DoS via code bloat
        const MAX_FUNCTIONS: usize = 1000;
        let function_count = program.statements.iter()
            .filter(|s| matches!(s, Statement::Function(_)))
            .count();

        if function_count > MAX_FUNCTIONS {
            return Err(self.error("compile_program",
                format!("Program contains too many functions\n  = note: found {} functions, maximum: {}\n  = help: reduce number of functions or split into multiple modules",
                    function_count, MAX_FUNCTIONS)));
        }

        // MEDIUM-6 FIX: Limit total statements to prevent DoS
        const MAX_STATEMENTS: usize = 10000;
        if program.statements.len() > MAX_STATEMENTS {
            return Err(self.error("compile_program",
                format!("Program contains too many statements\n  = note: found {} statements, maximum: {}\n  = help: reduce program size or split into multiple modules",
                    program.statements.len(), MAX_STATEMENTS)));
        }

        // Store C library imports for symbol resolution
        self.c_imports = c_imports.to_vec();
        // Store C function symbol tables from .cfc files
        self.cfc_symbols = cfc_symbols;
        self.install_cfc_record_types()?;

        // Declare all imported C functions from c_imports
        let c_imports_to_declare = self.c_imports.clone();
        for c_import in &c_imports_to_declare {
            if c_import.contains(':') {
                // Format is "library:symbol"
                let parts: Vec<&str> = c_import.split(':').collect();
                if parts.len() == 2 {
                    let func_name = parts[1];
                    // Declare the external function
                    if let Err(e) = self.declare_external_function(func_name) {
                        eprintln!("Warning: Failed to declare external function '{}': {}", func_name, e);
                    }
                }
            } else {
                // Direct symbol name
                // Declare the external function
                if let Err(e) = self.declare_external_function(c_import) {
                    eprintln!("Warning: Failed to declare external function '{}': {}", c_import, e);
                }
            }
        }

        // malloc / free / etc. must exist before `compile_class` emits `__new`.
        self.declare_runtime_functions();
        self.index_nested_decls(program);

        // Methods compiled in `compile_class` may call Coffee functions spliced
        // from imports (`use * in sys` inside `mem.cf`). Declare signatures first.
        for (module_name, parsed_module) in imported_modules {
            let symbols_to_import = imported_symbols.get(module_name);
            for stmt in &parsed_module.statements {
                if let Statement::Function(func) = stmt {
                    let should_import = if let Some(symbols) = symbols_to_import {
                        symbols.is_empty() || symbols.contains(&func.name)
                    } else {
                        false
                    };
                    if should_import {
                        self.ensure_coffee_function_declared(func)?;
                    }
                }
            }
        }
        for stmt in &program.statements {
            if let Statement::Function(func) = stmt {
                self.ensure_coffee_function_declared(func)?;
            }
        }

        let mut class_defs = Vec::new();
        for stmt in &program.statements {
            if let Statement::Class(class) = stmt {
                self.classes.insert(class.name.clone(), class.clone());
                class_defs.push(class.clone());
            }
        }
        if class_defs.iter().any(|c| c.parent.as_deref() == Some("Error")) {
            self.classes
                .entry("Error".to_string())
                .or_insert_with(crate::backend::error::builtin_error_class_def);
        }
        for class in crate::backend::class_layout::class_compile_order(&class_defs)? {
            self.compile_class(&class)?;
        }

        // Second pass: define functions and compile code
        coffee_debug!("DEBUG: compile_program: Second pass: compiling {} statements", program.statements.len());
        for (idx, stmt) in program.statements.iter().enumerate() {
            coffee_debug!("DEBUG: compile_program: compiling statement {}: {:?}", idx, stmt);
            // CRITICAL: Reset current_function before compiling top-level statements
            // This ensures that top-level statements (like main() calls) are not compiled
            // into the wrong function's basic block
            self.current_function = None;
            self.compile_statement(stmt)?;
        }

        // Third pass: generate the actual main() function if entry point is specified
        if self.main_entry.is_some() {
            self.generate_main_function()?;
        }

        // Fourth pass: lazy declaration of used C functions
        // This follows the "declare on demand" principle - only declare functions that are actually used
        self.declare_used_c_functions()?;

        Ok(())
    }

    fn install_cfc_record_types(&mut self) -> Result<(), String> {
        let tables = self.cfc_symbols.clone();
        for table in tables.values() {
            for def in table.type_defs.values() {
                match def {
                    c::CTypeDef::Class { name, fields, .. } => {
                        let class = cfc_fields_to_class(name, fields);
                        self.classes.insert(name.clone(), class.clone());
                        self.c_value_names.insert(name.clone());
                        self.type_mapper.c_struct_names.insert(name.clone());
                    }
                    c::CTypeDef::Union {
                        name,
                        fields,
                        size,
                        ..
                    } => {
                        let class = cfc_fields_to_class(name, fields);
                        self.classes.insert(name.clone(), class);
                        self.c_value_names.insert(name.clone());
                        let bytes = (*size).clamp(1, 1 << 20) as u32;
                        self.type_mapper
                            .c_union_byte_sizes
                            .insert(name.clone(), bytes);
                    }
                    c::CTypeDef::Enum { name, .. } => {
                        self.c_value_names.insert(name.clone());
                        self.type_mapper.c_enum_names.insert(name.clone());
                    }
                    c::CTypeDef::Newtype { .. } => {}
                }
            }
        }
        let class_names: Vec<String> = self
            .type_mapper
            .c_struct_names
            .iter()
            .cloned()
            .collect();
        for name in class_names {
            if let Some(class) = self.classes.get(&name).cloned() {
                self.type_mapper.get_or_create_struct_type_from_class(
                    &name,
                    &class,
                    &self.classes,
                )?;
            }
        }
        self.memory_ctx.c_value_names = self.c_value_names.clone();
        Ok(())
    }
}

fn cfc_fields_to_class(name: &str, fields: &[(String, String)]) -> crate::parser::class::ClassDef {
    crate::parser::class::ClassDef {
        name: name.to_string(),
        type_params: vec![],
        parent: None,
        fields: fields
            .iter()
            .map(|(n, t)| crate::parser::class::ClassField {
                name: n.clone(),
                field_type: t.clone(),
                bit_width: None,
            })
            .collect(),
        methods: vec![],
        packed: false,
        has_constructor: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{codegen::CodeGenerator, Backend};
    use crate::parser::expr::Expression;
    use crate::parser::function::{Function, FunctionBody};
    use crate::parser::var::ReturnStmt;
    use inkwell::context::Context;

    fn int_main() -> Function {
        Function {
            type_params: vec![],
            name: "main".into(),
            parameters: vec![],
            return_type: "int".into(),
            error_handler: None,
            body: FunctionBody::Block(vec![Statement::Return(ReturnStmt {
                value: Some(Expression::literal("0")),
            })]),
            is_c: false,
        }
    }

    #[test]
    fn compile_program_cannot_compile_function_bodies_from_ast() {
        let context = Context::create();
        let backend = Backend::new(&context, "trap_api");
        let mut cg = CodeGenerator::new(&backend);
        let program = Program::new(vec![Statement::Function(int_main())]);
        let err = cg
            .compile_program(&program, &[], std::collections::HashMap::new())
            .expect_err("AST-only compile_program is a dead trap");
        assert!(
            err.contains("compile_program_with_hir"),
            "dead API must point at compile_program_with_hir, not pretend to compile bodies; got: {}",
            err
        );
        assert!(
            !err.contains("missing MIR for function"),
            "must not walk functions as if empty hir_fns were a real compile; got: {}",
            err
        );
    }

    #[test]
    fn compile_program_with_hir_without_mir_is_missing_mir() {
        let context = Context::create();
        let backend = Backend::new(&context, "with_hir_empty");
        let mut cg = CodeGenerator::new(&backend);
        let program = Program::new(vec![Statement::Function(int_main())]);
        let err = cg
            .compile_program_with_hir(&program, &[], std::collections::HashMap::new(), Vec::new())
            .expect_err("empty hir_fns is missing MIR, not AST");
        assert!(
            err.contains("missing MIR for function"),
            "got: {}",
            err
        );
    }
}
