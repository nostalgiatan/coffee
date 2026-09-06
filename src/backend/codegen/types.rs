use inkwell::types::BasicTypeEnum;

use super::CodeGenerator;

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    /// Convert Coffee type to LLVM type
    ///
    /// Converts a Coffee type string (e.g., "int", "string", "int(4)+") to its
    /// corresponding LLVM type representation. This method uses the TypeMapper
    /// for consistent type conversion across the code generator.
    ///
    /// # Arguments
    ///
    /// * `type_str` - A string representation of the Coffee type to convert
    ///
    /// # Returns
    ///
    /// * `Ok(BasicTypeEnum)` - The corresponding LLVM type
    /// * `Err(String)` - If the type conversion fails
    ///
    /// Uses TypeMapper for consistent type conversion
    pub fn coffee_type_to_llvm(&self, type_str: &str) -> Result<BasicTypeEnum<'ctx>, String> {
        // Use TypeMapper for type conversion
        self.type_mapper.try_map_type(type_str)
    }
}
