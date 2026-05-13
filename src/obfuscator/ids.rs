use super::context::ObfuscationContext;
use super::names::generate_identifier;
use super::pass::ObfuscationPass;
use anyhow::{anyhow, Result};
use serde_json::{Map, Value};
use std::collections::HashMap;

pub struct RandomizeBlockIdsPass;

impl ObfuscationPass for RandomizeBlockIdsPass {
    fn name(&self) -> &'static str {
        "randomize block ids"
    }

    fn run(&self, project: &mut Value, ctx: &mut ObfuscationContext) -> Result<()> {
        let targets = project
            .get_mut("targets")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| anyhow!("Invalid project.json: missing 'targets' array."))?;

        for target in targets.iter_mut() {
            let Some(id_map) = target
                .get("blocks")
                .and_then(Value::as_object)
                .map(|blocks| {
                    blocks
                        .keys()
                        .map(|old_id| (old_id.clone(), generate_identifier("block", ctx)))
                        .collect::<HashMap<_, _>>()
                })
            else {
                continue;
            };

            let new_blocks = {
                let blocks = target
                    .get_mut("blocks")
                    .and_then(Value::as_object_mut)
                    .ok_or_else(|| anyhow!("Target blocks changed shape during id rewrite."))?;
                let old_blocks = std::mem::take(blocks);
                let mut rebuilt = Map::new();
                for (old_id, mut block) in old_blocks {
                    rewrite_block_references(&mut block, &id_map);
                    let new_id = id_map
                        .get(&old_id)
                        .ok_or_else(|| anyhow!("Missing replacement block id for '{}'.", old_id))?;
                    rebuilt.insert(new_id.clone(), block);
                }
                rebuilt
            };

            if let Some(comments) = target.get_mut("comments").and_then(Value::as_object_mut) {
                for comment in comments.values_mut() {
                    if let Some(block_id) = comment.get_mut("blockId") {
                        rewrite_block_references(block_id, &id_map);
                    }
                }
            }

            let blocks = target
                .get_mut("blocks")
                .and_then(Value::as_object_mut)
                .ok_or_else(|| anyhow!("Target blocks changed shape during id rewrite."))?;
            *blocks = new_blocks;
        }

        ctx.note_pass(self.name());
        Ok(())
    }
}

pub fn rewrite_block_references(value: &mut Value, id_map: &HashMap<String, String>) {
    match value {
        Value::String(text) => {
            if let Some(new_id) = id_map.get(text) {
                *text = new_id.clone();
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_block_references(item, id_map);
            }
        }
        Value::Object(obj) => {
            for entry in obj.values_mut() {
                rewrite_block_references(entry, id_map);
            }
        }
        _ => {}
    }
}
