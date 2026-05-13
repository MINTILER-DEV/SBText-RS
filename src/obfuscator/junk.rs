use super::context::ObfuscationContext;
use super::names::generate_identifier;
use super::pass::ObfuscationPass;
use anyhow::{anyhow, Result};
use rand::Rng;
use serde_json::{json, Value};
use std::collections::HashSet;

pub struct InjectJunkPass;

impl ObfuscationPass for InjectJunkPass {
    fn name(&self) -> &'static str {
        "inject bait variables"
    }

    fn run(&self, project: &mut Value, ctx: &mut ObfuscationContext) -> Result<()> {
        let targets = project
            .get_mut("targets")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| anyhow!("Invalid project.json: missing 'targets' array."))?;
        let Some(stage) = targets.iter_mut().find(|target| {
            target
                .get("isStage")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        }) else {
            return Ok(());
        };

        let variables = stage
            .get_mut("variables")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| anyhow!("Stage target missing 'variables' object."))?;

        let desired_count = match ctx.level {
            super::config::ObfuscationLevel::Low => 2,
            super::config::ObfuscationLevel::Medium => 4,
            super::config::ObfuscationLevel::High => 6,
            super::config::ObfuscationLevel::Insane => 8,
        };

        let fallback_bait_names = [
            "coins",
            "money",
            "cash",
            "gems",
            "rebirths",
            "admin",
            "debug",
            "real coins",
            "save code",
            "anti cheat",
            "cheatcode",
            "secret",
            "password",
            "pass",
        ];
        let bait_values = [
            json!(0),
            json!(1),
            json!(50),
            json!(100),
            json!("true"),
            json!("false"),
            json!("0"),
            json!("1"),
            json!("disabled"),
            json!("enabled"),
            json!("admin"),
            json!("user"),
            json!("guest"),
            json!("null"),
            json!("NaN"),
            json!("infinity"),
            json!("undefined"),
        ];

        let mut current_names = collect_current_variable_names(variables.values());
        let mut bait_names = collect_dynamic_bait_names(ctx, &current_names);
        bait_names.extend(
            fallback_bait_names
                .into_iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
        );

        let mut added = 0usize;
        let mut cursor = 0usize;
        while added < desired_count && cursor < bait_names.len() {
            let name = reserve_bait_name(&bait_names[cursor], &mut current_names, ctx);
            let id = generate_identifier("var", ctx);
            let value = bait_values[ctx.rng.gen_range(0..bait_values.len())].clone();
            variables.insert(id, Value::Array(vec![Value::String(name), value]));
            added += 1;
            cursor += 1;
        }

        ctx.note_pass(self.name());
        Ok(())
    }
}

fn collect_current_variable_names<'a>(
    variables: impl Iterator<Item = &'a Value>,
) -> HashSet<String> {
    let mut out = HashSet::new();
    for entry in variables {
        if let Some(name) = entry
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(Value::as_str)
        {
            out.insert(name.to_ascii_lowercase());
        }
    }
    out
}

fn collect_dynamic_bait_names(
    ctx: &ObfuscationContext,
    current_names: &HashSet<String>,
) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for name in &ctx.bait_name_candidates {
        let lowered = name.trim().to_ascii_lowercase();
        if lowered.is_empty() || current_names.contains(&lowered) || !seen.insert(lowered) {
            continue;
        }
        out.push(name.clone());
    }
    out
}

fn reserve_bait_name(
    base: &str,
    current_names: &mut HashSet<String>,
    ctx: &mut ObfuscationContext,
) -> String {
    let trimmed = if base.trim().is_empty() {
        "bait"
    } else {
        base.trim()
    };
    let lowered = trimmed.to_ascii_lowercase();
    if !current_names.contains(&lowered) {
        current_names.insert(lowered.clone());
        ctx.used_names.insert(lowered);
        return trimmed.to_string();
    }

    let mut counter = 2usize;
    loop {
        let candidate = format!("{}_{}", trimmed, counter);
        let lowered = candidate.to_ascii_lowercase();
        if !current_names.contains(&lowered) {
            current_names.insert(lowered.clone());
            ctx.used_names.insert(lowered);
            return candidate;
        }
        counter += 1;
    }
}
