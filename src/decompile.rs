use anyhow::{anyhow, Context, Result};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

pub fn decompile_sb3(input: &Path, output: Option<&Path>, split_sprites: bool) -> Result<()> {
    let (project_json, assets) = read_sb3(input)?;
    let targets = project_json
        .get("targets")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("Invalid project.json: missing 'targets' array."))?;

    let mut decompiled_targets = Vec::new();
    for target in targets {
        decompiled_targets.push(decompile_target(target)?);
    }

    if split_sprites {
        let out_dir = match output {
            Some(path) => path.to_path_buf(),
            None => default_split_output_dir(input),
        };
        write_split_project(&decompiled_targets, &assets, &out_dir)?;
    } else {
        let out_file = match output {
            Some(path) => {
                if path.extension().is_none() {
                    path.with_extension("sbtext")
                } else {
                    path.to_path_buf()
                }
            }
            None => input.with_extension("sbtext"),
        };
        write_single_project(&decompiled_targets, &assets, &out_file)?;
    }

    Ok(())
}

fn read_sb3(input: &Path) -> Result<(Value, HashMap<String, Vec<u8>>)> {
    let file =
        fs::File::open(input).with_context(|| format!("Failed to open '{}'.", input.display()))?;
    let mut zip = ZipArchive::new(file)
        .with_context(|| format!("'{}' is not a valid zip/.sb3 file.", input.display()))?;

    let mut project_json_str = String::new();
    {
        let mut entry = zip
            .by_name("project.json")
            .map_err(|_| anyhow!("project.json not found in '{}'.", input.display()))?;
        use std::io::Read;
        entry.read_to_string(&mut project_json_str)?;
    }
    let project_json: Value = serde_json::from_str(&project_json_str)
        .with_context(|| format!("Invalid project.json inside '{}'.", input.display()))?;

    let mut assets = HashMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let name = entry.name().to_string();
        if name == "project.json" || name.ends_with('/') {
            continue;
        }
        let mut bytes = Vec::new();
        use std::io::Read;
        entry.read_to_end(&mut bytes)?;
        assets.insert(name, bytes);
    }

    Ok((project_json, assets))
}

#[derive(Debug, Clone)]
struct DecompiledTarget {
    name: String,
    is_stage: bool,
    variables: Vec<String>,
    lists: Vec<String>,
    costumes: Vec<String>,
    procedures: Vec<DecompiledProcedure>,
    scripts: Vec<DecompiledScript>,
}

#[derive(Debug, Clone)]
struct DecompiledProcedure {
    name: String,
    params: Vec<String>,
    warp: bool,
    body: Vec<String>,
}

#[derive(Debug, Clone)]
struct DecompiledScript {
    header: String,
    body: Vec<String>,
}

fn decompile_target(target: &Value) -> Result<DecompiledTarget> {
    let name = target
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Target missing 'name'."))?
        .to_string();
    let is_stage = target
        .get("isStage")
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow!("Target '{}' missing isStage.", name))?;

    let variables = read_decls(target.get("variables"));
    let lists = read_decls(target.get("lists"));
    let costumes = read_costumes(target.get("costumes"));

    let blocks_obj = target
        .get("blocks")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("Target '{}' missing blocks object.", name))?;
    let blocks = blocks_obj.clone();

    let mut procedure_starts = Vec::new();
    let mut script_starts = Vec::new();
    for (id, block) in &blocks {
        if !block
            .get("topLevel")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let opcode = block.get("opcode").and_then(Value::as_str).unwrap_or("");
        match opcode {
            "procedures_definition" => procedure_starts.push(id.clone()),
            "event_whenflagclicked"
            | "event_whenthisspriteclicked"
            | "event_whenbroadcastreceived" => script_starts.push(id.clone()),
            _ => {}
        }
    }

    procedure_starts.sort_by(|a, b| block_sort_key(&blocks, a).cmp(&block_sort_key(&blocks, b)));
    script_starts.sort_by(|a, b| block_sort_key(&blocks, a).cmp(&block_sort_key(&blocks, b)));

    let mut procedures = Vec::new();
    for id in procedure_starts {
        procedures.push(decompile_procedure(&blocks, &id)?);
    }

    let mut scripts = Vec::new();
    for id in script_starts {
        scripts.push(decompile_script(&blocks, &id)?);
    }

    Ok(DecompiledTarget {
        name,
        is_stage,
        variables,
        lists,
        costumes,
        procedures,
        scripts,
    })
}

