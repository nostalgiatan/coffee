use crate::types::definition::*;
use crate::types::errors::TypeSystemError;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, RwLock};

/// 作用域空间：管理变量的可见性和作用域层级
///
/// 特性：
/// - 层级化：子作用域可以访问父作用域的绑定
/// - 影子化：子作用域可以重新定义父作用域的名称
/// - 并发安全：使用RwLock支持并发读取
pub struct ScopeSpace {
    /// 空间ID
    id: SpaceId,
    /// 空间名称
    name: String,
    /// 父作用域
    parent: Option<Arc<RwLock<ScopeSpace>>>,
    /// 本地绑定
    bindings: Arc<RwLock<HashMap<String, Binding>>>,
    /// 子作用域
    children: Arc<RwLock<Vec<Arc<ScopeSpace>>>>,
    /// 作用域深度（根作用域为0）
    depth: usize,
}

impl ScopeSpace {
    /// 创建根作用域
    pub fn root() -> Self {
        ScopeSpace {
            id: SpaceId::new(),
            name: "root".to_string(),
            parent: None,
            bindings: Arc::new(RwLock::new(HashMap::new())),
            children: Arc::new(RwLock::new(Vec::new())),
            depth: 0,
        }
    }

    /// 创建命名作用域
    pub fn new(name: impl Into<String>) -> Self {
        ScopeSpace {
            id: SpaceId::new(),
            name: name.into(),
            parent: None,
            bindings: Arc::new(RwLock::new(HashMap::new())),
            children: Arc::new(RwLock::new(Vec::new())),
            depth: 0,
        }
    }

    /// 创建子作用域
    pub fn child(&self, name: impl Into<String>) -> Arc<ScopeSpace> {
        let child = Arc::new(ScopeSpace {
            id: SpaceId::new(),
            name: name.into(),
            parent: Some(Arc::new(RwLock::new(self.clone()))),
            bindings: Arc::new(RwLock::new(HashMap::new())),
            children: Arc::new(RwLock::new(Vec::new())),
            depth: self.depth + 1,
        });

        // 添加到子作用域列表
        if let Ok(mut children) = self.children.write() {
            children.push(Arc::clone(&child));
        }

        child
    }

    /// 插入绑定
    pub fn bind(&self, name: impl Into<String>, entity: Entity, span: Span) -> Result<(), TypeSystemError> {
        let name = name.into();
        let binding = Binding {
            name: name.clone(),
            entity,
            span,
            mutable: false,
            visibility: Visibility::Private,
        };

        if let Ok(mut bindings) = self.bindings.write() {
            if let Some(existing_binding) = bindings.get(&name) {
                // Found existing definition - use its span
                return Err(TypeSystemError::Duplicate {
                    name,
                    existing: existing_binding.span,
                    new: span,
                });
            }
            bindings.insert(name, binding);
            Ok(())
        } else {
            Err(TypeSystemError::NotFound {
                name: format!("binding '{}'", name),
                kind: SpaceKind::Scope,
            })
        }
    }

    /// 插入可变绑定
    pub fn bind_mut(&self, name: impl Into<String>, entity: Entity, span: Span) -> Result<(), TypeSystemError> {
        let name = name.into();
        let binding = Binding {
            name: name.clone(),
            entity,
            span,
            mutable: true,
            visibility: Visibility::Private,
        };

        if let Ok(mut bindings) = self.bindings.write() {
            if let Some(existing_binding) = bindings.get(&name) {
                // Found existing definition - use its span
                return Err(TypeSystemError::Duplicate {
                    name,
                    existing: existing_binding.span,
                    new: span,
                });
            }
            bindings.insert(name, binding);
            Ok(())
        } else {
            Err(TypeSystemError::NotFound {
                name: format!("binding '{}'", name),
                kind: SpaceKind::Scope,
            })
        }
    }

    /// 插入公开绑定
    pub fn bind_public(&self, name: impl Into<String>, entity: Entity, span: Span) -> Result<(), TypeSystemError> {
        let name = name.into();
        let binding = Binding {
            name: name.clone(),
            entity,
            span,
            mutable: false,
            visibility: Visibility::Public,
        };

        if let Ok(mut bindings) = self.bindings.write() {
            if let Some(existing_binding) = bindings.get(&name) {
                // Found existing definition - use its span
                return Err(TypeSystemError::Duplicate {
                    name,
                    existing: existing_binding.span,
                    new: span,
                });
            }
            bindings.insert(name, binding);
            Ok(())
        } else {
            Err(TypeSystemError::NotFound {
                name: format!("binding '{}'", name),
                kind: SpaceKind::Scope,
            })
        }
    }

