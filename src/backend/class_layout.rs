//! Class field flattening and compile order (declaration ABI).
//!
//! Default LLVM members are ancestors (root first) then the class's own fields.
//! With `--enable-bitfields`, consecutive bitfields share storage-unit integers.

use crate::backend::memory::bitfields::{BitFieldLayout, BitFieldSpec};
use crate::parser::class::{ClassDef, ClassField};
use std::collections::{HashMap, HashSet};

/// GEP index plus optional bit-slice inside a packed storage unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackedFieldInfo {
    pub gep_index: usize,
    pub bit_offset: u8,
    /// Zero means the LLVM member is the whole field (not a bit slice).
    pub bit_width: u8,
    pub storage_bits: u8,
}

/// One LLVM `set_body` member: a normal field or a packed bitfield unit.
#[derive(Debug, Clone)]
pub enum LlvmStructMember {
    Regular(ClassField),
    BitStorage {
        bits: u8,
        fields: Vec<(String, u8, u8)>,
    },
}

/// Consecutive `bit_width` fields share the smallest 8/16/32/64-bit unit that fits.
pub fn packed_llvm_members(fields: &[ClassField]) -> Vec<LlvmStructMember> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < fields.len() {
        if fields[i].bit_width.is_some() {
            let mut specs = Vec::new();
            while i < fields.len() {
                if let Some(width) = fields[i].bit_width {
                    specs.push(BitFieldSpec {
                        name: fields[i].name.clone(),
                        width,
                    });
                    i += 1;
                } else {
                    break;
                }
            }
            for layout in BitFieldLayout::calculate_units(&specs) {
                out.push(LlvmStructMember::BitStorage {
                    bits: layout.storage_type.bits(),
                    fields: layout
                        .fields
                        .iter()
                        .map(|f| (f.name.clone(), f.offset, f.width))
                        .collect(),
                });
            }
        } else {
            out.push(LlvmStructMember::Regular(fields[i].clone()));
            i += 1;
        }
    }
    out
}

pub fn packed_field_info_map(fields: &[ClassField]) -> HashMap<String, PackedFieldInfo> {
    let mut map = HashMap::new();
    for (gep, member) in packed_llvm_members(fields).into_iter().enumerate() {
        match member {
            LlvmStructMember::Regular(f) => {
                map.insert(
                    f.name,
                    PackedFieldInfo {
                        gep_index: gep,
                        bit_offset: 0,
                        bit_width: 0,
                        storage_bits: 0,
                    },
                );
            }
            LlvmStructMember::BitStorage { bits, fields } => {
                for (name, offset, width) in fields {
                    map.insert(
                        name,
                        PackedFieldInfo {
                            gep_index: gep,
                            bit_offset: offset,
                            bit_width: width,
                            storage_bits: bits,
                        },
                    );
                }
            }
        }
    }
    map
}

/// Ancestor fields (root → parent) followed by `class.fields`.
pub fn flatten_class_fields(
    class: &ClassDef,
    all: &HashMap<String, ClassDef>,
) -> Vec<ClassField> {
    let mut out = Vec::new();
    let mut stack = HashSet::new();
    flatten_into(class, all, &mut out, &mut stack);
    out
}

fn flatten_into(
    class: &ClassDef,
    all: &HashMap<String, ClassDef>,
    out: &mut Vec<ClassField>,
    stack: &mut HashSet<String>,
) {
    if !stack.insert(class.name.clone()) {
        return;
    }
    if let Some(ref parent_name) = class.parent {
        if let Some(parent) = all.get(parent_name) {
            flatten_into(parent, all, out, stack);
        } else if parent_name == "Error" {
            let error = crate::backend::error::builtin_error_class_def();
            flatten_into(&error, all, out, stack);
        }
    }
    out.extend(class.fields.iter().cloned());
    stack.remove(&class.name);
}

/// Parents before children. Classes whose parent is missing still compile.
pub fn class_compile_order(classes: &[ClassDef]) -> Result<Vec<ClassDef>, String> {
    let by_name: HashMap<String, ClassDef> = classes
        .iter()
        .map(|c| (c.name.clone(), c.clone()))
        .collect();
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    let mut out = Vec::new();

    fn visit(
        name: &str,
        by_name: &HashMap<String, ClassDef>,
        visiting: &mut HashSet<String>,
        visited: &mut HashSet<String>,
        out: &mut Vec<ClassDef>,
    ) -> Result<(), String> {
        if visited.contains(name) {
            return Ok(());
        }
        if !visiting.insert(name.to_string()) {
            return Err(format!("cyclic class inheritance involving '{}'", name));
        }
        if let Some(class) = by_name.get(name) {
            if let Some(ref parent) = class.parent {
                if by_name.contains_key(parent) {
                    visit(parent, by_name, visiting, visited, out)?;
                }
            }
            visiting.remove(name);
            visited.insert(name.to_string());
            out.push(class.clone());
        } else {
            visiting.remove(name);
        }
        Ok(())
    }

    for class in classes {
        visit(&class.name, &by_name, &mut visiting, &mut visited, &mut out)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::class::ClassField;

    fn field(name: &str, ty: &str) -> ClassField {
        ClassField {
            name: name.to_string(),
            field_type: ty.to_string(),
            bit_width: None,
        }
    }

    fn class(name: &str, parent: Option<&str>, fields: Vec<ClassField>) -> ClassDef {
        ClassDef {
            type_params: vec![],
            name: name.to_string(),
            parent: parent.map(|s| s.to_string()),
            fields,
            methods: vec![],
            packed: false,
            has_constructor: false,
        }
    }

    #[test]
    fn flatten_grandparent() {
        let a = class("A", None, vec![field("a", "int")]);
        let b = class("B", Some("A"), vec![field("b", "int")]);
        let c = class("C", Some("B"), vec![field("c", "int")]);
        let mut all = HashMap::new();
        all.insert("A".into(), a);
        all.insert("B".into(), b.clone());
        all.insert("C".into(), c.clone());
        let names: Vec<_> = flatten_class_fields(&c, &all)
            .into_iter()
            .map(|f| f.name)
            .collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[test]
    fn compile_child_before_parent_in_list() {
        let parent = class("Base", None, vec![field("x", "int")]);
        let child = class("Child", Some("Base"), vec![field("y", "int")]);
        let order = class_compile_order(&[child, parent]).unwrap();
        assert_eq!(
            order.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            vec!["Base", "Child"]
        );
    }

    fn bits(name: &str, width: u8) -> ClassField {
        ClassField {
            name: name.to_string(),
            field_type: "int".to_string(),
            bit_width: Some(width),
        }
    }

    #[test]
    fn pack_consecutive_bitfields_one_unit_then_regular() {
        let fields = vec![bits("lo", 3), bits("hi", 5), field("x", "int")];
        let members = packed_llvm_members(&fields);
        assert_eq!(members.len(), 2);
        match &members[0] {
            LlvmStructMember::BitStorage { bits, fields } => {
                assert_eq!(*bits, 8);
                assert_eq!(fields.len(), 2);
            }
            _ => panic!("expected storage unit"),
        }
        match &members[1] {
            LlvmStructMember::Regular(f) => assert_eq!(f.name, "x"),
            _ => panic!("expected regular x"),
        }
        let info = packed_field_info_map(&fields);
        assert_eq!(info["lo"].gep_index, 0);
        assert_eq!(info["hi"].gep_index, 0);
        assert_eq!(info["x"].gep_index, 1);
        assert_eq!(info["hi"].bit_offset, 3);
    }
}