fn read_decls(node: Option<&Value>) -> Vec<String> {
    let mut out = Vec::new();
    let Some(obj) = node.and_then(Value::as_object) else {
        return out;
    };
    for value in obj.values() {
        if let Some(name) = value
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(Value::as_str)
            .map(ToString::to_string)
        {
            out.push(name);
        }
    }
    out
}

fn read_costumes(node: Option<&Value>) -> Vec<String> {
    let mut out = Vec::new();
    let Some(arr) = node.and_then(Value::as_array) else {
        return out;
    };
    for costume in arr {
        if let Some(md5ext) = costume.get("md5ext").and_then(Value::as_str) {
            out.push(md5ext.to_string());
        }
    }
    out
}

fn block_sort_key(blocks: &Map<String, Value>, id: &str) -> (i64, i64, String) {
    let block = blocks.get(id);
    let y = block
        .and_then(|b| b.get("y"))
        .and_then(Value::as_i64)
        .unwrap_or(i64::MAX);
    let x = block
        .and_then(|b| b.get("x"))
        .and_then(Value::as_i64)
        .unwrap_or(i64::MAX);
    (y, x, id.to_string())
}

fn decompile_procedure(
    blocks: &Map<String, Value>,
    definition_id: &str,
) -> Result<DecompiledProcedure> {
    let definition = get_block(blocks, definition_id)?;
    let prototype_id = block_input_block_id(definition, "custom_block").ok_or_else(|| {
        anyhow!(
            "Procedure definition '{}' missing custom_block input.",
            definition_id
        )
    })?;
    let prototype = get_block(blocks, &prototype_id)?;

    let mutation = prototype
        .get("mutation")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("Procedure prototype '{}' missing mutation.", prototype_id))?;
    let proccode = mutation
        .get("proccode")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Procedure prototype '{}' missing proccode.", prototype_id))?;
    let name = proccode_name(proccode);

    let params =
        if let Some(argument_names_raw) = mutation.get("argumentnames").and_then(Value::as_str) {
            serde_json::from_str::<Vec<String>>(argument_names_raw).unwrap_or_default()
        } else {
            Vec::new()
        };

    let warp = mutation
        .get("warp")
        .and_then(Value::as_str)
        .map(|s| s.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let body_start = definition.get("next").and_then(Value::as_str);
    let body = decompile_chain(blocks, body_start, 4, &mut HashSet::new())?;

    Ok(DecompiledProcedure {
        name,
        params,
        warp,
        body,
    })
}

fn decompile_script(blocks: &Map<String, Value>, hat_id: &str) -> Result<DecompiledScript> {
    let hat = get_block(blocks, hat_id)?;
    let opcode = hat.get("opcode").and_then(Value::as_str).unwrap_or("");
    let header = match opcode {
        "event_whenflagclicked" => "when flag clicked".to_string(),
        "event_whenthisspriteclicked" => "when this sprite clicked".to_string(),
        "event_whenbroadcastreceived" => {
            let msg = field_first_string(hat, "BROADCAST_OPTION")
                .unwrap_or_else(|| "message1".to_string());
            format!("when I receive [{}]", msg)
        }
        other => format!("# unsupported event opcode: {}", other),
    };
    let body_start = hat.get("next").and_then(Value::as_str);
    let body = decompile_chain(blocks, body_start, 4, &mut HashSet::new())?;
    Ok(DecompiledScript { header, body })
}

fn decompile_chain(
    blocks: &Map<String, Value>,
    start: Option<&str>,
    indent: usize,
    visited: &mut HashSet<String>,
) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    let mut current = start.map(ToString::to_string);
    while let Some(id) = current {
        if !visited.insert(id.clone()) {
            lines.push(format!(
                "{}# warning: cyclic block chain at {}",
                spaces(indent),
                id
            ));
            break;
        }
        let block = get_block(blocks, &id)?;
        let mut stmt = decompile_statement(blocks, &id, block, indent, visited)?;
        lines.append(&mut stmt);
        current = block
            .get("next")
            .and_then(Value::as_str)
            .map(ToString::to_string);
    }
    Ok(lines)
}