    /// 更新现有绑定
    pub fn rebind(&self, name: &str, entity: Entity) -> Result<(), TypeSystemError> {
        if let Ok(mut bindings) = self.bindings.write() {
            if let Some(binding) = bindings.get_mut(name) {
                binding.entity = entity;
                Ok(())
            } else {
                Err(TypeSystemError::NotFound {
                    name: name.to_string(),
                    kind: SpaceKind::Scope,
                })
            }
        } else {
            Err(TypeSystemError::NotFound {
                name: name.to_string(),
                kind: SpaceKind::Scope,
            })
        }
    }

    /// 查找绑定（只在当前作用域）
    pub fn get_local(&self, name: &str) -> Option<Binding> {
        if let Ok(bindings) = self.bindings.read() {
            bindings.get(name).cloned()
        } else {
            None
        }
    }

    /// 查找绑定（包括父作用域链）
    pub fn get(&self, name: &str) -> Option<Binding> {
        // 先检查本地
        if let Some(binding) = self.get_local(name) {
            return Some(binding);
        }

        // 递归检查父作用域
        if let Some(ref parent) = self.parent {
            if let Ok(parent_lock) = parent.read() {
                return parent_lock.get(name);
            }
        }

        None
    }

    /// 获取作用域深度
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// Whether this is the root scope
    pub fn is_root(&self) -> bool {
        self.parent.is_none()
    }

    /// Get parent scope ID
    pub fn parent_id(&self) -> Option<SpaceId> {
        if let Some(ref parent_lock) = self.parent {
            if let Ok(parent) = parent_lock.read() {
                return Some(parent.id);
            }
        }
        None
    }

    /// Get all child scopes
    pub fn children(&self) -> Vec<Arc<ScopeSpace>> {
        if let Ok(children) = self.children.read() {
            children.clone()
        } else {
            Vec::new()
        }
    }

