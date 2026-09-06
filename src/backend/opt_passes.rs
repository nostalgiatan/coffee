//! LLVM new pass manager pipelines via inkwell `Module::run_passes`.
//!
//! Flags follow clang, not "turn everything on at O1":
//! - O1: `default<O1>`, no loop/SLP vectorize, no unroll/interleave, no merge
//! - O2: `default<O2>`, vectorize + unroll + interleave; no merge-functions
//! - O3: `default<O3>`, same as O2 plus merge-functions
//!
//! `LLVMCreatePassBuilderOptions` defaults several of those knobs **on**, so
//! O1 must set them false or `default<O1>` still vectorizes.

use inkwell::module::Module;
use inkwell::passes::PassBuilderOptions;
use inkwell::targets::TargetMachine;
use inkwell::OptimizationLevel;

/// Pass-builder pipeline for `level`, matching inkwell 0.7 `default<On>`.
fn pipeline_for(level: OptimizationLevel) -> Option<&'static str> {
    match level {
        OptimizationLevel::None => None,
        OptimizationLevel::Less => Some("default<O1>"),
        OptimizationLevel::Default => Some("default<O2>"),
        OptimizationLevel::Aggressive => Some("default<O3>"),
    }
}

fn pass_builder_options(level: OptimizationLevel, verify_each: bool) -> PassBuilderOptions {
    let opts = PassBuilderOptions::create();
    opts.set_verify_each(verify_each);
    // Clang: vectorize / unroll / interleave start at -O2; merge-functions at -O3.
    let (vectorize, unroll, slp, interleave, merge) = match level {
        OptimizationLevel::None | OptimizationLevel::Less => {
            (false, false, false, false, false)
        }
        OptimizationLevel::Default => (true, true, true, true, false),
        OptimizationLevel::Aggressive => (true, true, true, true, true),
    };
    opts.set_loop_vectorization(vectorize);
    opts.set_loop_unrolling(unroll);
    opts.set_loop_slp_vectorization(slp);
    opts.set_loop_interleaving(interleave);
    opts.set_merge_functions(merge);
    opts
}

/// Run the new pass manager on `module`. `None` / O0 is a no-op.
pub fn apply(
    module: &Module<'_>,
    machine: &TargetMachine,
    level: OptimizationLevel,
) -> Result<(), String> {
    let Some(passes) = pipeline_for(level) else {
        return Ok(());
    };
    module
        .run_passes(passes, machine, pass_builder_options(level, false))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;
    use inkwell::targets::{CodeModel, InitializationConfig, RelocMode, Target, TargetMachine};
    use inkwell::OptimizationLevel;

    fn native_machine(level: OptimizationLevel) -> TargetMachine {
        Target::initialize_native(&InitializationConfig::default()).unwrap();
        let triple = TargetMachine::get_default_triple();
        let target = Target::from_triple(&triple).unwrap();
        target
            .create_target_machine(
                &triple,
                "generic",
                "",
                level,
                RelocMode::Default,
                CodeModel::Default,
            )
            .expect("target machine")
    }

    fn nop_module(context: &Context) -> inkwell::module::Module<'_> {
        let module = context.create_module("opt_passes_smoke");
        let void_ty = context.void_type();
        let fn_ty = void_ty.fn_type(&[], false);
        let function = module.add_function("nop", fn_ty, None);
        let entry = context.append_basic_block(function, "entry");
        let builder = context.create_builder();
        builder.position_at_end(entry);
        builder.build_return(None).unwrap();
        module
    }

    #[test]
    fn o1_o2_o3_pipelines_succeed() {
        let context = Context::create();
        for level in [
            OptimizationLevel::Less,
            OptimizationLevel::Default,
            OptimizationLevel::Aggressive,
        ] {
            let module = nop_module(&context);
            let machine = native_machine(level);
            apply(&module, &machine, level).unwrap_or_else(|e| {
                panic!("{:?} pipeline: {}", level, e)
            });
        }
    }

    #[test]
    fn default_o2_pipeline_succeeds() {
        let context = Context::create();
        let module = nop_module(&context);
        let machine = native_machine(OptimizationLevel::Default);

        apply(&module, &machine, OptimizationLevel::Default)
            .expect("default<O2> pipeline");

        module
            .run_passes(
                "default<O2>",
                &machine,
                pass_builder_options(OptimizationLevel::Default, true),
            )
            .expect("default<O2> with verify_each");
    }
}