fn decompile_statement(
    blocks: &Map<String, Value>,
    id: &str,
    block: &Value,
    indent: usize,
    visited: &mut HashSet<String>,
) -> Result<Vec<String>> {
    let op = block.get("opcode").and_then(Value::as_str).unwrap_or("");
    let pad = spaces(indent);
    let mut out = Vec::new();
    match op {
        "event_broadcast" => {
            let msg = broadcast_message(blocks, block).unwrap_or_else(|| "message1".to_string());
            out.push(format!("{}broadcast [{}]", pad, msg));
        }
        "event_broadcastandwait" => {
            let msg = broadcast_message(blocks, block).unwrap_or_else(|| "message1".to_string());
            out.push(format!("{}broadcast and wait [{}]", pad, msg));
        }
        "data_setvariableto" => {
            let name = field_first_string(block, "VARIABLE").unwrap_or_else(|| "var".to_string());
            let value = expr_from_input(blocks, block, "VALUE")?;
            out.push(format!(
                "{}set [{}] to ({})",
                pad,
                format_bracket_name(&name),
                value
            ));
        }
        "data_changevariableby" => {
            let name = field_first_string(block, "VARIABLE").unwrap_or_else(|| "var".to_string());
            let value = expr_from_input(blocks, block, "VALUE")?;
            out.push(format!(
                "{}change [{}] by ({})",
                pad,
                format_bracket_name(&name),
                value
            ));
        }
        "motion_movesteps" => {
            let steps = expr_from_input(blocks, block, "STEPS")?;
            out.push(format!("{}move ({}) [steps]", pad, steps));
        }
        "looks_say" => {
            let message = expr_from_input(blocks, block, "MESSAGE")?;
            out.push(format!("{}say ({})", pad, message));
        }
        "looks_sayforsecs" => {
            let message = expr_from_input(blocks, block, "MESSAGE")?;
            let secs = expr_from_input(blocks, block, "SECS")?;
            out.push(format!("{}say ({}) for ({}) [seconds]", pad, message, secs));
        }
        "looks_think" => {
            let message = expr_from_input(blocks, block, "MESSAGE")?;
            out.push(format!("{}think ({})", pad, message));
        }
        "motion_turnright" => {
            let degrees = expr_from_input(blocks, block, "DEGREES")?;
            out.push(format!("{}turn right ({})", pad, degrees));
        }
        "motion_turnleft" => {
            let degrees = expr_from_input(blocks, block, "DEGREES")?;
            out.push(format!("{}turn left ({})", pad, degrees));
        }
        "motion_gotoxy" => {
            let x = expr_from_input(blocks, block, "X")?;
            let y = expr_from_input(blocks, block, "Y")?;
            out.push(format!("{}go to x ({}) y ({})", pad, x, y));
        }
        "motion_changexby" => {
            let v = expr_from_input(blocks, block, "DX")?;
            out.push(format!("{}change x by ({})", pad, v));
        }
        "motion_setx" => {
            let v = expr_from_input(blocks, block, "X")?;
            out.push(format!("{}set x to ({})", pad, v));
        }
        "motion_changeyby" => {
            let v = expr_from_input(blocks, block, "DY")?;
            out.push(format!("{}change y by ({})", pad, v));
        }
        "motion_sety" => {
            let v = expr_from_input(blocks, block, "Y")?;
            out.push(format!("{}set y to ({})", pad, v));
        }
        "motion_pointindirection" => {
            let v = expr_from_input(blocks, block, "DIRECTION")?;
            out.push(format!("{}point in direction ({})", pad, v));
        }
        "motion_ifonedgebounce" => out.push(format!("{}if on edge bounce", pad)),
        "looks_changesizeby" => {
            let v = expr_from_input(blocks, block, "CHANGE")?;
            out.push(format!("{}change size by ({})", pad, v));
        }
        "looks_setsizeto" => {
            let v = expr_from_input(blocks, block, "SIZE")?;
            out.push(format!("{}set size to ({})", pad, v));
        }
        "looks_show" => out.push(format!("{}show", pad)),
        "looks_hide" => out.push(format!("{}hide", pad)),
        "looks_nextcostume" => out.push(format!("{}next costume", pad)),
        "looks_nextbackdrop" => out.push(format!("{}next backdrop", pad)),
        "control_wait" => {
            let v = expr_from_input(blocks, block, "DURATION")?;
            out.push(format!("{}wait ({})", pad, v));
        }
        "control_wait_until" => {
            let c = expr_from_input(blocks, block, "CONDITION")?;
            out.push(format!("{}wait until <{}>", pad, c));
        }
        "control_repeat" => {
            let times = expr_from_input(blocks, block, "TIMES")?;
            out.push(format!("{}repeat ({})", pad, times));
            let sub = block_input_block_id(block, "SUBSTACK");
            let mut body = decompile_chain(blocks, sub.as_deref(), indent + 2, visited)?;
            out.append(&mut body);
            out.push(format!("{}end", pad));
        }
        "control_repeat_until" => {
            let c = expr_from_input(blocks, block, "CONDITION")?;
            out.push(format!("{}repeat until <{}>", pad, c));
            let sub = block_input_block_id(block, "SUBSTACK");
            let mut body = decompile_chain(blocks, sub.as_deref(), indent + 2, visited)?;
            out.append(&mut body);
            out.push(format!("{}end", pad));
        }
        "control_forever" => {
            out.push(format!("{}forever", pad));
            let sub = block_input_block_id(block, "SUBSTACK");
            let mut body = decompile_chain(blocks, sub.as_deref(), indent + 2, visited)?;
            out.append(&mut body);
            out.push(format!("{}end", pad));
        }
        "control_if" => {
            let c = expr_from_input(blocks, block, "CONDITION")?;
            out.push(format!("{}if <{}> then", pad, c));
            let sub = block_input_block_id(block, "SUBSTACK");
            let mut body = decompile_chain(blocks, sub.as_deref(), indent + 2, visited)?;
            out.append(&mut body);
            out.push(format!("{}end", pad));
        }
        "control_if_else" => {
            let c = expr_from_input(blocks, block, "CONDITION")?;
            out.push(format!("{}if <{}> then", pad, c));
            let sub_then = block_input_block_id(block, "SUBSTACK");
            let mut then_body = decompile_chain(blocks, sub_then.as_deref(), indent + 2, visited)?;
            out.append(&mut then_body);
            out.push(format!("{}else", pad));
            let sub_else = block_input_block_id(block, "SUBSTACK2");
            let mut else_body = decompile_chain(blocks, sub_else.as_deref(), indent + 2, visited)?;
            out.append(&mut else_body);
            out.push(format!("{}end", pad));
        }
        "control_stop" => {
            let option =
                field_first_string(block, "STOP_OPTION").unwrap_or_else(|| "all".to_string());
            out.push(format!("{}stop ({})", pad, quote_str(&option)));
        }
        "sensing_askandwait" => {
            let q = expr_from_input(blocks, block, "QUESTION")?;
            out.push(format!("{}ask ({})", pad, q));
        }
        "sensing_resettimer" => out.push(format!("{}reset timer", pad)),
        "data_addtolist" => {
            let list = field_first_string(block, "LIST").unwrap_or_else(|| "list".to_string());
            let item = expr_from_input(blocks, block, "ITEM")?;
            out.push(format!(
                "{}add ({}) to [{}]",
                pad,
                item,
                format_bracket_name(&list)
            ));
        }
        "data_deleteoflist" => {
            let list = field_first_string(block, "LIST").unwrap_or_else(|| "list".to_string());
            let idx = expr_from_input(blocks, block, "INDEX")?;
            out.push(format!(
                "{}delete ({}) of [{}]",
                pad,
                idx,
                format_bracket_name(&list)
            ));
        }
        "data_deletealloflist" => {
            let list = field_first_string(block, "LIST").unwrap_or_else(|| "list".to_string());
            out.push(format!("{}delete all of [{}]", pad, format_bracket_name(&list)));
        }
        "data_insertatlist" => {
            let list = field_first_string(block, "LIST").unwrap_or_else(|| "list".to_string());
            let item = expr_from_input(blocks, block, "ITEM")?;
            let idx = expr_from_input(blocks, block, "INDEX")?;
            out.push(format!(
                "{}insert ({}) at ({}) of [{}]",
                pad,
                item,
                idx,
                format_bracket_name(&list)
            ));
        }
        "data_replaceitemoflist" => {
            let list = field_first_string(block, "LIST").unwrap_or_else(|| "list".to_string());
            let item = expr_from_input(blocks, block, "ITEM")?;
            let idx = expr_from_input(blocks, block, "INDEX")?;
            out.push(format!(
                "{}replace item ({}) of [{}] with ({})",
                pad,
                idx,
                format_bracket_name(&list),
                item
            ));
        }
        "procedures_call" => {
            let (name, arg_order) = procedure_call_shape(block)?;
            let mut line = format!("{}{}", pad, name);
            for arg_id in arg_order {
                let arg_expr = expr_from_input(blocks, block, &arg_id)?;
                line.push_str(&format!(" ({})", arg_expr));
            }
            out.push(line);
        }
        "pen_penDown" => out.push(format!("{}pen down", pad)),
        "pen_penUp" => out.push(format!("{}pen up", pad)),
        "pen_clear" => out.push(format!("{}erase all", pad)),
        "pen_stamp" => out.push(format!("{}stamp", pad)),
        "pen_changePenSizeBy" => {
            let v = expr_from_input(blocks, block, "SIZE")?;
            out.push(format!("{}change pen size by ({})", pad, v));
        }
        "pen_setPenSizeTo" => {
            let v = expr_from_input(blocks, block, "SIZE")?;
            out.push(format!("{}set pen size to ({})", pad, v));
        }
        "pen_changePenColorParamBy" => {
            let param = pen_color_param(blocks, block).unwrap_or_else(|| "color".to_string());
            let v = expr_from_input(blocks, block, "VALUE")?;
            out.push(format!("{}change pen {} by ({})", pad, param, v));
        }
        "pen_setPenColorParamTo" => {
            let param = pen_color_param(blocks, block).unwrap_or_else(|| "color".to_string());
            let v = expr_from_input(blocks, block, "VALUE")?;
            out.push(format!("{}set pen {} to ({})", pad, param, v));
        }
        _ => out.push(format!(
            "{}# unsupported opcode: {} (block {})",
            pad, op, id
        )),
    }
    Ok(out)
}

