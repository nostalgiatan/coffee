use crate::types::definition::*;
use crate::types::errors::TypeSystemError;
use std::collections::HashMap;

/// 符号空间：管理符号声明和引用
pub struct SymbolSpace {
    /// 空间ID
    id: SpaceId,
    /// 空间名称
    name: String,
    /// 符号声明
    declarations: HashMap<String, Binding>,
    /// 符号引用
    references: HashMap<String, Vec<Span>>,
}

impl SymbolSpace {
    /// 创建新的符号空间
    pub fn new(name: impl Into<String>) -> Self {
        SymbolSpace {
            id: SpaceId::new(),
            name: name.into(),
            declarations: HashMap::new(),
            references: HashMap::new(),
        }
    }

    /// 声明符号
    pub fn declare(&mut self, name: String, binding: Binding) -> Result<(), TypeSystemError> {
        if let Some(existing_binding) = self.declarations.get(&name) {
            return Err(TypeSystemError::Duplicate {
                name: name.clone(),
                existing: existing_binding.span,
                new: binding.span,
            });
        }
        self.declarations.insert(name, binding);
        Ok(())
    }

    /// 添加符号引用
    pub fn reference(&mut self, name: String, span: Span) {
        self.references.entry(name).or_insert_with(Vec::new).push(span);
    }

    /// 查找符号声明
    pub fn lookup(&self, name: &str) -> Option<&Binding> {
        self.declarations.get(name)
    }

    /// 获取符号的所有引用
    pub fn get_references(&self, name: &str) -> Vec<Span> {
        self.references.get(name).cloned().unwrap_or_default()
    }

    /// 路径查找（支持模块路径）
    pub fn path_lookup(&self, path: &Path) -> Option<&Binding> {
        match path {
            Path::Ident(name) => self.lookup(name),
            Path::Chain { base: _, segment } => {
                // 简化实现：暂不支持复杂路径查找
                self.lookup(segment)
            }
            _ => None,
        }
    }

    /// 移除符号
    pub fn remove(&mut self, name: &str) {
        self.declarations.remove(name);
        self.references.remove(name);
    }

    /// 获取所有声明的符号名称
    pub fn declared_symbols(&self) -> Vec<String> {
        self.declarations.keys().cloned().collect()
    }

    /// 获取所有被引用的符号名称
    pub fn referenced_symbols(&self) -> Vec<String> {
        self.references.keys().cloned().collect()
    }

    /// 检查是否有未使用的符号
    pub fn unused_symbols(&self) -> Vec<String> {
        self.declarations.keys()
            .filter(|name| !self.references.contains_key(*name))
            .cloned()
            .collect()
    }

    /// Import symbols from another module with a prefix
    /// This registers all symbols from the imported module with qualified names
    pub fn import_module(&mut self, module_prefix: &str, symbols: &SymbolSpace) -> Result<(), String> {
        for (name, binding) in &symbols.declarations {
            let qualified_name = format!("{}.{}", module_prefix, name);

            // Create a new binding with the qualified name
            let mut imported_binding = binding.clone();
            imported_binding.name = qualified_name.clone();

            // Insert into declarations
            if self.declarations.contains_key(&qualified_name) {
                return Err(format!("duplicate symbol '{}' during import", qualified_name));
            }

            self.declarations.insert(qualified_name, imported_binding);
        }

        Ok(())
    }

    pub fn id(&self) -> SpaceId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Space for SymbolSpace {
    fn id(&self) -> SpaceId {
        self.id
    }

    fn kind(&self) -> SpaceKind {
        SpaceKind::Symbol
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn parent(&self) -> Option<&dyn Space> {
        None
    }

    fn lookup(&self, name: &str) -> Option<&Binding> {
        self.declarations.get(name)
    }

    fn bindings(&self) -> Vec<String> {
        self.declared_symbols()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}