use super::context::ObfuscationContext;
use super::pass::ObfuscationPass;
use anyhow::{anyhow, Result};
use rand::Rng;
use serde_json::{Number, Value};

pub struct ScrambleLayoutPass;

impl ObfuscationPass for ScrambleLayoutPass {
    fn name(&self) -> &'static str {
        "scramble layout"
    }

    fn run(&self, project: &mut Value, ctx: &mut ObfuscationContext) -> Result<()> {
        let targets = project
            .get_mut("targets")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| anyhow!("Invalid project.json: missing 'targets' array."))?;

        let spread = match ctx.level {
            super::config::ObfuscationLevel::Low => (1200, 900),
            super::config::ObfuscationLevel::Medium => (4000, 3000),
            super::config::ObfuscationLevel::High => (5000, 3600),
            super::config::ObfuscationLevel::Insane => (7000, 5000),
        };

        for target in targets {
            let Some(blocks) = target.get_mut("blocks").and_then(Value::as_object_mut) else {
                continue;
            };
            for block in blocks.values_mut() {
                let is_top_level = block
                    .get("topLevel")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if !is_top_level {
                    continue;
                }
                let Some(obj) = block.as_object_mut() else {
                    continue;
                };
                obj.insert(
                    "x".to_string(),
                    Value::Number(Number::from(ctx.rng.gen_range(-spread.0..=spread.0))),
                );
                obj.insert(
                    "y".to_string(),
                    Value::Number(Number::from(ctx.rng.gen_range(-spread.1..=spread.1))),
                );
            }
        }

        ctx.note_pass(self.name());
        Ok(())
    }
}