fn expr_from_input(blocks: &Map<String, Value>, block: &Value, input_name: &str) -> Result<String> {
    let inputs = block.get("inputs").and_then(Value::as_object);
    let Some(input_val) = inputs.and_then(|m| m.get(input_name)) else {
        return Ok("0".to_string());
    };
    input_to_expr(blocks, input_val)
}

fn input_to_expr(blocks: &Map<String, Value>, input_val: &Value) -> Result<String> {
    if let Some(block_id) = input_val.as_str() {
        return reporter_expr(blocks, block_id);
    }
    let Some(arr) = input_val.as_array() else {
        return Ok("0".to_string());
    };
    if arr.len() < 2 {
        return Ok("0".to_string());
    }
    let mode = arr[0].as_i64().unwrap_or_default();
    let payload = &arr[1];
    match mode {
        1 => {
            if let Some(block_id) = payload.as_str() {
                reporter_expr(blocks, block_id)
            } else if let Some(lit) = payload.as_array() {
                Ok(literal_to_expr(lit))
            } else {
                Ok("0".to_string())
            }
        }
        2 | 3 => {
            if let Some(block_id) = payload.as_str() {
                reporter_expr(blocks, block_id)
            } else {
                Ok("0".to_string())
            }
        }
        _ => Ok("0".to_string()),
    }
}

