use crate::types::definition::*;
use crate::types::errors::TypeSystemError;
use std::collections::HashMap;

/// Named lifetime parameter (future `'a` syntax; not used for function names).
#[derive(Debug, Clone)]
pub struct LifetimeParam {
    pub name: String,
    pub bounds: Vec<String>,
}

/// Lifetime parameter space for a future `'a`. Borrow checking is `src/types/borrow.rs`.
pub struct LifetimeSpace {
    /// 空间ID
    id: SpaceId,
    /// 空间名称
    name: String,
    /// 生命周期参数
    params: HashMap<String, LifetimeParam>,
    /// 生命周期约束关系
    constraints: HashMap<String, Vec<String>>,
}

impl LifetimeSpace {
    /// 创建新的生命周期空间
    pub fn new(name: impl Into<String>) -> Self {
        LifetimeSpace {
            id: SpaceId::new(),
            name: name.into(),
            params: HashMap::new(),
            constraints: HashMap::new(),
        }
    }

    /// 添加生命周期参数
    pub fn add_param(&mut self, param: LifetimeParam) -> Result<(), TypeSystemError> {
        if self.params.contains_key(&param.name) {
            return Err(TypeSystemError::Duplicate {
                name: param.name.clone(),
                existing: TypeSystemError::span_for_name(&param.name),
                new: TypeSystemError::span_for_name(&param.name),
            });
        }
        self.params.insert(param.name.clone(), param);
        Ok(())
    }

    /// 获取生命周期参数
    pub fn get_param(&self, name: &str) -> Option<&LifetimeParam> {
        self.params.get(name)
    }

    /// 添加约束关系
    pub fn add_constraint(&mut self, shorter: String, longer: String) {
        self.constraints.entry(shorter).or_insert_with(Vec::new).push(longer);
    }

    /// 检查生命周期兼容性
    pub fn is_compatible(&self, shorter: &str, longer: &str) -> bool {
        if shorter == longer {
            return true;
        }

        if let Some(bounds) = self.constraints.get(shorter) {
            for bound in bounds {
                if bound == longer || self.is_compatible(bound, longer) {
                    return true;
                }
            }
        }

        false
    }

    pub fn id(&self) -> SpaceId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Space for LifetimeSpace {
    fn id(&self) -> SpaceId {
        self.id
    }

    fn kind(&self) -> SpaceKind {
        SpaceKind::Lifetime
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn parent(&self) -> Option<&dyn Space> {
        None
    }

    fn lookup(&self, _name: &str) -> Option<&Binding> {
        None
    }

    fn bindings(&self) -> Vec<String> {
        self.params.keys().cloned().collect()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_lifetime_param_span_covers_name() {
        let mut space = LifetimeSpace::new("fn");
        space
            .add_param(LifetimeParam {
                name: "a".to_string(),
                bounds: vec![],
            })
            .unwrap();
        let err = space
            .add_param(LifetimeParam {
                name: "a".to_string(),
                bounds: vec![],
            })
            .unwrap_err();
        match err {
            TypeSystemError::Duplicate { name, existing, new } => {
                assert_eq!(name, "a");
                assert_eq!(existing, TypeSystemError::span_for_name("a"));
                assert_eq!(new, TypeSystemError::span_for_name("a"));
            }
            other => panic!("expected Duplicate, got {other:?}"),
        }
    }
}