    /// 获取当前作用域的所有绑定名称
    pub fn local_names(&self) -> Vec<String> {
        if let Ok(bindings) = self.bindings.read() {
            bindings.keys().cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// 获取所有绑定的名称（包括父作用域）
    pub fn all_names(&self) -> Vec<String> {
        let mut names = self.local_names();

        if let Some(ref parent) = self.parent {
            if let Ok(parent_lock) = parent.read() {
                names.extend(parent_lock.all_names());
            }
        }

        names
    }

    /// 检查名称是否在当前作用域中定义
    pub fn is_defined_locally(&self, name: &str) -> bool {
        self.get_local(name).is_some()
    }

    /// 检查名称是否在任何作用域中定义
    pub fn is_defined(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// 导出为Region结构
    pub fn to_region(&self) -> Region {
        let mut region = Region::new(&self.name);
        if let Ok(bindings) = self.bindings.read() {
            for (name, binding) in bindings.iter() {
                region.bindings.insert(name.clone(), binding.clone());
            }
        }
        if let Ok(children) = self.children.read() {
            for child in children.iter() {
                region.children.push(child.to_region());
            }
        }
        region
    }

    /// 合并另一个作用域的绑定
    pub fn merge(&self, other: &ScopeSpace) -> Result<(), TypeSystemError> {
        if let Ok(other_bindings) = other.bindings.read() {
            if let Ok(mut bindings) = self.bindings.write() {
                for (name, binding) in other_bindings.iter() {
                    if bindings.contains_key(name) {
                        return Err(TypeSystemError::Duplicate {
                            name: name.clone(),
                            existing: binding.span,
                            new: binding.span,
                        });
                    }
                    bindings.insert(name.clone(), binding.clone());
                }
            }
        }
        Ok(())
    }

    /// 检查可见性权限
    pub fn check_visibility(&self, name: &str, required_visibility: Visibility) -> Result<(), TypeSystemError> {
        if let Some(binding) = self.get(name) {
            match (binding.visibility, required_visibility) {
                (Visibility::Public, _) => Ok(()),
                (Visibility::Protected, Visibility::Protected) | (Visibility::Protected, Visibility::Private) => Ok(()),
                (Visibility::Private, Visibility::Private) => Ok(()),
                _ => Err(TypeSystemError::VisibilityError {
                    name: name.to_string(),
                    required: format!("{:?}", required_visibility),
                    actual: format!("{:?}", binding.visibility),
                })
            }
        } else {
            Err(TypeSystemError::NotFound {
                name: name.to_string(),
                kind: SpaceKind::Scope,
            })
        }
    }

    /// 获取空间ID
    pub fn id(&self) -> SpaceId {
        self.id
    }

    /// 获取空间名称
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Clone for ScopeSpace {
    fn clone(&self) -> Self {
        ScopeSpace {
            id: self.id,
            name: self.name.clone(),
            parent: self.parent.clone(),
            bindings: Arc::clone(&self.bindings),
            children: Arc::clone(&self.children),
            depth: self.depth,
        }
    }
}

impl Space for ScopeSpace {
    fn id(&self) -> SpaceId {
        self.id
    }

    fn kind(&self) -> SpaceKind {
        SpaceKind::Scope
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn parent(&self) -> Option<&dyn Space> {
        // This is a simplified implementation
        // In a full implementation, this would require more complex lifetime management
        None
    }

    fn lookup(&self, _name: &str) -> Option<&Binding> {
        // This is a simplified implementation
        // The actual implementation would need to handle the borrowing correctly
        None
    }

    fn bindings(&self) -> Vec<String> {
        self.all_names()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl fmt::Debug for ScopeSpace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScopeSpace")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("depth", &self.depth)
            .field("bindings", &self.local_names())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
use crate::types::definition::Type;

    #[test]
    fn test_scope_creation() {
        let root = ScopeSpace::root();
        assert_eq!(root.name(), "root");
        assert_eq!(root.depth(), 0);
        assert!(root.is_root());
    }

    #[test]
    fn test_child_scope() {
        let root = ScopeSpace::root();
        let child = root.child("function");

        assert_eq!(child.name(), "function");
        assert_eq!(child.depth(), 1);
        assert!(!child.is_root());
        assert_eq!(child.parent_id(), Some(root.id()));
    }

    #[test]
    fn test_binding() {
        let scope = ScopeSpace::new("test");

        let entity = Entity::Variable {
            ty: Type::int(),
            initialized: true,
        };

        scope.bind("x", entity, Span::new(0, 1)).unwrap();

        assert!(scope.is_defined_locally("x"));
        assert!(scope.is_defined("x"));

        let binding = scope.get_local("x").unwrap();
        assert_eq!(binding.name, "x");
        assert_eq!(binding.visibility, Visibility::Private);
    }

    #[test]
    fn test_scope_lookup_hierarchy() {
        let root = ScopeSpace::root();
        let child = root.child("inner");

        // 在根作用域绑定变量
        let entity = Entity::Variable {
            ty: Type::int(),
            initialized: true,
        };
        root.bind("global", entity.clone(), Span::new(0, 1)).unwrap();

        // 在子作用域绑定变量
        child.bind("local", entity, Span::new(0, 1)).unwrap();

        // 子作用域可以访问父作用域的变量
        assert!(child.is_defined("global"));
        assert!(child.is_defined("local"));

        // 父作用域无法访问子作用域的变量
        assert!(!root.is_defined("local"));
        assert!(root.is_defined("global"));
    }

    #[test]
    fn test_scope_shadowing() {
        let root = ScopeSpace::root();
        let child = root.child("inner");

        let entity1 = Entity::Variable {
            ty: Type::int(),
            initialized: true,
        };
        let entity2 = Entity::Variable {
            ty: Type::bool(),
            initialized: true,
        };

        // 在根作用域绑定变量
        root.bind("x", entity1, Span::new(0, 1)).unwrap();

        // 在子作用域绑定同名变量（影子化）
        child.bind("x", entity2, Span::new(0, 1)).unwrap();

        // 子作用域中的绑定应该覆盖父作用域
        let binding = child.get("x").unwrap();
        if let Entity::Variable { ty, .. } = binding.entity {
            assert_eq!(ty, Type::bool());
        } else {
            panic!("Expected variable entity");
        }

        // 根作用域的绑定不应该被影响
        let binding = root.get("x").unwrap();
        if let Entity::Variable { ty, .. } = binding.entity {
            assert_eq!(ty, Type::int());
        } else {
            panic!("Expected variable entity");
        }
    }
}