fn reporter_expr(blocks: &Map<String, Value>, block_id: &str) -> Result<String> {
    let block = get_block(blocks, block_id)?;
    let op = block.get("opcode").and_then(Value::as_str).unwrap_or("");
    let expr = match op {
        "data_variable" => format_var_ref(
            field_first_string(block, "VARIABLE").unwrap_or_else(|| "var".to_string()),
        ),
        "argument_reporter_string_number" => {
            format_var_ref(field_first_string(block, "VALUE").unwrap_or_default())
        }
        "sensing_answer" => "answer".to_string(),
        "sensing_mousex" => "mouse x".to_string(),
        "sensing_mousey" => "mouse y".to_string(),
        "sensing_timer" => "timer".to_string(),
        "operator_round" => format!("round ({})", expr_from_input(blocks, block, "NUM")?),
        "operator_mathop" => {
            let op_name =
                field_first_string(block, "OPERATOR").unwrap_or_else(|| "floor".to_string());
            format!("{} ({})", op_name, expr_from_input(blocks, block, "NUM")?)
        }
        "sensing_of" => {
            let prop = field_first_string(block, "PROPERTY").unwrap_or_else(|| "var".to_string());
            let obj_id = block_input_block_id(block, "OBJECT").unwrap_or_default();
            let obj_name = blocks
                .get(&obj_id)
                .and_then(|b| field_first_string(b, "OBJECT"))
                .unwrap_or_else(|| "Sprite".to_string());
            format!("{}.{}", obj_name, prop)
        }
        "operator_random" => format!(
            "pick random ({}) to ({})",
            expr_from_input(blocks, block, "FROM")?,
            expr_from_input(blocks, block, "TO")?
        ),
        "data_itemoflist" => {
            let list = field_first_string(block, "LIST").unwrap_or_else(|| "list".to_string());
            let idx = expr_from_input(blocks, block, "INDEX")?;
            format!("item ({}) of [{}]", idx, format_bracket_name(&list))
        }
        "data_lengthoflist" => {
            let list = field_first_string(block, "LIST").unwrap_or_else(|| "list".to_string());
            format!("length of [{}]", format_bracket_name(&list))
        }
        "data_listcontainsitem" => {
            let list = field_first_string(block, "LIST").unwrap_or_else(|| "list".to_string());
            let item = expr_from_input(blocks, block, "ITEM")?;
            format!("[{}] contains ({})", format_bracket_name(&list), item)
        }
        "sensing_keypressed" => {
            let key = key_option(blocks, block).unwrap_or_else(|| "space".to_string());
            format!("key ({}) pressed?", quote_str(&key))
        }
        "operator_not" => format!("not ({})", expr_from_input(blocks, block, "OPERAND")?),
        "operator_add" => binary_expr(blocks, block, "+", "NUM1", "NUM2")?,
        "operator_subtract" => binary_expr(blocks, block, "-", "NUM1", "NUM2")?,
        "operator_multiply" => binary_expr(blocks, block, "*", "NUM1", "NUM2")?,
        "operator_divide" => binary_expr(blocks, block, "/", "NUM1", "NUM2")?,
        "operator_mod" => binary_expr(blocks, block, "%", "NUM1", "NUM2")?,
        "operator_lt" => binary_expr(blocks, block, "<", "OPERAND1", "OPERAND2")?,
        "operator_gt" => binary_expr(blocks, block, ">", "OPERAND1", "OPERAND2")?,
        "operator_equals" => binary_expr(blocks, block, "=", "OPERAND1", "OPERAND2")?,
        "operator_and" => binary_expr(blocks, block, "and", "OPERAND1", "OPERAND2")?,
        "operator_or" => binary_expr(blocks, block, "or", "OPERAND1", "OPERAND2")?,
        _ => "0".to_string(),
    };
    Ok(expr)
}

