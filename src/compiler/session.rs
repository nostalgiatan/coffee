//! Shared compilation session (analyzer, types, diagnostics, import caches).

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::c;
use crate::diagnostics::DiagnosticEmitter;
use crate::semantic::SemanticAnalyzer;
use crate::types::{TypeChecker, TypeRegistry};

use super::{CompilerConfig, ParsedModule};

/// Compilation session that manages shared state across compilation
pub struct Session {
    pub analyzer: Arc<RwLock<SemanticAnalyzer>>,
    pub type_checker: Arc<RwLock<TypeChecker>>,
    pub emitter: DiagnosticEmitter,
    pub config: CompilerConfig,
    pub module_cache: Arc<RwLock<HashMap<String, ParsedModule>>>,
    pub import_stack: Arc<RwLock<Vec<String>>>,
    pub c_imports: Arc<RwLock<Vec<String>>>,
    pub cfc_symbols: Arc<RwLock<HashMap<String, c::CSymbolTable>>>,
}

impl Session {
    pub fn new() -> Self {
        Self::with_config(CompilerConfig::default())
    }

    pub fn with_config(config: CompilerConfig) -> Self {
        let type_registry = Arc::new(RwLock::new(TypeRegistry::root()));

        let analyzer = Arc::new(RwLock::new(SemanticAnalyzer::new(type_registry.clone())));
        let type_checker = Arc::new(RwLock::new(
            crate::types::TypeChecker::with_analyzer(
                type_registry.clone(),
                crate::types::CheckingMode::Comprehensive,
                Some(analyzer.clone()),
            )
        ));

        Session {
            analyzer,
            type_checker,
            emitter: DiagnosticEmitter::new(),
            config,
            module_cache: Arc::new(RwLock::new(HashMap::new())),
            import_stack: Arc::new(RwLock::new(Vec::new())),
            c_imports: Arc::new(RwLock::new(Vec::new())),
            cfc_symbols: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn get_c_imports(&self) -> Vec<String> {
        self.c_imports.read()
            .map(|imports| imports.clone())
            .unwrap_or_default()
    }

    pub fn get_cfc_symbols(&self) -> std::collections::HashMap<String, c::CSymbolTable> {
        self.cfc_symbols.read()
            .map(|symbols| symbols.clone())
            .unwrap_or_default()
    }
}
