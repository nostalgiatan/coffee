use crate::types::definition::*;
use crate::types::errors::TypeSystemError;
use std::collections::HashMap;

/// 生命周期参数
#[derive(Debug, Clone)]
pub struct LifetimeParam {
    pub name: String,
    pub bounds: Vec<String>,
}

/// 生命周期空间：管理引用的有效性和生命周期约束
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
                existing: Span::new(0, 0),
                new: Span::new(0, 0),
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