fn binary_expr(
    blocks: &Map<String, Value>,
    block: &Value,
    op: &str,
    left: &str,
    right: &str,
) -> Result<String> {
    Ok(format!(
        "(({}) {} ({}))",
        expr_from_input(blocks, block, left)?,
        op,
        expr_from_input(blocks, block, right)?
    ))
}

fn key_option(blocks: &Map<String, Value>, block: &Value) -> Option<String> {
    let menu_id = block_input_block_id(block, "KEY_OPTION")?;
    let menu_block = blocks.get(&menu_id)?;
    field_first_string(menu_block, "KEY_OPTION")
}

fn pen_color_param(blocks: &Map<String, Value>, block: &Value) -> Option<String> {
    let menu_id = block_input_block_id(block, "COLOR_PARAM")?;
    let menu_block = blocks.get(&menu_id)?;
    field_first_string(menu_block, "colorParam")
}

fn procedure_call_shape(block: &Value) -> Result<(String, Vec<String>)> {
    let mutation = block
        .get("mutation")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("procedures_call block missing mutation."))?;
    let proccode = mutation
        .get("proccode")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("procedures_call mutation missing proccode."))?;
    let name = proccode_name(proccode);
    let arg_ids_raw = mutation
        .get("argumentids")
        .and_then(Value::as_str)
        .unwrap_or("[]");
    let arg_order = serde_json::from_str::<Vec<String>>(arg_ids_raw).unwrap_or_default();
    Ok((name, arg_order))
}

