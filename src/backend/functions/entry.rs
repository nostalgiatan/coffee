use crate::parser::expr::Expression;
use crate::backend::codegen::CodeGenerator;
use inkwell::{values::BasicValueEnum, AddressSpace};

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    /// Compile main entry point statement
    /// 
    /// This method processes the main entry point statement from the Coffee source,
    /// storing the entry point information for later generation of the actual C
    /// main() function. The main entry point specifies which function should be
    /// called when the program starts execution.
    /// 
    /// The method validates that only one main entry point is specified per program
    /// and stores the entry function name along with its arguments for use during
    /// the main function generation phase.
    /// 
    /// # Arguments
    /// 
    /// * `main_entry` - The parsed MainEntry to compile
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` - If the main entry point was processed successfully
    /// * `Err(String)` - If there was an error during processing (e.g., duplicate main)
    /// 
    /// This stores the entry point info and generates the actual main() function later
    pub fn compile_main_entry(&mut self, main_entry: &crate::parser::MainEntry) -> Result<(), String> {
        // Check if main entry already specified
        if self.main_entry.is_some() {
            return Err(self.error("compile_main_entry",
                "multiple main() entry points specified\n  = note: only one main(entry_function()) is allowed per program"));
        }

        // Store the main entry point info
        self.main_entry = Some((
            main_entry.entry_function.clone(),
            main_entry.args.clone(),
        ));

        Ok(())
    }

    /// Generate the actual main() function that calls the entry function
    /// 
    /// This internal method creates the actual C-compatible main() function that
    /// serves as the entry point for the compiled program. The generated main()
    /// function follows the standard C signature (int main(int argc, char** argv))
    /// and calls the Coffee function specified as the program's entry point.
    /// 
    /// The method handles command-line argument passing to the Coffee entry function
    /// and manages the conversion between C-style arguments and Coffee's expected
    /// parameter format. It also ensures that the main function returns an
    /// appropriate exit code to the operating system.
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` - If the main function was generated successfully
    /// * `Err(String)` - If there was an error during generation
    pub(crate) fn generate_main_function(&mut self) -> Result<(), String> {
        let (entry_func_name, entry_args) = self.main_entry.take().ok_or_else(|| {
            self.error("generate_main",
                "no main entry point specified\n  = note: use main(function_name()) to specify the program entry point")
        })?;

        let context = self.backend.context;
        let i32_type = context.i32_type();
        let i8_ptr_type = context.ptr_type(AddressSpace::default());

        // Create C-compatible main function: int main(int argc, char** argv)
        let main_type = i32_type.fn_type(&[i32_type.into(), i8_ptr_type.into()], false);
        let main_func = self.backend.module.add_function("main", main_type, None);

        let entry_block = context.append_basic_block(main_func, "entry");
        self.backend.builder.position_at_end(entry_block);

        // Get argc and argv parameters
        let params = main_func.get_params();
        if params.len() < 2 {
            return Err(self.error("generate_main", "main function requires 2 parameters (argc, argv)"));
        }

        let argc = params[0].into_int_value();
        let argv = params[1].into_pointer_value();

        // Store argc and argv for use in arg1, arg2, ... expressions
        self.main_argc = Some(argc);
        self.main_argv = Some(argv);

        // Check if the entry function exists
        let entry_func = *self.functions.get(&entry_func_name).ok_or_else(|| {
            self.error("generate_main",
                format!("entry function '{}' not found\n  = note: ensure the function is defined before main() statement", entry_func_name))
        })?;

        // Compile the entry function arguments
        let mut compiled_args = Vec::new();
        for arg_expr in &entry_args {
            match arg_expr.kind() {
                Expression::Variable(name) => {
                    if let Some(arg_num) = Self::parse_arg_number(name) {
                        let arg_value = self.get_command_line_arg(argv, argc, arg_num)?;
                        compiled_args.push(arg_value);
                    } else if name == "argc" {
                        let i64_type = context.i64_type();
                        let argc_i64 = self.backend.builder.build_int_s_extend(argc, i64_type, "argc_i64")
                            .map_err(|e| self.error("generate_main", format!("failed to extend argc to i64: {}", e)))?;
                        compiled_args.push(argc_i64.into());
                    } else if name == "argv" {
                        compiled_args.push(argv.into());
                    } else {
                        compiled_args.push(self.compile_literal_atom(name).map_err(|e| {
                            self.error("generate_main",
                                format!("failed to compile entry function argument '{}': {}", arg_expr, e))
                        })?);
                    }
                }
                Expression::Literal(lit) => {
                    compiled_args.push(self.compile_literal_atom(lit).map_err(|e| {
                        self.error("generate_main",
                            format!("failed to compile entry function argument '{}': {}", arg_expr, e))
                    })?);
                }
                _ => {
                    return Err(self.error(
                        "generate_main",
                        format!(
                            "entry argument '{}' must be a literal or argc/argv/argN",
                            arg_expr
                        ),
                    ));
                }
            }
        }

        // Call the entry function
        let args_ref: Vec<_> = compiled_args.iter().map(|v| (*v).into()).collect();

        let call_site = self.backend.builder
            .build_call(entry_func, &args_ref, "entry_call")
            .map_err(|e| self.error("generate_main", format!("failed to build call to entry function: {}", e)))?;

        // Get return value and convert to i32
        let exit_code = match call_site.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(val) => {
                // Convert the return value to i32
                match val {
                    BasicValueEnum::IntValue(int_val) => {
                        // Truncate or extend to i32 as needed
                        let bit_width = int_val.get_type().get_bit_width();
                        if bit_width == 64 {
                            // Truncate i64 to i32
                            self.backend.builder.build_int_truncate(int_val, i32_type, "trunc")
                                .map_err(|e| self.error("generate_main", format!("failed to truncate i64 to i32: {}", e)))?
                                .into()
                        } else if bit_width == 32 {
                            // Already i32
                            int_val.into()
                        } else if bit_width < 32 {
                            // Sign extend to i32
                            self.backend.builder.build_int_s_extend(int_val, i32_type, "sext")
                                .map_err(|e| self.error("generate_main", format!("failed to extend to i32: {}", e)))?
                                .into()
                        } else {
                            // Truncate larger types to i32
                            self.backend.builder.build_int_truncate(int_val, i32_type, "trunc")
                                .map_err(|e| self.error("generate_main", format!("failed to truncate to i32: {}", e)))?
                                .into()
                        }
                    }
                    BasicValueEnum::FloatValue(_) => {
                        // For float, return 0
                        i32_type.const_int(0, false)
                    }
                    _ => {
                        // For other types, return 0
                        i32_type.const_int(0, false)
                    }
                }
            }
            inkwell::values::ValueKind::Instruction(_) => {
                // Void return - exit with 0
                i32_type.const_int(0, false)
            }
        };

        // Return the exit code
        self.backend.builder.build_return(Some(&exit_code))
            .map_err(|e| self.error("generate_main", format!("failed to build return: {}", e)))?;

        Ok(())
    }

    /// Parse arg1, arg2, arg3, etc. and return the argument number
    /// Returns Some(arg_num) for valid arg names, None otherwise
    pub fn parse_arg_number(arg_str: &str) -> Option<u32> {
        let arg_str = arg_str.trim();

        // Check if it starts with "arg" followed by a number
        if arg_str.starts_with("arg") {
            let num_str = &arg_str[3..]; // Skip "arg"
            if let Ok(num) = num_str.parse::<u32>() {
                if num >= 1 && num <= 127 { // Reasonable limit
                    return Some(num);
                }
            }
        }

        None
    }

    /// Get command-line argument from argv by index
    /// argv[0] is the program name, argv[1] is arg1, etc.
    pub fn get_command_line_arg(
        &self,
        argv: inkwell::values::PointerValue<'ctx>,
        argc: inkwell::values::IntValue<'ctx>,
        arg_num: u32,
    ) -> Result<inkwell::values::BasicValueEnum<'ctx>, String> {
        let builder = &self.backend.builder;
        let context = self.backend.context;

        // Check bounds: arg_num must be < argc
        let i32_type = context.i32_type();
        let arg_num_value = i32_type.const_int(arg_num as u64, false);

        // Build condition: if (arg_num >= argc) return default value
        let arg_num_ge_argc = builder
            .build_int_compare(inkwell::IntPredicate::UGE, arg_num_value, argc, "arg.ge.argc")
            .map_err(|e| format!("failed to compare arg_num with argc: {}", e))?;

        let current_block = builder.get_insert_block().ok_or("no insert block")?;
        let function = current_block.get_parent().ok_or("no parent function")?;

        let then_block = context.append_basic_block(function, "arg.bounds_fail");
        let else_block = context.append_basic_block(function, "arg.bounds_ok");
        let merge_block = context.append_basic_block(function, "arg.merge");

        builder
            .build_conditional_branch(arg_num_ge_argc, then_block, else_block)
            .map_err(|e| format!("failed to build conditional branch: {}", e))?;

        // Then block: return default empty string
        builder.position_at_end(then_block);
        let default_str = builder
            .build_global_string_ptr("default_empty_str", "")
            .map_err(|e| format!("failed to build default string: {}", e))?;
        builder.build_unconditional_branch(merge_block).map_err(|e| format!("failed to build branch: {}", e))?;

        // Else block: get argv[arg_num]
        builder.position_at_end(else_block);

        // Calculate argv[arg_num] pointer
        let argv_elem_ptr = unsafe {
            builder.build_in_bounds_gep(
                context.ptr_type(AddressSpace::default()),
                argv,
                &[arg_num_value],
                "argv_elem_ptr",
            )
        }.map_err(|e| format!("failed to build GEP: {}", e))?;

        // Load the pointer to the actual string
        let arg_ptr = builder
            .build_load(context.ptr_type(AddressSpace::default()), argv_elem_ptr, "arg_ptr")
            .map_err(|e| format!("failed to load argv element: {}", e))?
            .into_pointer_value();

        builder.build_unconditional_branch(merge_block).map_err(|e| format!("failed to build branch: {}", e))?;

        // Merge block
        builder.position_at_end(merge_block);

        let phi_node = builder
            .build_phi(context.ptr_type(AddressSpace::default()), "arg_value")
            .map_err(|e| format!("failed to build phi node: {}", e))?;

        phi_node.add_incoming(&[(&default_str, then_block), (&arg_ptr, else_block)]);

        // Convert PhiValue to PointerValue and then to BasicValueEnum
        Ok(inkwell::values::BasicValueEnum::PointerValue(
            phi_node.as_basic_value().into_pointer_value()
        ))
    }

}

#[cfg(test)]
mod tests {
    use crate::parser::expr::{parse_expression, Expression};

    #[test]
    fn parsed_main_shim_args_need_kind_not_outer_match() {
        let argc = parse_expression("argc").expect("parse");
        assert!(
            !matches!(&argc, Expression::Variable(name) if name == "argc"),
            "parse_expression wraps Variable; matching the outer enum misses argc"
        );
        assert!(
            matches!(argc.kind(), Expression::Variable(name) if name == "argc"),
            "generate_main must match arg_expr.kind() for argc"
        );

        let argv = parse_expression("argv").expect("parse");
        assert!(matches!(argv.kind(), Expression::Variable(name) if name == "argv"));

        let arg1 = parse_expression("arg1").expect("parse");
        match arg1.kind() {
            Expression::Variable(name) => {
                assert_eq!(
                    crate::backend::codegen::CodeGenerator::<'_, '_>::parse_arg_number(name),
                    Some(1)
                );
            }
            other => panic!("expected Variable(arg1), got {other:?}"),
        }
    }
}
