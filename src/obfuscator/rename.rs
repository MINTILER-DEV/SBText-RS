use super::context::ObfuscationContext;
use super::names::{generate_name, rewrite_proccode_name, NameKind};
use super::pass::ObfuscationPass;
use anyhow::{anyhow, Result};
use serde_json::{Map, Value};
use std::collections::HashMap;

pub struct RenamePass;

impl ObfuscationPass for RenamePass {
    fn name(&self) -> &'static str {
        "rename variables/lists/broadcasts/procedures"
    }

    fn run(&self, project: &mut Value, ctx: &mut ObfuscationContext) -> Result<()> {
        let targets = project_targets(project)?;

        let mut variable_names = HashMap::new();
        let mut list_names = HashMap::new();
        let mut broadcast_names = HashMap::new();
        let mut procedure_names = HashMap::new();

        for target in targets.iter() {
            if let Some(vars) = target.get("variables").and_then(Value::as_object) {
                for (var_id, entry) in vars {
                    let Some(name) = variable_entry_name(entry) else {
                        continue;
                    };
                    ctx.original_variable_names
                        .entry(var_id.clone())
                        .or_insert_with(|| name.to_string());
                    if ctx.is_protected_variable_name(name) {
                        continue;
                    }
                    ctx.push_bait_name_candidate(name);
                    variable_names
                        .entry(var_id.clone())
                        .or_insert_with(|| generate_name(NameKind::Variable, ctx));
                }
            }

            if let Some(lists) = target.get("lists").and_then(Value::as_object) {
                for list_id in lists.keys() {
                    list_names
                        .entry(list_id.clone())
                        .or_insert_with(|| generate_name(NameKind::List, ctx));
                }
            }

            if let Some(broadcasts) = target.get("broadcasts").and_then(Value::as_object) {
                for broadcast_id in broadcasts.keys() {
                    broadcast_names
                        .entry(broadcast_id.clone())
                        .or_insert_with(|| generate_name(NameKind::Broadcast, ctx));
                }
            }

            if let Some(blocks) = target.get("blocks").and_then(Value::as_object) {
                for block in blocks.values() {
                    let Some(mutation) = block.get("mutation").and_then(Value::as_object) else {
                        continue;
                    };
                    let Some(old_proccode) = mutation.get("proccode").and_then(Value::as_str)
                    else {
                        continue;
                    };
                    procedure_names
                        .entry(old_proccode.to_string())
                        .or_insert_with(|| {
                            let new_name = generate_name(NameKind::Procedure, ctx);
                            rewrite_proccode_name(old_proccode, &new_name)
                        });
                }
            }
        }

        let targets = project_targets_mut(project)?;
        for target in targets.iter_mut() {
            rename_named_table(target, "variables", &variable_names)?;
            rename_named_table(target, "lists", &list_names)?;
            rename_broadcast_table(target, &broadcast_names)?;
            rename_block_fields(target, &variable_names, &list_names, &broadcast_names)?;
            rename_procedure_mutations(target, &procedure_names)?;
        }
        rename_monitors(project, &variable_names, &list_names)?;

        ctx.note_pass(self.name());
        Ok(())
    }
}

pub(crate) fn rename_variable_references(
    project: &mut Value,
    variable_id: &str,
    new_name: &str,
) -> Result<()> {
    let mut id_map = HashMap::new();
    id_map.insert(variable_id.to_string(), new_name.to_string());
    let targets = project_targets_mut(project)?;
    for target in targets {
        rename_block_fields(target, &id_map, &HashMap::new(), &HashMap::new())?;
    }
    rename_monitors(project, &id_map, &HashMap::new())
}

fn project_targets(project: &Value) -> Result<&Vec<Value>> {
    project
        .get("targets")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("Invalid project.json: missing 'targets' array."))
}

fn project_targets_mut(project: &mut Value) -> Result<&mut Vec<Value>> {
    project
        .get_mut("targets")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow!("Invalid project.json: missing 'targets' array."))
}

