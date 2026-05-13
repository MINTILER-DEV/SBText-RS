use super::context::ObfuscationContext;
use super::names::generate_identifier;
use anyhow::{anyhow, Result};
use rand::Rng;
use serde_json::{json, Map, Value};

#[derive(Debug, Clone)]
pub struct ProcedureSignature {
    pub proccode: String,
    pub argument_names: Vec<String>,
    pub argument_ids: Vec<String>,
    pub warp: bool,
}

#[derive(Debug, Clone)]
pub struct ProcedureDefinitionInfo {
    pub definition_id: String,
    pub prototype_id: String,
    pub signature: ProcedureSignature,
}

pub fn collect_procedure_definitions(
    blocks: &Map<String, Value>,
) -> Result<Vec<ProcedureDefinitionInfo>> {
    let mut definitions = Vec::new();
    for (definition_id, block) in blocks {
        if block.get("opcode").and_then(Value::as_str) != Some("procedures_definition") {
            continue;
        }
        let prototype_id = block_input_block_id(block, "custom_block").ok_or_else(|| {
            anyhow!(
                "Procedure definition '{}' is missing the custom_block input.",
                definition_id
            )
        })?;
        let prototype = blocks.get(&prototype_id).ok_or_else(|| {
            anyhow!(
                "Procedure prototype '{}' referenced by '{}' is missing.",
                prototype_id,
                definition_id
            )
        })?;
        definitions.push(ProcedureDefinitionInfo {
            definition_id: definition_id.clone(),
            prototype_id,
            signature: parse_procedure_signature(prototype)?,
        });
    }
    Ok(definitions)
}

pub fn parse_procedure_signature(prototype: &Value) -> Result<ProcedureSignature> {
    let mutation = prototype
        .get("mutation")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("Procedure prototype is missing its mutation object."))?;
    let proccode = mutation
        .get("proccode")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Procedure prototype mutation is missing proccode."))?;
    let argument_names = parse_string_array(mutation.get("argumentnames"))?;
    let argument_ids = parse_string_array(mutation.get("argumentids"))?;
    let warp = mutation
        .get("warp")
        .and_then(Value::as_str)
        .map(|value| value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    Ok(ProcedureSignature {
        proccode: proccode.to_string(),
        argument_names,
        argument_ids,
        warp,
    })
}

pub fn generated_procedure_signature(
    param_names: &[String],
    warp: bool,
    proccode: String,
    ctx: &mut ObfuscationContext,
) -> ProcedureSignature {
    let argument_ids = param_names
        .iter()
        .map(|_| generate_identifier("arg", ctx))
        .collect::<Vec<_>>();
    ProcedureSignature {
        proccode,
        argument_names: param_names.to_vec(),
        argument_ids,
        warp,
    }
}

pub fn create_procedure_definition(
    blocks: &mut Map<String, Value>,
    signature: &ProcedureSignature,
    body_start: Option<&str>,
    ctx: &mut ObfuscationContext,
) -> Result<ProcedureDefinitionInfo> {
    let definition_id = generate_identifier("block", ctx);
    let prototype_id = generate_identifier("block", ctx);
    let (x, y) = generated_procedure_position(ctx);

    let mut prototype_inputs = Map::new();
    for (param_name, arg_id) in signature
        .argument_names
        .iter()
        .zip(signature.argument_ids.iter())
    {
        let reporter_id = generate_identifier("block", ctx);
        blocks.insert(
            reporter_id.clone(),
            json!({
                "opcode": "argument_reporter_string_number",
                "next": Value::Null,
                "parent": prototype_id.clone(),
                "inputs": {},
                "fields": { "VALUE": [param_name, Value::Null] },
                "shadow": true,
                "topLevel": false
            }),
        );
        prototype_inputs.insert(arg_id.clone(), json!([1, reporter_id]));
    }

    blocks.insert(
        definition_id.clone(),
        json!({
            "opcode": "procedures_definition",
            "next": body_start.map(Value::from).unwrap_or(Value::Null),
            "parent": Value::Null,
            "inputs": { "custom_block": [1, prototype_id.clone()] },
            "fields": {},
            "shadow": false,
            "topLevel": true,
            "x": x,
            "y": y
        }),
    );
    blocks.insert(
        prototype_id.clone(),
        json!({
            "opcode": "procedures_prototype",
            "next": Value::Null,
            "parent": definition_id.clone(),
            "inputs": prototype_inputs,
            "fields": {},
            "shadow": true,
            "topLevel": false,
            "mutation": {
                "tagName": "mutation",
                "children": [],
                "proccode": signature.proccode,
                "argumentids": serde_json::to_string(&signature.argument_ids)?,
                "argumentnames": serde_json::to_string(&signature.argument_names)?,
                "argumentdefaults": serde_json::to_string(&vec![""; signature.argument_names.len()])?,
                "warp": if signature.warp { "true" } else { "false" }
            }
        }),
    );

    if let Some(body_start) = body_start {
        set_block_parent(blocks, body_start, &definition_id)?;
    }

    Ok(ProcedureDefinitionInfo {
        definition_id,
        prototype_id,
        signature: signature.clone(),
    })
}

