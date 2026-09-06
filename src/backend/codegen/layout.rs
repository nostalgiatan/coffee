use super::CodeGenerator;

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    /// Get memory layout report
    pub fn get_memory_layout_report(&self) -> String {
        self.layout_collector.generate_report()
    }

    /// Check if there are any structs to report
    pub fn has_structs(&self) -> bool {
        !self.layout_collector.structs.is_empty()
    }

    pub(crate) fn llvm_target_data(&self) -> inkwell::targets::TargetData {
        let dl = self.backend.module.get_data_layout();
        let repr = dl.as_str().to_str().unwrap_or("e");
        inkwell::targets::TargetData::create(repr)
    }
}