fn proccode_name(proccode: &str) -> String {
    let mut parts = Vec::new();
    for token in proccode.split_whitespace() {
        if token == "%s" {
            break;
        }
        parts.push(token);
    }
    if parts.is_empty() {
        proccode.to_string()
    } else {
        parts.join(" ")
    }
}

fn broadcast_message(blocks: &Map<String, Value>, block: &Value) -> Option<String> {
    let menu_id = block_input_block_id(block, "BROADCAST_INPUT")?;
    let menu_block = blocks.get(&menu_id)?;
    field_first_string(menu_block, "BROADCAST_OPTION")
}

fn block_input_block_id(block: &Value, input_name: &str) -> Option<String> {
    let input_val = block
        .get("inputs")
        .and_then(Value::as_object)
        .and_then(|m| m.get(input_name))?;
    if let Some(id) = input_val.as_str() {
        return Some(id.to_string());
    }
    let arr = input_val.as_array()?;
    if arr.len() < 2 {
        return None;
    }
    arr[1].as_str().map(ToString::to_string)
}

fn field_first_string(block: &Value, field_name: &str) -> Option<String> {
    let fields = block.get("fields").and_then(Value::as_object)?;
    let value = fields.get(field_name)?;
    if let Some(s) = value.as_str() {
        return Some(s.to_string());
    }
    let arr = value.as_array()?;
    arr.first()?.as_str().map(ToString::to_string)
}

fn literal_to_expr(lit: &[Value]) -> String {
    if lit.len() < 2 {
        return "0".to_string();
    }
    let code = lit[0].as_i64().unwrap_or_default();
    match code {
        4 => lit[1].as_str().unwrap_or("0").to_string(),
        10 => quote_str(lit[1].as_str().unwrap_or("")),
        _ => {
            if let Some(s) = lit[1].as_str() {
                quote_str(s)
            } else {
                "0".to_string()
            }
        }
    }
}

fn format_var_ref(name: String) -> String {
    if is_simple_identifier_or_qualified(&name) {
        name
    } else {
        format!("[{}]", format_bracket_name(&name))
    }
}

fn format_bracket_name(name: &str) -> String {
    if is_simple_identifier_or_qualified(name) {
        name.to_string()
    } else {
        quote_str(name)
    }
}

fn is_simple_identifier_or_qualified(name: &str) -> bool {
    if let Some((left, right)) = name.split_once('.') {
        if right.contains('.') {
            return false;
        }
        return is_simple_identifier(left) && is_simple_identifier(right);
    }
    is_simple_identifier(name)
}

fn is_simple_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '?')
}

fn quote_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn spaces(n: usize) -> String {
    " ".repeat(n)
}

fn get_block<'a>(blocks: &'a Map<String, Value>, id: &str) -> Result<&'a Value> {
    blocks
        .get(id)
        .ok_or_else(|| anyhow!("Missing block '{}'.", id))
}

fn render_target(target: &DecompiledTarget) -> String {
    let mut lines = Vec::new();
    if target.is_stage {
        if target.name.eq_ignore_ascii_case("stage") {
            lines.push("stage".to_string());
        } else {
            lines.push(format!("stage {}", format_decl_name(&target.name)));
        }
    } else {
        lines.push(format!("sprite {}", format_decl_name(&target.name)));
    }

    for var in &target.variables {
        lines.push(format!("  var {}", format_decl_name(var)));
    }
    for list in &target.lists {
        lines.push(format!("  list {}", format_decl_name(list)));
    }
    for costume in &target.costumes {
        lines.push(format!("  costume {}", quote_str(costume)));
    }

    if (!target.variables.is_empty() || !target.lists.is_empty() || !target.costumes.is_empty())
        && (!target.procedures.is_empty() || !target.scripts.is_empty())
    {
        lines.push(String::new());
    }

    for (idx, proc_def) in target.procedures.iter().enumerate() {
        let mut header = format!(
            "  define {}{}",
            if proc_def.warp { "!" } else { "" },
            format_decl_name(&proc_def.name)
        );
        for param in &proc_def.params {
            header.push_str(&format!(" ({})", format_decl_name(param)));
        }
        lines.push(header);
        if proc_def.body.is_empty() {
            lines.push("    # empty".to_string());
        } else {
            lines.extend(proc_def.body.clone());
        }
        lines.push("  end".to_string());
        if idx + 1 < target.procedures.len() || !target.scripts.is_empty() {
            lines.push(String::new());
        }
    }

    for (idx, script) in target.scripts.iter().enumerate() {
        lines.push(format!("  {}", script.header));
        if script.body.is_empty() {
            lines.push("    # empty".to_string());
        } else {
            lines.extend(script.body.clone());
        }
        lines.push("  end".to_string());
        if idx + 1 < target.scripts.len() {
            lines.push(String::new());
        }
    }

    lines.push("end".to_string());
    lines.push(String::new());
    lines.join("\n")
}

