use super::context::ObfuscationContext;
use super::names::{generate_name, NameKind};
use super::pass::ObfuscationPass;
use super::procedures::{
    block_input_block_id, block_next_id, collect_procedure_definitions, create_forwarding_call,
    create_procedure_definition, generated_procedure_signature, replace_input_block_reference,
    set_block_next, set_block_parent,
};
use anyhow::{anyhow, Result};
use serde_json::{Map, Value};

pub struct FlattenControlFlowPass;

impl ObfuscationPass for FlattenControlFlowPass {
    fn name(&self) -> &'static str {
        "flatten control flow"
    }

    fn run(&self, project: &mut Value, ctx: &mut ObfuscationContext) -> Result<()> {
        let targets = project
            .get_mut("targets")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| anyhow!("Invalid project.json: missing 'targets' array."))?;

        for target in targets {
            let Some(blocks) = target.get_mut("blocks").and_then(Value::as_object_mut) else {
                continue;
            };
            flatten_target(blocks, ctx)?;
        }

        ctx.note_pass(self.name());
        Ok(())
    }
}

#[derive(Clone)]
struct RootStack {
    owner_id: String,
    params: Vec<String>,
    warp: bool,
}

fn flatten_target(blocks: &mut Map<String, Value>, ctx: &mut ObfuscationContext) -> Result<()> {
    let procedure_definitions = collect_procedure_definitions(blocks)?;
    let mut roots = Vec::new();
    for procedure in &procedure_definitions {
        roots.push(RootStack {
            owner_id: procedure.definition_id.clone(),
            params: procedure.signature.argument_names.clone(),
            warp: procedure.signature.warp,
        });
    }

    for (block_id, block) in blocks.iter() {
        let is_top_level = block
            .get("topLevel")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let opcode = block
            .get("opcode")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !is_top_level || opcode == "procedures_definition" {
            continue;
        }
        if block.get("next").and_then(Value::as_str).is_none() {
            continue;
        }
        roots.push(RootStack {
            owner_id: block_id.clone(),
            params: Vec::new(),
            warp: false,
        });
    }

    for root in roots {
        let Some(start_id) = block_next_id(blocks, &root.owner_id) else {
            continue;
        };
        let helper_call = extract_chain_to_helper(blocks, &start_id, &root.params, root.warp, ctx)?;
        set_block_parent(blocks, &helper_call, &root.owner_id)?;
        set_block_next(blocks, &root.owner_id, Some(&helper_call))?;
    }

    Ok(())
}

fn extract_chain_to_helper(
    blocks: &mut Map<String, Value>,
    start_id: &str,
    param_names: &[String],
    warp: bool,
    ctx: &mut ObfuscationContext,
) -> Result<String> {
    flatten_chain_in_place(blocks, start_id, param_names, warp, ctx)?;

    let helper_signature = generated_procedure_signature(
        param_names,
        warp,
        helper_proccode(generate_name(NameKind::Procedure, ctx), param_names.len()),
        ctx,
    );
    let helper_definition =
        create_procedure_definition(blocks, &helper_signature, Some(start_id), ctx)?;
    create_forwarding_call(
        blocks,
        &helper_definition.signature,
        param_names,
        &helper_definition.definition_id,
        ctx,
    )
}

fn flatten_chain_in_place(
    blocks: &mut Map<String, Value>,
    block_id: &str,
    param_names: &[String],
    warp: bool,
    ctx: &mut ObfuscationContext,
) -> Result<()> {
    flatten_substack(blocks, block_id, "SUBSTACK", param_names, warp, ctx)?;
    flatten_substack(blocks, block_id, "SUBSTACK2", param_names, warp, ctx)?;

    if let Some(next_id) = block_next_id(blocks, block_id) {
        let next_call = extract_chain_to_helper(blocks, &next_id, param_names, warp, ctx)?;
        set_block_parent(blocks, &next_call, block_id)?;
        set_block_next(blocks, block_id, Some(&next_call))?;
    } else {
        set_block_next(blocks, block_id, None)?;
    }

    Ok(())
}

fn flatten_substack(
    blocks: &mut Map<String, Value>,
    block_id: &str,
    input_name: &str,
    param_names: &[String],
    warp: bool,
    ctx: &mut ObfuscationContext,
) -> Result<()> {
    let Some(substack_id) = blocks
        .get(block_id)
        .and_then(|block| block_input_block_id(block, input_name))
    else {
        return Ok(());
    };

    let substack_call = extract_chain_to_helper(blocks, &substack_id, param_names, warp, ctx)?;
    set_block_parent(blocks, &substack_call, block_id)?;
    replace_input_block_reference(blocks, block_id, input_name, &substack_call)?;
    Ok(())
}

fn helper_proccode(base_name: String, param_count: usize) -> String {
    if param_count == 0 {
        return base_name;
    }
    let placeholders = std::iter::repeat_n("%s", param_count)
        .collect::<Vec<_>>()
        .join(" ");
    format!("{} {}", base_name, placeholders)
}
