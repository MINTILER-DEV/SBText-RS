use super::context::ObfuscationContext;
use super::names::{generate_name, rewrite_proccode_name, NameKind};
use super::pass::ObfuscationPass;
use super::procedures::{
    collect_procedure_definitions, create_forwarding_call, create_procedure_definition,
    generated_procedure_signature, ProcedureSignature,
};
use anyhow::{anyhow, Result};
use serde_json::{Map, Value};

pub struct WrapProceduresPass;

impl ObfuscationPass for WrapProceduresPass {
    fn name(&self) -> &'static str {
        "wrap procedure calls"
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
            wrap_target_procedures(blocks, ctx)?;
        }

        ctx.note_pass(self.name());
        Ok(())
    }
}

fn wrap_target_procedures(
    blocks: &mut Map<String, Value>,
    ctx: &mut ObfuscationContext,
) -> Result<()> {
    let procedures = collect_procedure_definitions(blocks)?;
    for procedure in procedures {
        let old_signature = procedure.signature.clone();
        let internal_signature = ProcedureSignature {
            proccode: rewrite_proccode_name(
                &old_signature.proccode,
                &generate_name(NameKind::Procedure, ctx),
            ),
            argument_names: old_signature.argument_names.clone(),
            argument_ids: old_signature.argument_ids.clone(),
            warp: old_signature.warp,
        };
        let wrapper_signature = generated_procedure_signature(
            &old_signature.argument_names,
            old_signature.warp,
            rewrite_proccode_name(
                &old_signature.proccode,
                &generate_name(NameKind::Procedure, ctx),
            ),
            ctx,
        );

        update_prototype_signature(blocks, &procedure.prototype_id, &internal_signature)?;
        rewrite_existing_calls(blocks, &old_signature, &wrapper_signature)?;

        let wrapper_definition =
            create_procedure_definition(blocks, &wrapper_signature, None, ctx)?;
        let call_id = create_forwarding_call(
            blocks,
            &internal_signature,
            &wrapper_signature.argument_names,
            &wrapper_definition.definition_id,
            ctx,
        )?;
        if let Some(definition) = blocks.get_mut(&wrapper_definition.definition_id) {
            if let Some(obj) = definition.as_object_mut() {
                obj.insert("next".to_string(), Value::String(call_id));
            }
        }
    }
    Ok(())
}

fn update_prototype_signature(
    blocks: &mut Map<String, Value>,
    prototype_id: &str,
    signature: &ProcedureSignature,
) -> Result<()> {
    let prototype = blocks
        .get_mut(prototype_id)
        .ok_or_else(|| anyhow!("Missing procedure prototype '{}'.", prototype_id))?;
    let mutation = prototype
        .get_mut("mutation")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            anyhow!(
                "Prototype '{}' is missing its mutation object.",
                prototype_id
            )
        })?;
    mutation.insert(
        "proccode".to_string(),
        Value::String(signature.proccode.clone()),
    );
    mutation.insert(
        "argumentids".to_string(),
        Value::String(serde_json::to_string(&signature.argument_ids)?),
    );
    mutation.insert(
        "argumentnames".to_string(),
        Value::String(serde_json::to_string(&signature.argument_names)?),
    );
    mutation.insert(
        "argumentdefaults".to_string(),
        Value::String(serde_json::to_string(&vec![
            "";
            signature.argument_names.len()
        ])?),
    );
    mutation.insert(
        "warp".to_string(),
        Value::String(if signature.warp { "true" } else { "false" }.to_string()),
    );
    Ok(())
}

fn rewrite_existing_calls(
    blocks: &mut Map<String, Value>,
    old_signature: &ProcedureSignature,
    wrapper_signature: &ProcedureSignature,
) -> Result<()> {
    let call_ids = blocks
        .iter()
        .filter_map(|(block_id, block)| {
            let is_call = block.get("opcode").and_then(Value::as_str) == Some("procedures_call");
            let matches_old = block
                .get("mutation")
                .and_then(Value::as_object)
                .and_then(|mutation| mutation.get("proccode"))
                .and_then(Value::as_str)
                == Some(old_signature.proccode.as_str());
            if is_call && matches_old {
                Some(block_id.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    for call_id in call_ids {
        let Some(call) = blocks.get_mut(&call_id) else {
            continue;
        };
        let Some(call_obj) = call.as_object_mut() else {
            continue;
        };
        if let Some(mutation) = call_obj.get_mut("mutation").and_then(Value::as_object_mut) {
            mutation.insert(
                "proccode".to_string(),
                Value::String(wrapper_signature.proccode.clone()),
            );
            mutation.insert(
                "argumentids".to_string(),
                Value::String(serde_json::to_string(&wrapper_signature.argument_ids)?),
            );
            mutation.insert(
                "warp".to_string(),
                Value::String(
                    if wrapper_signature.warp {
                        "true"
                    } else {
                        "false"
                    }
                    .to_string(),
                ),
            );
        }

        let Some(inputs) = call_obj.get_mut("inputs").and_then(Value::as_object_mut) else {
            continue;
        };
        let old_inputs = std::mem::take(inputs);
        let mut reordered = Map::new();
        for (index, new_arg_id) in wrapper_signature.argument_ids.iter().enumerate() {
            if let Some(old_arg_id) = old_signature.argument_ids.get(index) {
                if let Some(input) = old_inputs.get(old_arg_id) {
                    reordered.insert(new_arg_id.clone(), input.clone());
                    continue;
                }
            }
            reordered.insert(
                new_arg_id.clone(),
                Value::Array(vec![
                    Value::from(1),
                    Value::Array(vec![Value::from(10), Value::String(String::new())]),
                ]),
            );
        }
        *inputs = reordered;
    }

    Ok(())
}