fn format_decl_name(name: &str) -> String {
    if is_simple_identifier(name) {
        name.to_string()
    } else {
        quote_str(name)
    }
}

fn write_single_project(
    targets: &[DecompiledTarget],
    assets: &HashMap<String, Vec<u8>>,
    out_file: &Path,
) -> Result<()> {
    let mut ordered = targets.to_vec();
    ordered.sort_by_key(|t| if t.is_stage { 0 } else { 1 });
    let mut text = String::new();
    for target in &ordered {
        text.push_str(&render_target(target));
        text.push('\n');
    }

    if let Some(parent) = out_file.parent() {
        fs::create_dir_all(parent)?;
        write_assets_for_targets(&ordered, assets, parent)?;
    }
    fs::write(out_file, text.as_bytes())
        .with_context(|| format!("Failed to write '{}'.", out_file.display()))?;
    Ok(())
}

fn write_split_project(
    targets: &[DecompiledTarget],
    assets: &HashMap<String, Vec<u8>>,
    out_dir: &Path,
) -> Result<()> {
    fs::create_dir_all(out_dir)?;
    let mut stage = None;
    let mut sprites = Vec::new();
    for target in targets {
        if target.is_stage && stage.is_none() {
            stage = Some(target.clone());
        } else if !target.is_stage {
            sprites.push(target.clone());
        }
    }

    let mut used_files = HashSet::new();
    let mut imports = Vec::new();
    for sprite in &sprites {
        let file_name = unique_sprite_filename(&sprite.name, &mut used_files);
        imports.push((sprite.name.clone(), file_name.clone()));
        let sprite_path = out_dir.join(&file_name);
        fs::write(&sprite_path, render_target(sprite).as_bytes())
            .with_context(|| format!("Failed to write '{}'.", sprite_path.display()))?;
    }

    let mut main_text = String::new();
    for (sprite_name, file_name) in &imports {
        main_text.push_str(&format!(
            "import [{}] from {}\n",
            sprite_name,
            quote_str(file_name)
        ));
    }
    if !imports.is_empty() {
        main_text.push('\n');
    }
    if let Some(stage_target) = stage {
        main_text.push_str(&render_target(&stage_target));
    } else {
        main_text.push_str("stage\nend\n");
    }

    let main_path = out_dir.join("main.sbtext");
    fs::write(&main_path, main_text.as_bytes())
        .with_context(|| format!("Failed to write '{}'.", main_path.display()))?;

    write_assets_for_targets(targets, assets, out_dir)?;
    Ok(())
}

fn write_assets_for_targets(
    targets: &[DecompiledTarget],
    assets: &HashMap<String, Vec<u8>>,
    out_dir: &Path,
) -> Result<()> {
    let mut needed = HashSet::new();
    for target in targets {
        for costume in &target.costumes {
            needed.insert(costume.clone());
        }
    }
    for asset_name in needed {
        if let Some(bytes) = assets.get(&asset_name) {
            let path = out_dir.join(&asset_name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, bytes)?;
        }
    }
    Ok(())
}

fn unique_sprite_filename(name: &str, used: &mut HashSet<String>) -> String {
    let mut base = sanitize_filename(name);
    if base.is_empty() {
        base = "sprite".to_string();
    }
    let mut candidate = format!("{}.sbtext", base);
    let mut index = 2usize;
    while !used.insert(candidate.to_lowercase()) {
        candidate = format!("{}_{}.sbtext", base, index);
        index += 1;
    }
    candidate
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn default_split_output_dir(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("project");
    input
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("{}_sbtext", stem))
}
