use super::context::{ObfuscationContext, ProtectedVariableMetadata};
use super::names::{generate_identifier, generate_name, NameKind};
use super::pass::ObfuscationPass;
use super::rename::rename_variable_references;
use anyhow::{anyhow, Result};
use rand::Rng;
use serde_json::{json, Number, Value};

pub struct ProtectVariablesPass;

impl ObfuscationPass for ProtectVariablesPass {
    fn name(&self) -> &'static str {
        "protect economy variables"
    }

    fn run(&self, project: &mut Value, ctx: &mut ObfuscationContext) -> Result<()> {
        if ctx.protected_variable_names.is_empty() {
            return Ok(());
        }

        let mut matches = Vec::new();
        let targets = project
            .get("targets")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("Invalid project.json: missing 'targets' array."))?;
        for (target_index, target) in targets.iter().enumerate() {
            let target_name = target
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Target")
                .to_string();
            let Some(variables) = target.get("variables").and_then(Value::as_object) else {
                continue;
            };
            for (var_id, entry) in variables {
                let Some(arr) = entry.as_array() else {
                    continue;
                };
                let Some(name) = arr.first().and_then(Value::as_str) else {
                    continue;
                };
                if ctx.is_protected_variable_name(name) {
                    matches.push((
                        target_index,
                        target_name.clone(),
                        var_id.clone(),
                        name.to_string(),
                        arr.get(1)
                            .cloned()
                            .unwrap_or(Value::Number(Number::from(0))),
                    ));
                }
            }
        }

        if matches.is_empty() {
            ctx.push_warning("protect economy: no matching variables were found");
            return Ok(());
        }

        ctx.push_warning(
            "protect economy: full mutation rewriting not implemented; applied rename+bait+checksum metadata",
        );

        for (target_index, target_name, var_id, original_name, initial_value) in matches {
            let real_name = generate_name(NameKind::Variable, ctx);
            let checksum_name = generate_name(NameKind::Checksum, ctx);
            let fake_id = generate_identifier("var", ctx);
            let checksum_id = generate_identifier("var", ctx);
            let checksum_secret = ctx.rng.gen_range(1000i64..999_983i64);
            let checksum_value = checksum_for_value(&initial_value, checksum_secret);

            {
                let targets = project
                    .get_mut("targets")
                    .and_then(Value::as_array_mut)
                    .ok_or_else(|| anyhow!("Invalid project.json: missing 'targets' array."))?;
                let target = targets.get_mut(target_index).ok_or_else(|| {
                    anyhow!(
                        "Invalid target index {} while protecting variable.",
                        target_index
                    )
                })?;
                let variables = target
                    .get_mut("variables")
                    .and_then(Value::as_object_mut)
                    .ok_or_else(|| anyhow!("Target '{}' missing variables.", target_name))?;
                let entry = variables.get_mut(&var_id).ok_or_else(|| {
                    anyhow!(
                        "Variable '{}' disappeared while protecting target '{}'.",
                        original_name,
                        target_name
                    )
                })?;
                let arr = entry.as_array_mut().ok_or_else(|| {
                    anyhow!(
                        "Variable '{}' in target '{}' has invalid shape.",
                        original_name,
                        target_name
                    )
                })?;
                if let Some(name_slot) = arr.first_mut() {
                    *name_slot = Value::String(real_name.clone());
                }
                variables.insert(
                    fake_id.clone(),
                    Value::Array(vec![
                        Value::String(original_name.clone()),
                        fake_value(&original_name),
                    ]),
                );
                variables.insert(
                    checksum_id.clone(),
                    Value::Array(vec![Value::String(checksum_name.clone()), checksum_value]),
                );
            }

            rename_variable_references(project, &var_id, &real_name)?;

            ctx.metadata.push(ProtectedVariableMetadata {
                target_name,
                original_name,
                real_name,
                real_id: var_id,
                fake_id,
                checksum_name,
                checksum_id,
                checksum_secret,
            });
        }

        write_metadata(project, ctx)?;
        ctx.note_pass(self.name());
        Ok(())
    }
}

fn checksum_for_value(value: &Value, secret: i64) -> Value {
    let raw = match value {
        Value::Number(num) => num
            .as_i64()
            .or_else(|| num.as_u64().map(|n| n as i64))
            .or_else(|| num.as_f64().map(|n| n.round() as i64))
            .unwrap_or(0),
        Value::String(text) => text.parse::<i64>().unwrap_or(0),
        Value::Bool(flag) => i64::from(*flag),
        _ => 0,
    };
    json!(((raw * 37) + secret).rem_euclid(999_983))
}

fn fake_value(original_name: &str) -> Value {
    let lower = original_name.to_ascii_lowercase();
    if lower.contains("admin") || lower.contains("debug") {
        json!("true")
    } else if lower.contains("save") {
        json!("DEBUG_ENABLED")
    } else {
        json!(999999999)
    }
}

fn write_metadata(project: &mut Value, ctx: &ObfuscationContext) -> Result<()> {
    let meta = project
        .get_mut("meta")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| anyhow!("Invalid project.json: missing 'meta' object."))?;
    let protected_variables = ctx
        .metadata
        .iter()
        .map(|entry| {
            json!({
                "target": entry.target_name,
                "originalName": entry.original_name,
                "realName": entry.real_name,
                "realId": entry.real_id,
                "fakeId": entry.fake_id,
                "checksumName": entry.checksum_name,
                "checksumId": entry.checksum_id,
                "checksumSecret": entry.checksum_secret,
            })
        })
        .collect::<Vec<_>>();

    meta.insert(
        "sbtextObfuscation".to_string(),
        json!({
            "seed": ctx.seed,
            "mode": "mvp-light",
            "protectedVariables": protected_variables,
        }),
    );
    Ok(())
}
