use std::collections::BTreeMap;
use std::ffi::CStr;

use anyhow::{bail, Result};

use log::debug;

use memflow::prelude::v1::*;

use pelite::pattern;
use pelite::pe64::{Pe, PeView};

use serde::{Deserialize, Serialize};

use crate::source2::*;

pub type SchemaMap = BTreeMap<String, (Vec<Class>, Vec<Enum>)>;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ClassMetadata {
    Unknown { name: String },
    NetworkChangeCallback { name: String },
    NetworkVarNames { name: String, type_name: String },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Class {
    pub name: String,
    pub module_name: String,
    pub parent: Option<Box<Class>>,
    pub metadata: Vec<ClassMetadata>,
    pub fields: Vec<ClassField>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ClassField {
    pub name: String,
    pub type_name: String,
    pub offset: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Enum {
    pub name: String,
    pub alignment: u8,
    pub size: u16,
    pub members: Vec<EnumMember>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EnumMember {
    pub name: String,
    pub value: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TypeScope {
    pub module_name: String,
    pub classes: Vec<Class>,
    pub enums: Vec<Enum>,
}

pub fn schemas(process: &mut IntoProcessInstanceArcBox<'_>) -> Result<SchemaMap> {
    let schema_system = read_schema_system(process)?;
    let type_scopes = read_type_scopes(process, &schema_system)?;

    let map = type_scopes
        .into_iter()
        .map(|type_scope| {
            (
                type_scope.module_name,
                (type_scope.classes, type_scope.enums),
            )
        })
        .collect();

    Ok(map)
}

fn read_class_binding(
    process: &mut IntoProcessInstanceArcBox<'_>,
    binding_ptr: Pointer64<SchemaClassBinding>,
) -> Result<Class> {
    let binding = process.read_ptr(binding_ptr).data_part()?;

    let module_name = process
        .read_utf8(binding.module_name.address(), 4096)
        .data_part()
        .map(|s| format!("{}.dll", s))?;

    let name = process
        .read_utf8(binding.name.address(), 4096)
        .data_part()?;

    if name.is_empty() {
        bail!("empty class name");
    }

    let parent = binding.base_classes.non_null().and_then(|ptr| {
        let base_class = process.read_ptr(ptr).data_part().ok()?;
        let parent_class = process.read_ptr(base_class.prev).data_part().ok()?;

        let module_name = process
            .read_utf8(parent_class.module_name.address(), 4096)
            .data_part()
            .ok()?;

        let name = process
            .read_utf8(parent_class.name.address(), 4096)
            .data_part()
            .ok()?;

        Some(Box::new(Class {
            name,
            module_name,
            parent: None,
            metadata: Vec::new(),
            fields: Vec::new(),
        }))
    });

    let fields = read_class_binding_fields(process, &binding)?;
    let metadata = read_class_binding_metadata(process, &binding)?;

    debug!(
        "found class: {} at {:#X} (module name: {}) (parent name: {:?}) (metadata count: {}) (field count: {})",
        name,
        binding_ptr.to_umem(),
        module_name,
        parent.as_ref().map(|p| p.name.clone()),
        metadata.len(),
        fields.len(),
    );

    Ok(Class {
        name,
        module_name,
        parent,
        metadata,
        fields,
    })
}

fn read_class_binding_fields(
    process: &mut IntoProcessInstanceArcBox<'_>,
    binding: &SchemaClassBinding,
) -> Result<Vec<ClassField>> {
    if binding.fields.is_null() {
        return Ok(Vec::new());
    }

    (0..binding.field_count).try_fold(Vec::new(), |mut acc, i| {
        let field = process.read_ptr(binding.fields.at(i as _)).data_part()?;

        if field.schema_type.is_null() {
            return Ok(acc);
        }

        let name = process.read_utf8(field.name.address(), 4096).data_part()?;
        let schema_type = process.read_ptr(field.schema_type).data_part()?;

        // TODO: Parse this properly.
        let type_name = process
            .read_utf8(schema_type.name.address(), 4096)
            .data_part()?
            .replace(" ", "");

        acc.push(ClassField {
            name,
            type_name,
            offset: field.single_inheritance_offset,
        });

        Ok(acc)
    })
}

fn read_class_binding_metadata(
    process: &mut IntoProcessInstanceArcBox<'_>,
    binding: &SchemaClassBinding,
) -> Result<Vec<ClassMetadata>> {
    if binding.static_metadata.is_null() {
        return Ok(Vec::new());
    }

    (0..binding.static_metadata_count).try_fold(Vec::new(), |mut acc, i| {
        let metadata = process
            .read_ptr(binding.static_metadata.at(i as _))
            .data_part()?;

        if metadata.network_value.is_null() {
            return Ok(acc);
        }

        let name = process
            .read_utf8(metadata.name.address(), 4096)
            .data_part()?;

        let network_value = process.read_ptr(metadata.network_value).data_part()?;

        let metadata = match name.as_str() {
            "MNetworkChangeCallback" => unsafe {
                let name = process
                    .read_utf8(network_value.value.name_ptr.address(), 4096)
                    .data_part()?;

                ClassMetadata::NetworkChangeCallback { name }
            },
            "MNetworkVarNames" => unsafe {
                let var_value = network_value.value.var_value;

                let name = process
                    .read_utf8(var_value.name.address(), 4096)
                    .data_part()?;

                let type_name = process
                    .read_utf8(var_value.type_name.address(), 4096)
                    .data_part()?
                    .replace(" ", "");

                ClassMetadata::NetworkVarNames { name, type_name }
            },
            _ => ClassMetadata::Unknown { name },
        };

        acc.push(metadata);

        Ok(acc)
    })
}

fn read_enum_binding(
    process: &mut IntoProcessInstanceArcBox<'_>,
    binding_ptr: Pointer64<SchemaEnumBinding>,
) -> Result<Enum> {
    let binding = process.read_ptr(binding_ptr).data_part()?;

    let name = process
        .read_utf8(binding.name.address(), 4096)
        .data_part()?;

    if name.is_empty() {
        bail!("empty enum name");
    }

    let members = read_enum_binding_members(process, &binding)?;

    debug!(
        "found enum: {} at {:#X} (alignment: {}) (member count: {})",
        name,
        binding_ptr.to_umem(),
        binding.align_of,
        binding.size,
    );

    Ok(Enum {
        name,
        alignment: binding.align_of,
        size: binding.enumerator_count,
        members,
    })
}

fn read_enum_binding_members(
    process: &mut IntoProcessInstanceArcBox<'_>,
    binding: &SchemaEnumBinding,
) -> Result<Vec<EnumMember>> {
    if binding.enumerators.is_null() {
        return Ok(Vec::new());
    }

    (0..binding.enumerator_count).try_fold(Vec::new(), |mut acc, i| {
        let enumerator = process
            .read_ptr(binding.enumerators.at(i as _))
            .data_part()?;

        let name = process
            .read_utf8(enumerator.name.address(), 4096)
            .data_part()?;

        acc.push(EnumMember {
            name,
            value: unsafe { enumerator.value.ulong } as i64,
        });

        Ok(acc)
    })
}

fn read_schema_system(process: &mut IntoProcessInstanceArcBox<'_>) -> Result<SchemaSystem> {
    let module = process.module_by_name("schemasystem.dll")?;

    let buf = process
        .read_raw(module.base, module.size as _)
        .data_part()?;

    let view = PeView::from_bytes(&buf)?;

    let mut save = [0; 2];

    if !view
        .scanner()
        .finds_code(pattern!("4c8d35${'} 0f2845"), &mut save)
    {
        bail!("outdated schema system pattern");
    }

    let schema_system: SchemaSystem = process.read(module.base + save[1]).data_part()?;

    if schema_system.num_registrations == 0 {
        bail!("no schema system registrations found");
    }

    Ok(schema_system)
}

fn read_type_scopes(
    process: &mut IntoProcessInstanceArcBox<'_>,
    schema_system: &SchemaSystem,
) -> Result<Vec<TypeScope>> {
    let type_scopes = &schema_system.type_scopes;

    (0..type_scopes.size).try_fold(Vec::new(), |mut acc, i| {
        let type_scope_ptr = type_scopes.element(process, i as _)?;
        let type_scope = process.read_ptr(type_scope_ptr).data_part()?;

        let module_name = unsafe { CStr::from_ptr(type_scope.name.as_ptr()) }
            .to_string_lossy()
            .to_string();

        let classes: Vec<_> = type_scope
            .class_bindings
            .elements(process)?
            .iter()
            .filter_map(|ptr| read_class_binding(process, *ptr).ok())
            .collect();

        let enums: Vec<_> = type_scope
            .enum_bindings
            .elements(process)?
            .iter()
            .filter_map(|ptr| read_enum_binding(process, *ptr).ok())
            .collect();

        if classes.is_empty() && enums.is_empty() {
            return Ok(acc);
        }

        debug!(
            "found type scope: {} at {:#X} (class count: {}) (enum count: {})",
            module_name,
            type_scope_ptr.to_umem(),
            classes.len(),
            enums.len(),
        );

        acc.push(TypeScope {
            module_name,
            classes,
            enums,
        });

        Ok(acc)
    })
}
