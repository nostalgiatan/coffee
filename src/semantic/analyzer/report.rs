use super::SemanticAnalyzer;
use crate::types::errors::TypeSystemError;

/// 分析报告
#[derive(Debug, Clone)]
pub struct AnalysisReport {
    pub scope_info: ScopeInfo,
    pub symbol_info: SymbolInfo,
    pub errors: Vec<TypeSystemError>,
}

/// 作用域信息
#[derive(Debug, Clone, Default)]
pub struct ScopeInfo {
    pub total_scopes: usize,
    pub bindings: usize,
    pub depth: usize,
}

/// 符号信息
#[derive(Debug, Clone, Default)]
pub struct SymbolInfo {
    pub declared: usize,
    pub referenced: usize,
    pub unused: Vec<String>,
}

impl SemanticAnalyzer {
    /// 获取分析报告
    pub fn report(&self) -> AnalysisReport {
        let scope_info = if let Ok(scope) = self.scope_space.read() {
            ScopeInfo {
                total_scopes: 1 + scope.children().len(), // 简化计算
                bindings: scope.all_names().len(),
                depth: scope.depth(),
            }
        } else {
            ScopeInfo::default()
        };

        let symbol_info = if let Ok(symbols) = self.symbol_space.read() {
            SymbolInfo {
                declared: symbols.declared_symbols().len(),
                referenced: symbols.referenced_symbols().len(),
                unused: symbols
                    .unused_symbols()
                    .into_iter()
                    .filter(|name| !name.starts_with('_') && name != "main")
                    .collect(),
            }
        } else {
            SymbolInfo::default()
        };

        AnalysisReport {
            scope_info,
            symbol_info,
            errors: self.errors.clone(),
        }
    }

    /// Generate a report with source file information (for compatibility)
    pub fn to_report_with_source(&self, _source_file: Option<String>) -> AnalysisReport {
        // For now, just use the same report generation, but could include source file info in the future
        self.report()
    }
    /// Check for unused variables
    pub fn check_unused_variables(&self) -> Vec<String> {
        let mut warnings = Vec::new();

        // Get the symbol space
        let symbol_space = self.symbol_space.read().unwrap();
        let unused = symbol_space.unused_symbols();

        for var_name in unused {
            // Skip common special variables
            if var_name.starts_with('_') || var_name == "main" {
                continue;
            }

            // Skip command-line arguments (arg1, arg2, etc.)
            if self.is_command_line_arg(&var_name) {
                continue;
            }

            warnings.push(format!("Warning: Variable '{}' declared but never used", var_name));
        }

        warnings
    }
}