pub fn create_forwarding_call(
    blocks: &mut Map<String, Value>,
    callee_signature: &ProcedureSignature,
    current_param_names: &[String],
    parent_id: &str,
    ctx: &mut ObfuscationContext,
) -> Result<String> {
    let call_id = generate_identifier("block", ctx);
    let mut inputs = Map::new();
    for (arg_id, param_name) in callee_signature
        .argument_ids
        .iter()
        .zip(current_param_names.iter())
    {
        let reporter_id = generate_identifier("block", ctx);
        blocks.insert(
            reporter_id.clone(),
            json!({
                "opcode": "argument_reporter_string_number",
                "next": Value::Null,
                "parent": call_id.clone(),
                "inputs": {},
                "fields": { "VALUE": [param_name, Value::Null] },
                "shadow": false,
                "topLevel": false
            }),
        );
        inputs.insert(arg_id.clone(), json!([2, reporter_id]));
    }

    blocks.insert(
        call_id.clone(),
        json!({
            "opcode": "procedures_call",
            "next": Value::Null,
            "parent": parent_id,
            "inputs": inputs,
            "fields": {},
            "shadow": false,
            "topLevel": false,
            "mutation": {
                "tagName": "mutation",
                "children": [],
                "proccode": callee_signature.proccode,
                "argumentids": serde_json::to_string(&callee_signature.argument_ids)?,
                "warp": if callee_signature.warp { "true" } else { "false" }
            }
        }),
    );
    Ok(call_id)
}

pub fn block_input_block_id(block: &Value, input_name: &str) -> Option<String> {
    let input_value = block
        .get("inputs")
        .and_then(Value::as_object)
        .and_then(|inputs| inputs.get(input_name))?;
    if let Some(id) = input_value.as_str() {
        return Some(id.to_string());
    }
    let arr = input_value.as_array()?;
    arr.get(1)?.as_str().map(ToString::to_string)
}

pub fn replace_input_block_reference(
    blocks: &mut Map<String, Value>,
    block_id: &str,
    input_name: &str,
    child_id: &str,
) -> Result<()> {
    let block = blocks.get_mut(block_id).ok_or_else(|| {
        anyhow!(
            "Missing block '{}' while replacing input '{}'.",
            block_id,
            input_name
        )
    })?;
    let obj = block
        .as_object_mut()
        .ok_or_else(|| anyhow!("Block '{}' is not an object.", block_id))?;
    let inputs = obj
        .entry("inputs")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| anyhow!("Block '{}' has invalid inputs shape.", block_id))?;
    inputs.insert(input_name.to_string(), json!([2, child_id]));
    Ok(())
}

pub fn set_block_parent(
    blocks: &mut Map<String, Value>,
    block_id: &str,
    parent_id: &str,
) -> Result<()> {
    let block = blocks
        .get_mut(block_id)
        .ok_or_else(|| anyhow!("Missing block '{}' while setting parent.", block_id))?;
    let obj = block
        .as_object_mut()
        .ok_or_else(|| anyhow!("Block '{}' is not an object.", block_id))?;
    obj.insert("parent".to_string(), Value::String(parent_id.to_string()));
    obj.insert("topLevel".to_string(), Value::Bool(false));
    Ok(())
}

pub fn set_block_next(
    blocks: &mut Map<String, Value>,
    block_id: &str,
    next_id: Option<&str>,
) -> Result<()> {
    let block = blocks
        .get_mut(block_id)
        .ok_or_else(|| anyhow!("Missing block '{}' while setting next.", block_id))?;
    let obj = block
        .as_object_mut()
        .ok_or_else(|| anyhow!("Block '{}' is not an object.", block_id))?;
    obj.insert(
        "next".to_string(),
        next_id.map(Value::from).unwrap_or(Value::Null),
    );
    Ok(())
}

pub fn block_next_id(blocks: &Map<String, Value>, block_id: &str) -> Option<String> {
    blocks
        .get(block_id)
        .and_then(|block| block.get("next"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

pub fn generated_procedure_position(ctx: &mut ObfuscationContext) -> (i64, i64) {
    (
        ctx.rng.gen_range(6000i64..12000i64),
        ctx.rng.gen_range(-9000i64..9000i64),
    )
}

fn parse_string_array(node: Option<&Value>) -> Result<Vec<String>> {
    let Some(node) = node else {
        return Ok(Vec::new());
    };
    let Some(raw) = node.as_str() else {
        return Ok(Vec::new());
    };
    Ok(serde_json::from_str::<Vec<String>>(raw).unwrap_or_default())
}