fn variable_entry_name(entry: &Value) -> Option<&str> {
    entry.as_array()?.first()?.as_str()
}

fn rename_named_table(
    target: &mut Value,
    key: &str,
    names: &HashMap<String, String>,
) -> Result<()> {
    let Some(entries) = target.get_mut(key).and_then(Value::as_object_mut) else {
        return Ok(());
    };
    for (entry_id, value) in entries {
        let Some(new_name) = names.get(entry_id) else {
            continue;
        };
        let Some(arr) = value.as_array_mut() else {
            continue;
        };
        if let Some(slot) = arr.first_mut() {
            *slot = Value::String(new_name.clone());
        }
    }
    Ok(())
}

fn rename_broadcast_table(target: &mut Value, names: &HashMap<String, String>) -> Result<()> {
    let Some(entries) = target.get_mut("broadcasts").and_then(Value::as_object_mut) else {
        return Ok(());
    };
    for (broadcast_id, value) in entries {
        let Some(new_name) = names.get(broadcast_id) else {
            continue;
        };
        *value = Value::String(new_name.clone());
    }
    Ok(())
}

fn rename_block_fields(
    target: &mut Value,
    variable_names: &HashMap<String, String>,
    list_names: &HashMap<String, String>,
    broadcast_names: &HashMap<String, String>,
) -> Result<()> {
    let Some(blocks) = target.get_mut("blocks").and_then(Value::as_object_mut) else {
        return Ok(());
    };

    for block in blocks.values_mut() {
        let Some(fields) = block.get_mut("fields").and_then(Value::as_object_mut) else {
            continue;
        };
        update_named_field(fields, "VARIABLE", variable_names);
        update_named_field(fields, "LIST", list_names);
        update_named_field(fields, "BROADCAST_OPTION", broadcast_names);
    }

    Ok(())
}

fn update_named_field(fields: &mut Map<String, Value>, key: &str, names: &HashMap<String, String>) {
    let Some(field_value) = fields.get_mut(key) else {
        return;
    };
    let Some(arr) = field_value.as_array_mut() else {
        return;
    };
    if arr.len() < 2 {
        return;
    }
    let Some(entry_id) = arr[1].as_str() else {
        return;
    };
    let Some(new_name) = names.get(entry_id) else {
        return;
    };
    arr[0] = Value::String(new_name.clone());
}

fn rename_procedure_mutations(target: &mut Value, names: &HashMap<String, String>) -> Result<()> {
    let Some(blocks) = target.get_mut("blocks").and_then(Value::as_object_mut) else {
        return Ok(());
    };
    for block in blocks.values_mut() {
        let Some(mutation) = block.get_mut("mutation").and_then(Value::as_object_mut) else {
            continue;
        };
        let Some(old_proccode) = mutation.get("proccode").and_then(Value::as_str) else {
            continue;
        };
        let Some(new_proccode) = names.get(old_proccode) else {
            continue;
        };
        mutation.insert("proccode".to_string(), Value::String(new_proccode.clone()));
    }
    Ok(())
}

fn rename_monitors(
    project: &mut Value,
    variable_names: &HashMap<String, String>,
    list_names: &HashMap<String, String>,
) -> Result<()> {
    let Some(monitors) = project.get_mut("monitors").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    for monitor in monitors {
        let Some(obj) = monitor.as_object_mut() else {
            continue;
        };
        let monitor_id = obj
            .get("id")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        if let Some(id) = monitor_id.as_deref() {
            if let Some(new_name) = variable_names.get(id) {
                rename_monitor_param(obj, "VARIABLE", new_name);
            }
            if let Some(new_name) = list_names.get(id) {
                rename_monitor_param(obj, "LIST", new_name);
            }
        }
    }
    Ok(())
}

fn rename_monitor_param(obj: &mut Map<String, Value>, key: &str, new_name: &str) {
    let Some(params) = obj.get_mut("params").and_then(Value::as_object_mut) else {
        return;
    };
    if params.contains_key(key) {
        params.insert(key.to_string(), Value::String(new_name.to_string()));
    }
}
