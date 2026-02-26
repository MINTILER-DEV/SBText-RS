use crate::ast::{
    EventScript, EventType, Expr, ListDecl, Position, Procedure, Project, Statement, Target,
    VariableDecl,
};
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Cursor;
use std::io::Write;
use std::path::Path;
use xmltree::{Element, XMLNode};
use zip::write::SimpleFileOptions;

const DEFAULT_STAGE_SVG: &str =
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1" viewBox="0 0 1 1"></svg>"##;
const DEFAULT_SPRITE_SVG: &str =
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1" viewBox="0 0 1 1"></svg>"##;
const DEFAULT_SVG_TARGET_SIZE: f64 = 64.0;

type CodegenProgressCallback<'a> = dyn FnMut(usize, usize, &str) + 'a;

#[derive(Debug, Clone, Copy)]
pub struct CodegenOptions {
    pub scale_svgs: bool,
}

impl Default for CodegenOptions {
    fn default() -> Self {
        Self { scale_svgs: true }
    }
}

pub fn write_sb3(
    project: &Project,
    source_dir: &Path,
    output_path: &Path,
    options: CodegenOptions,
) -> Result<()> {
    write_sb3_with_progress(
        project,
        source_dir,
        output_path,
        options,
        Option::<&mut fn(usize, usize, &str)>::None,
    )
}

pub fn write_sb3_with_progress<F>(
    project: &Project,
    source_dir: &Path,
    output_path: &Path,
    options: CodegenOptions,
    progress: Option<&mut F>,
) -> Result<()>
where
    F: FnMut(usize, usize, &str),
{
    let bytes = build_sb3_bytes_with_progress(project, source_dir, options, progress)?;
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output_path, bytes)?;
    Ok(())
}

pub fn build_sb3_bytes(
    project: &Project,
    source_dir: &Path,
    options: CodegenOptions,
) -> Result<Vec<u8>> {
    build_sb3_bytes_with_progress(
        project,
        source_dir,
        options,
        Option::<&mut fn(usize, usize, &str)>::None,
    )
}

pub fn build_sb3_bytes_with_progress<F>(
    project: &Project,
    source_dir: &Path,
    options: CodegenOptions,
    progress: Option<&mut F>,
) -> Result<Vec<u8>>
where
    F: FnMut(usize, usize, &str),
{
    let mut progress = progress.map(|cb| cb as &mut CodegenProgressCallback<'_>);
    let mut builder = ProjectBuilder::new(project, source_dir, options);
    let (project_json, assets) = builder.build_with_progress(&mut progress)?;
    let mut buffer = Cursor::new(Vec::<u8>::new());
    let mut zip = zip::ZipWriter::new(&mut buffer);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    report_progress(&mut progress, 1, 1, "Writing project.json");
    zip.start_file("project.json", opts)?;
    let project_bytes = serde_json::to_vec_pretty(&project_json)?;
    zip.write_all(&project_bytes)?;

    let mut assets = assets.into_iter().collect::<Vec<_>>();
    assets.sort_by(|(left_name, _), (right_name, _)| left_name.cmp(right_name));
    let asset_total = assets.len().max(1);
    if assets.is_empty() {
        report_progress(&mut progress, 1, 1, "Packaging assets");
    }
    for (index, (name, bytes)) in assets.into_iter().enumerate() {
        zip.start_file(name, opts)?;
        zip.write_all(&bytes)?;
        report_progress(&mut progress, index + 1, asset_total, "Packaging assets");
    }
    zip.finish()?;
    Ok(buffer.into_inner())
}

fn report_progress(
    progress: &mut Option<&mut CodegenProgressCallback<'_>>,
    step: usize,
    total: usize,
    label: &str,
) {
    if let Some(cb) = progress.as_deref_mut() {
        cb(step, total, label);
    }
}

#[derive(Debug, Clone)]
struct ProcedureSignature {
    params: Vec<String>,
    arg_ids: Vec<String>,
    proccode: String,
    warp: bool,
}

#[derive(Debug, Clone)]
struct RemoteCallSpec {
    callee_target_lower: String,
    procedure_lower: String,
    procedure_name: String,
    message: String,
    arg_var_names: Vec<String>,
}

#[derive(Debug, Clone)]
struct EmittedStatement {
    first: String,
    last: String,
}

struct ProjectBuilder<'a> {
    project: &'a Project,
    source_dir: &'a Path,
    options: CodegenOptions,
    id_counter: usize,
    assets: HashMap<String, Vec<u8>>,
    broadcast_ids: HashMap<String, String>,
    remote_calls: Vec<RemoteCallSpec>,
    global_var_ids: HashMap<String, String>,
    global_var_names: HashMap<String, String>,
    global_list_ids: HashMap<String, String>,
    global_list_names: HashMap<String, String>,
}

impl<'a> ProjectBuilder<'a> {
    fn new(project: &'a Project, source_dir: &'a Path, options: CodegenOptions) -> Self {
        Self {
            project,
            source_dir,
            options,
            id_counter: 0,
            assets: HashMap::new(),
            broadcast_ids: HashMap::new(),
            remote_calls: Vec::new(),
            global_var_ids: HashMap::new(),
            global_var_names: HashMap::new(),
            global_list_ids: HashMap::new(),
            global_list_names: HashMap::new(),
        }
    }

    fn build_with_progress(
        &mut self,
        progress: &mut Option<&mut CodegenProgressCallback<'_>>,
    ) -> Result<(Value, HashMap<String, Vec<u8>>)> {
        self.broadcast_ids = self.collect_broadcast_ids();
        self.remote_calls = self.collect_remote_call_specs()?;
        self.register_remote_call_broadcasts();
        self.allocate_generated_global_vars();

        let mut ordered_targets = self.project.targets.clone();
        ordered_targets.sort_by_key(|t| if t.is_stage { 0 } else { 1 });
        if !ordered_targets.iter().any(|t| t.is_stage) {
            ordered_targets.insert(0, self.synthesized_stage_target(&ordered_targets));
        }
        self.register_declared_stage_globals(&ordered_targets);

        let mut targets_json = Vec::new();
        let mut sprite_layer = 1;
        if ordered_targets.is_empty() {
            report_progress(progress, 1, 1, "Emitting targets");
        }
        for (index, target) in ordered_targets.iter().enumerate() {
            let layer = if target.is_stage {
                0
            } else {
                let out = sprite_layer;
                sprite_layer += 1;
                out
            };
            targets_json.push(self.build_target_json(target, layer)?);
            report_progress(
                progress,
                index + 1,
                ordered_targets.len().max(1),
                "Emitting targets",
            );
        }

        let extensions = self.collect_extensions();
        let project_json = json!({
            "targets": targets_json,
            "monitors": [],
            "extensions": extensions,
            "meta": {
                "semver": "3.0.0",
                "vm": "0.2.0",
                "agent": "SBText Rust Compiler"
            }
        });
        Ok((project_json, std::mem::take(&mut self.assets)))
    }

    fn synthesized_stage_target(&self, existing: &[Target]) -> Target {
        let mut names = HashSet::new();
        for t in existing {
            names.insert(t.name.to_lowercase());
        }
        let mut stage_name = "Stage".to_string();
        let mut suffix = 1;
        while names.contains(&stage_name.to_lowercase()) {
            suffix += 1;
            stage_name = format!("Stage{}", suffix);
        }
        Target {
            pos: Position::new(0, 0),
            name: stage_name,
            is_stage: true,
            variables: Vec::<VariableDecl>::new(),
            lists: Vec::<ListDecl>::new(),
            costumes: Vec::new(),
            procedures: Vec::<Procedure>::new(),
            scripts: Vec::<EventScript>::new(),
        }
    }

    fn build_target_json(&mut self, target: &Target, layer_order: i32) -> Result<Value> {
        let mut blocks: Map<String, Value> = Map::new();
        let mut local_variables_map: HashMap<String, String> = HashMap::new();
        let mut variables_json: Map<String, Value> = Map::new();
        let mut lists_map: HashMap<String, String> = HashMap::new();
        let mut lists_json: Map<String, Value> = Map::new();

        for var_decl in &target.variables {
            let key = var_decl.name.to_lowercase();
            if local_variables_map.contains_key(&key) {
                continue;
            }
            let var_id = if target.is_stage {
                self.global_var_ids
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| self.new_id("var"))
            } else {
                self.new_id("var")
            };
            local_variables_map.insert(key, var_id.clone());
            variables_json.insert(var_id, json!([var_decl.name, 0]));
        }
        if target.is_stage {
            for (var_lower, var_id) in &self.global_var_ids {
                let var_name = self.global_var_names.get(var_lower).ok_or_else(|| {
                    anyhow!("Missing generated global var name for '{}'.", var_lower)
                })?;
                variables_json.insert(var_id.clone(), json!([var_name, 0]));
            }
        }
        for list_decl in &target.lists {
            let key = list_decl.name.to_lowercase();
            if lists_map.contains_key(&key) {
                continue;
            }
            let list_id = if target.is_stage {
                self.global_list_ids
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| self.new_id("list"))
            } else {
                self.new_id("list")
            };
            lists_map.insert(key, list_id.clone());
            lists_json.insert(list_id, json!([list_decl.name, []]));
        }

        let mut variables_map = local_variables_map.clone();
        for (k, v) in &self.global_var_ids {
            variables_map.insert(k.clone(), v.clone());
        }
        for (k, v) in &self.global_list_ids {
            lists_map.insert(k.clone(), v.clone());
        }

        let signatures = self.build_procedure_signatures(target);
        let mut y_cursor: i32 = 30;
        for procedure in &target.procedures {
            y_cursor = self.emit_procedure_definition(
                &mut blocks,
                procedure,
                &signatures,
                &variables_map,
                &lists_map,
                y_cursor,
            )?;
            y_cursor += 40;
        }
        for script in &target.scripts {
            y_cursor = self.emit_event_script(
                &mut blocks,
                script,
                &signatures,
                &variables_map,
                &lists_map,
                y_cursor,
            )?;
            y_cursor += 40;
        }
        let _ = self.emit_remote_call_handlers(
            &mut blocks,
            target,
            &signatures,
            &variables_map,
            &lists_map,
            y_cursor,
        )?;

        let costumes = self.build_costumes(target)?;
        let stage_broadcasts = if target.is_stage {
            let mut m = Map::new();
            for (msg, id) in &self.broadcast_ids {
                m.insert(id.clone(), Value::String(msg.clone()));
            }
            Value::Object(m)
        } else {
            Value::Object(Map::new())
        };

        let mut target_json = json!({
            "isStage": target.is_stage,
            "name": target.name,
            "variables": variables_json,
            "lists": lists_json,
            "broadcasts": stage_broadcasts,
            "blocks": blocks,
            "comments": {},
            "currentCostume": 0,
            "costumes": costumes,
            "sounds": [],
            "volume": 100,
            "layerOrder": layer_order
        });
        if target.is_stage {
            merge_object(
                &mut target_json,
                json!({
                    "tempo": 60,
                    "videoTransparency": 50,
                    "videoState": "on",
                    "textToSpeechLanguage": Value::Null
                }),
            )?;
        } else {
            merge_object(
                &mut target_json,
                json!({
                    "visible": true,
                    "x": 0,
                    "y": 0,
                    "size": 100,
                    "direction": 90,
                    "draggable": false,
                    "rotationStyle": "all around"
                }),
            )?;
        }
        Ok(target_json)
    }

    fn build_procedure_signatures(
        &mut self,
        target: &Target,
    ) -> HashMap<String, ProcedureSignature> {
        let mut signatures = HashMap::new();
        for procedure in &target.procedures {
            let arg_ids = procedure
                .params
                .iter()
                .map(|_| self.new_id("arg"))
                .collect::<Vec<_>>();
            let placeholders = procedure
                .params
                .iter()
                .map(|_| "%s")
                .collect::<Vec<_>>()
                .join(" ");
            let proccode = if placeholders.is_empty() {
                procedure.name.clone()
            } else {
                format!("{} {}", procedure.name, placeholders)
            };
            signatures.insert(
                procedure.name.to_lowercase(),
                ProcedureSignature {
                    params: procedure.params.clone(),
                    arg_ids,
                    proccode,
                    warp: procedure.run_without_screen_refresh,
                },
            );
        }
        signatures
    }

    fn collect_extensions(&self) -> Vec<String> {
        let mut extensions = Vec::new();
        if self
            .project
            .targets
            .iter()
            .any(|target| target_uses_pen_extension(target))
        {
            extensions.push("pen".to_string());
        }
        extensions
    }

    fn collect_remote_call_specs(&self) -> Result<Vec<RemoteCallSpec>> {
        let mut local_procs: HashMap<String, (String, String, usize)> = HashMap::new();
        for target in &self.project.targets {
            let target_lower = target.name.to_lowercase();
            for procedure in &target.procedures {
                local_procs.insert(
                    format!("{}.{}", target_lower, procedure.name.to_lowercase()),
                    (
                        target.name.clone(),
                        procedure.name.clone(),
                        procedure.params.len(),
                    ),
                );
            }
        }

        let mut out: HashMap<String, RemoteCallSpec> = HashMap::new();
        for target in &self.project.targets {
            for script in &target.scripts {
                self.collect_remote_calls_from_statements(&script.body, &local_procs, &mut out)?;
            }
            for procedure in &target.procedures {
                self.collect_remote_calls_from_statements(&procedure.body, &local_procs, &mut out)?;
            }
        }

        let mut specs = out.into_values().collect::<Vec<_>>();
        specs.sort_by(|a, b| a.message.cmp(&b.message));
        Ok(specs)
    }

    fn collect_remote_calls_from_statements(
        &self,
        statements: &[Statement],
        local_procs: &HashMap<String, (String, String, usize)>,
        out: &mut HashMap<String, RemoteCallSpec>,
    ) -> Result<()> {
        for stmt in statements {
            match stmt {
                Statement::ProcedureCall { name, args, .. } => {
                    if local_procs.contains_key(&name.to_lowercase()) {
                        continue;
                    }
                    if let Some((target_name, proc_name)) = split_qualified(name) {
                        let key = format!(
                            "{}.{}",
                            target_name.to_lowercase(),
                            proc_name.to_lowercase()
                        );
                        let Some((_target_display, proc_display, expected_args)) =
                            local_procs.get(&key)
                        else {
                            continue;
                        };
                        if *expected_args != args.len() {
                            bail!(
                                "Remote procedure '{}' expects {} args, got {}.",
                                name,
                                expected_args,
                                args.len()
                            );
                        }
                        out.entry(key.clone()).or_insert_with(|| {
                            let arg_var_names = (0..*expected_args)
                                .map(|i| {
                                    format!(
                                        "__rpc__{}__{}__arg{}",
                                        target_name.to_lowercase(),
                                        proc_name.to_lowercase(),
                                        i + 1
                                    )
                                })
                                .collect::<Vec<_>>();
                            RemoteCallSpec {
                                callee_target_lower: target_name.to_lowercase(),
                                procedure_lower: proc_name.to_lowercase(),
                                procedure_name: proc_display.clone(),
                                message: format!(
                                    "__rpc__{}__{}",
                                    target_name.to_lowercase(),
                                    proc_name.to_lowercase()
                                ),
                                arg_var_names,
                            }
                        });
                    }
                }
                Statement::Repeat { body, .. }
                | Statement::ForEach { body, .. }
                | Statement::While { body, .. }
                | Statement::RepeatUntil { body, .. }
                | Statement::Forever { body, .. } => {
                    self.collect_remote_calls_from_statements(body, local_procs, out)?;
                }
                Statement::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    self.collect_remote_calls_from_statements(then_body, local_procs, out)?;
                    self.collect_remote_calls_from_statements(else_body, local_procs, out)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn register_remote_call_broadcasts(&mut self) {
        let remote_calls = self.remote_calls.clone();
        for spec in &remote_calls {
            if !self.broadcast_ids.contains_key(&spec.message) {
                let id = self.new_id("broadcast");
                self.broadcast_ids.insert(spec.message.clone(), id);
            }
        }
    }

    fn allocate_generated_global_vars(&mut self) {
        let remote_calls = self.remote_calls.clone();
        for spec in &remote_calls {
            for var_name in &spec.arg_var_names {
                let key = var_name.to_lowercase();
                if self.global_var_ids.contains_key(&key) {
                    continue;
                }
                let id = self.new_id("gvar");
                self.global_var_ids.insert(key.clone(), id);
                self.global_var_names.insert(key, var_name.clone());
            }
        }
    }

    fn register_declared_stage_globals(&mut self, ordered_targets: &[Target]) {
        for target in ordered_targets {
            if !target.is_stage {
                continue;
            }
            for var_decl in &target.variables {
                let key = var_decl.name.to_lowercase();
                if self.global_var_ids.contains_key(&key) {
                    continue;
                }
                let id = self.new_id("gvar");
                self.global_var_ids.insert(key.clone(), id);
                self.global_var_names.insert(key, var_decl.name.clone());
            }
            for list_decl in &target.lists {
                let key = list_decl.name.to_lowercase();
                if self.global_list_ids.contains_key(&key) {
                    continue;
                }
                let id = self.new_id("glist");
                self.global_list_ids.insert(key.clone(), id);
                self.global_list_names.insert(key, list_decl.name.clone());
            }
        }
    }

    fn lookup_remote_call_spec(
        &self,
        callee_target: &str,
        callee_proc: &str,
        arg_count: usize,
    ) -> Result<&RemoteCallSpec> {
        let target_lower = callee_target.to_lowercase();
        let proc_lower = callee_proc.to_lowercase();
        let spec = self
            .remote_calls
            .iter()
            .find(|s| s.callee_target_lower == target_lower && s.procedure_lower == proc_lower)
            .ok_or_else(|| {
                anyhow!(
                    "Unknown remote procedure '{}.{}'.",
                    callee_target,
                    callee_proc
                )
            })?;
        if spec.arg_var_names.len() != arg_count {
            bail!(
                "Remote procedure '{}.{}' expects {} args, got {}.",
                callee_target,
                callee_proc,
                spec.arg_var_names.len(),
                arg_count
            );
        }
        Ok(spec)
    }

    fn emit_remote_call_handlers(
        &mut self,
        blocks: &mut Map<String, Value>,
        target: &Target,
        signatures: &HashMap<String, ProcedureSignature>,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        mut start_y: i32,
    ) -> Result<i32> {
        let target_lower = target.name.to_lowercase();
        let handlers = self
            .remote_calls
            .iter()
            .filter(|s| s.callee_target_lower == target_lower)
            .cloned()
            .collect::<Vec<_>>();
        for handler in handlers {
            let hat_id = self.new_block_id();
            let bid = self.broadcast_id(&handler.message);
            blocks.insert(
                hat_id.clone(),
                json!({
                    "opcode": "event_whenbroadcastreceived",
                    "next": Value::Null,
                    "parent": Value::Null,
                    "inputs": {},
                    "fields": {"BROADCAST_OPTION": [handler.message, bid]},
                    "shadow": false,
                    "topLevel": true,
                    "x": 580,
                    "y": start_y
                }),
            );

            let args = handler
                .arg_var_names
                .iter()
                .map(|name| Expr::Var {
                    pos: target.pos,
                    name: name.clone(),
                })
                .collect::<Vec<_>>();
            let emitted = self.emit_call_stmt(
                blocks,
                &hat_id,
                &handler.procedure_name,
                &args,
                signatures,
                variables_map,
                lists_map,
                &HashSet::new(),
            )?;
            set_block_next(blocks, &hat_id, Value::String(emitted.first))?;
            start_y += 140;
        }
        Ok(start_y)
    }

    fn new_id(&mut self, prefix: &str) -> String {
        self.id_counter += 1;
        format!("{}_{}", prefix, self.id_counter)
    }

    fn new_block_id(&mut self) -> String {
        self.new_id("block")
    }

    fn collect_broadcast_ids(&mut self) -> HashMap<String, String> {
        let mut messages = HashSet::new();
        for target in &self.project.targets {
            for script in &target.scripts {
                if let EventType::WhenIReceive(msg) = &script.event_type {
                    messages.insert(msg.clone());
                }
                collect_messages_from_statements(&script.body, &mut messages);
            }
            for procedure in &target.procedures {
                collect_messages_from_statements(&procedure.body, &mut messages);
            }
        }
        let mut map = HashMap::new();
        let mut sorted = messages.into_iter().collect::<Vec<_>>();
        sorted.sort();
        for msg in sorted {
            map.insert(msg, self.new_id("broadcast"));
        }
        map
    }

    fn broadcast_id(&mut self, message: &str) -> String {
        if let Some(id) = self.broadcast_ids.get(message) {
            return id.clone();
        }
        let id = self.new_id("broadcast");
        self.broadcast_ids.insert(message.to_string(), id.clone());
        id
    }

    fn emit_procedure_definition(
        &mut self,
        blocks: &mut Map<String, Value>,
        procedure: &Procedure,
        signatures: &HashMap<String, ProcedureSignature>,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        start_y: i32,
    ) -> Result<i32> {
        let signature = signatures
            .get(&procedure.name.to_lowercase())
            .ok_or_else(|| anyhow!("Missing procedure signature for '{}'.", procedure.name))?;
        let definition_id = self.new_block_id();
        let prototype_id = self.new_block_id();
        blocks.insert(
            definition_id.clone(),
            json!({
                "opcode": "procedures_definition",
                "next": Value::Null,
                "parent": Value::Null,
                "inputs": { "custom_block": [1, prototype_id.clone()]},
                "fields": {},
                "shadow": false,
                "topLevel": true,
                "x": 30,
                "y": start_y
            }),
        );

        let mut prototype_inputs = Map::new();
        for (param_name, arg_id) in signature.params.iter().zip(signature.arg_ids.iter()) {
            let reporter_id = self.new_block_id();
            blocks.insert(
                reporter_id.clone(),
                json!({
                    "opcode": "argument_reporter_string_number",
                    "next": Value::Null,
                    "parent": prototype_id.clone(),
                    "inputs": {},
                    "fields": { "VALUE": [param_name, Value::Null]},
                    "shadow": true,
                    "topLevel": false
                }),
            );
            prototype_inputs.insert(arg_id.clone(), json!([1, reporter_id]));
        }
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
                    "argumentids": serde_json::to_string(&signature.arg_ids)?,
                    "argumentnames": serde_json::to_string(&signature.params)?,
                    "argumentdefaults": serde_json::to_string(&vec![""; signature.params.len()])?,
                    "warp": if signature.warp { "true" } else { "false" }
                }
            }),
        );
        let (first, last) = self.emit_statement_chain(
            blocks,
            &procedure.body,
            &definition_id,
            variables_map,
            lists_map,
            signatures,
            &signature
                .params
                .iter()
                .map(|s| s.to_lowercase())
                .collect::<HashSet<_>>(),
        )?;
        if let Some(fid) = first {
            set_block_next(blocks, &definition_id, Value::String(fid))?;
            return Ok(start_y + 120 + if last.is_some() { 20 } else { 0 });
        }
        Ok(start_y + 80)
    }

    fn emit_event_script(
        &mut self,
        blocks: &mut Map<String, Value>,
        script: &EventScript,
        signatures: &HashMap<String, ProcedureSignature>,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        start_y: i32,
    ) -> Result<i32> {
        let (opcode, fields) = match &script.event_type {
            EventType::WhenFlagClicked => ("event_whenflagclicked", json!({})),
            EventType::WhenThisSpriteClicked => ("event_whenthisspriteclicked", json!({})),
            EventType::WhenIReceive(msg) => {
                let bid = self.broadcast_id(msg);
                (
                    "event_whenbroadcastreceived",
                    json!({"BROADCAST_OPTION": [msg.clone(), bid]}),
                )
            }
        };
        let hat_id = self.new_block_id();
        blocks.insert(
            hat_id.clone(),
            json!({
                "opcode": opcode,
                "next": Value::Null,
                "parent": Value::Null,
                "inputs": {},
                "fields": fields,
                "shadow": false,
                "topLevel": true,
                "x": 320,
                "y": start_y
            }),
        );
        let (first, last) = self.emit_statement_chain(
            blocks,
            &script.body,
            &hat_id,
            variables_map,
            lists_map,
            signatures,
            &HashSet::new(),
        )?;
        if let Some(fid) = first {
            set_block_next(blocks, &hat_id, Value::String(fid))?;
            return Ok(start_y + 120 + if last.is_some() { 20 } else { 0 });
        }
        Ok(start_y + 80)
    }

    fn emit_statement_chain(
        &mut self,
        blocks: &mut Map<String, Value>,
        statements: &[Statement],
        parent_id: &str,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        signatures: &HashMap<String, ProcedureSignature>,
        param_scope: &HashSet<String>,
    ) -> Result<(Option<String>, Option<String>)> {
        let mut first: Option<String> = None;
        let mut prev_last: Option<String> = None;
        for stmt in statements {
            let stmt_parent = prev_last.clone().unwrap_or_else(|| parent_id.to_string());
            let emitted = self.emit_statement(
                blocks,
                stmt,
                &stmt_parent,
                variables_map,
                lists_map,
                signatures,
                param_scope,
            )?;
            if let Some(prev_id) = &prev_last {
                set_block_next(blocks, prev_id, Value::String(emitted.first.clone()))?;
            }
            if first.is_none() {
                first = Some(emitted.first.clone());
            }
            prev_last = Some(emitted.last);
        }
        Ok((first, prev_last))
    }

    fn emit_statement(
        &mut self,
        blocks: &mut Map<String, Value>,
        stmt: &Statement,
        parent_id: &str,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        signatures: &HashMap<String, ProcedureSignature>,
        param_scope: &HashSet<String>,
    ) -> Result<EmittedStatement> {
        let single = |id: String| EmittedStatement {
            first: id.clone(),
            last: id,
        };
        match stmt {
            Statement::Broadcast { message, .. } => Ok(single(
                self.emit_broadcast_stmt(blocks, parent_id, message)?,
            )),
            Statement::BroadcastAndWait { message, .. } => Ok(single(
                self.emit_broadcast_and_wait_stmt(blocks, parent_id, message)?,
            )),
            Statement::SetVar {
                var_name, value, ..
            } => Ok(single(self.emit_set_stmt(
                blocks,
                parent_id,
                var_name,
                value,
                variables_map,
                lists_map,
                param_scope,
            )?)),
            Statement::ChangeVar {
                var_name, delta, ..
            } => Ok(single(self.emit_change_stmt(
                blocks,
                parent_id,
                var_name,
                delta,
                variables_map,
                lists_map,
                param_scope,
            )?)),
            Statement::Move { steps, .. } => Ok(single(self.emit_single_input_stmt(
                blocks,
                parent_id,
                "motion_movesteps",
                "STEPS",
                steps,
                variables_map,
                lists_map,
                param_scope,
                "number",
            )?)),
            Statement::Say { message, .. } => Ok(single(self.emit_single_input_stmt(
                blocks,
                parent_id,
                "looks_say",
                "MESSAGE",
                message,
                variables_map,
                lists_map,
                param_scope,
                "string",
            )?)),
            Statement::SayForSeconds {
                message, duration, ..
            } => Ok(single(self.emit_say_for_seconds_stmt(
                blocks,
                parent_id,
                message,
                duration,
                variables_map,
                lists_map,
                param_scope,
            )?)),
            Statement::Think { message, .. } => Ok(single(self.emit_single_input_stmt(
                blocks,
                parent_id,
                "looks_think",
                "MESSAGE",
                message,
                variables_map,
                lists_map,
                param_scope,
                "string",
            )?)),
            Statement::TurnRight { degrees, .. } => Ok(single(self.emit_single_input_stmt(
                blocks,
                parent_id,
                "motion_turnright",
                "DEGREES",
                degrees,
                variables_map,
                lists_map,
                param_scope,
                "number",
            )?)),
            Statement::TurnLeft { degrees, .. } => Ok(single(self.emit_single_input_stmt(
                blocks,
                parent_id,
                "motion_turnleft",
                "DEGREES",
                degrees,
                variables_map,
                lists_map,
                param_scope,
                "number",
            )?)),
            Statement::GoToXY { x, y, .. } => Ok(single(self.emit_go_to_xy_stmt(
                blocks,
                parent_id,
                x,
                y,
                variables_map,
                lists_map,
                param_scope,
            )?)),
            Statement::GoToTarget { target, .. } => Ok(single(self.emit_motion_target_menu_stmt(
                blocks,
                parent_id,
                "motion_goto",
                "TO",
                "motion_goto_menu",
                "TO",
                target,
                "_random_",
            )?)),
            Statement::GlideToXY { duration, x, y, .. } => Ok(single(self.emit_glide_to_xy_stmt(
                blocks,
                parent_id,
                duration,
                x,
                y,
                variables_map,
                lists_map,
                param_scope,
            )?)),
            Statement::GlideToTarget {
                duration, target, ..
            } => Ok(single(self.emit_glide_to_target_stmt(
                blocks,
                parent_id,
                duration,
                target,
                variables_map,
                lists_map,
                param_scope,
            )?)),
            Statement::ChangeXBy { value, .. } => Ok(single(self.emit_single_input_stmt(
                blocks,
                parent_id,
                "motion_changexby",
                "DX",
                value,
                variables_map,
                lists_map,
                param_scope,
                "number",
            )?)),
            Statement::SetX { value, .. } => Ok(single(self.emit_single_input_stmt(
                blocks,
                parent_id,
                "motion_setx",
                "X",
                value,
                variables_map,
                lists_map,
                param_scope,
                "number",
            )?)),
            Statement::ChangeYBy { value, .. } => Ok(single(self.emit_single_input_stmt(
                blocks,
                parent_id,
                "motion_changeyby",
                "DY",
                value,
                variables_map,
                lists_map,
                param_scope,
                "number",
            )?)),
            Statement::SetY { value, .. } => Ok(single(self.emit_single_input_stmt(
                blocks,
                parent_id,
                "motion_sety",
                "Y",
                value,
                variables_map,
                lists_map,
                param_scope,
                "number",
            )?)),
            Statement::PointInDirection { direction, .. } => {
                Ok(single(self.emit_single_input_stmt(
                    blocks,
                    parent_id,
                    "motion_pointindirection",
                    "DIRECTION",
                    direction,
                    variables_map,
                    lists_map,
                    param_scope,
                    "number",
                )?))
            }
            Statement::PointTowards { target, .. } => {
                Ok(single(self.emit_motion_target_menu_stmt(
                    blocks,
                    parent_id,
                    "motion_pointtowards",
                    "TOWARDS",
                    "motion_pointtowards_menu",
                    "TOWARDS",
                    target,
                    "_mouse_",
                )?))
            }
            Statement::SetRotationStyle { style, .. } => Ok(single(
                self.emit_set_rotation_style_stmt(blocks, parent_id, style)?,
            )),
            Statement::IfOnEdgeBounce { .. } => Ok(single(self.emit_no_input_stmt(
                blocks,
                parent_id,
                "motion_ifonedgebounce",
            )?)),
            Statement::ChangeSizeBy { value, .. } => Ok(single(self.emit_single_input_stmt(
                blocks,
                parent_id,
                "looks_changesizeby",
                "CHANGE",
                value,
                variables_map,
                lists_map,
                param_scope,
                "number",
            )?)),
            Statement::SetSizeTo { value, .. } => Ok(single(self.emit_single_input_stmt(
                blocks,
                parent_id,
                "looks_setsizeto",
                "SIZE",
                value,
                variables_map,
                lists_map,
                param_scope,
                "number",
            )?)),
            Statement::ClearGraphicEffects { .. } => Ok(single(self.emit_no_input_stmt(
                blocks,
                parent_id,
                "looks_cleargraphiceffects",
            )?)),
            Statement::SetGraphicEffectTo { effect, value, .. } => {
                Ok(single(self.emit_looks_effect_stmt(
                    blocks,
                    parent_id,
                    "looks_seteffectto",
                    "VALUE",
                    "EFFECT",
                    effect,
                    value,
                    variables_map,
                    lists_map,
                    param_scope,
                )?))
            }
            Statement::ChangeGraphicEffectBy { effect, value, .. } => {
                Ok(single(self.emit_looks_effect_stmt(
                    blocks,
                    parent_id,
                    "looks_changeeffectby",
                    "CHANGE",
                    "EFFECT",
                    effect,
                    value,
                    variables_map,
                    lists_map,
                    param_scope,
                )?))
            }
            Statement::GoToLayer { layer, .. } => Ok(single(
                self.emit_looks_layer_stmt(blocks, parent_id, layer)?,
            )),
            Statement::GoLayers {
                direction, layers, ..
            } => Ok(single(self.emit_looks_go_layers_stmt(
                blocks,
                parent_id,
                direction,
                layers,
                variables_map,
                lists_map,
                param_scope,
            )?)),
            Statement::PenDown { .. } => Ok(single(self.emit_no_input_stmt(
                blocks,
                parent_id,
                "pen_penDown",
            )?)),
            Statement::PenUp { .. } => Ok(single(self.emit_no_input_stmt(
                blocks,
                parent_id,
                "pen_penUp",
            )?)),
            Statement::PenClear { .. } => Ok(single(self.emit_no_input_stmt(
                blocks,
                parent_id,
                "pen_clear",
            )?)),
            Statement::PenStamp { .. } => Ok(single(self.emit_no_input_stmt(
                blocks,
                parent_id,
                "pen_stamp",
            )?)),
            Statement::ChangePenSizeBy { value, .. } => Ok(single(self.emit_single_input_stmt(
                blocks,
                parent_id,
                "pen_changePenSizeBy",
                "SIZE",
                value,
                variables_map,
                lists_map,
                param_scope,
                "number",
            )?)),
            Statement::SetPenSizeTo { value, .. } => Ok(single(self.emit_single_input_stmt(
                blocks,
                parent_id,
                "pen_setPenSizeTo",
                "SIZE",
                value,
                variables_map,
                lists_map,
                param_scope,
                "number",
            )?)),
            Statement::ChangePenColorParamBy { param, value, .. } => {
                Ok(single(self.emit_pen_color_param_stmt(
                    blocks,
                    parent_id,
                    "pen_changePenColorParamBy",
                    param,
                    value,
                    variables_map,
                    lists_map,
                    param_scope,
                )?))
            }
            Statement::SetPenColorParamTo { param, value, .. } => {
                Ok(single(self.emit_pen_color_param_stmt(
                    blocks,
                    parent_id,
                    "pen_setPenColorParamTo",
                    param,
                    value,
                    variables_map,
                    lists_map,
                    param_scope,
                )?))
            }
            Statement::Show { .. } => Ok(single(self.emit_no_input_stmt(
                blocks,
                parent_id,
                "looks_show",
            )?)),
            Statement::Hide { .. } => Ok(single(self.emit_no_input_stmt(
                blocks,
                parent_id,
                "looks_hide",
            )?)),
            Statement::NextCostume { .. } => Ok(single(self.emit_no_input_stmt(
                blocks,
                parent_id,
                "looks_nextcostume",
            )?)),
            Statement::NextBackdrop { .. } => Ok(single(self.emit_no_input_stmt(
                blocks,
                parent_id,
                "looks_nextbackdrop",
            )?)),
            Statement::SwitchCostumeTo { costume, .. } => Ok(single(self.emit_single_input_stmt(
                blocks,
                parent_id,
                "looks_switchcostumeto",
                "COSTUME",
                costume,
                variables_map,
                lists_map,
                param_scope,
                "string",
            )?)),
            Statement::SwitchBackdropTo { backdrop, .. } => {
                Ok(single(self.emit_single_input_stmt(
                    blocks,
                    parent_id,
                    "looks_switchbackdropto",
                    "BACKDROP",
                    backdrop,
                    variables_map,
                    lists_map,
                    param_scope,
                    "string",
                )?))
            }
            Statement::Wait { duration, .. } => Ok(single(self.emit_single_input_stmt(
                blocks,
                parent_id,
                "control_wait",
                "DURATION",
                duration,
                variables_map,
                lists_map,
                param_scope,
                "number",
            )?)),
            Statement::WaitUntil { condition, .. } => Ok(single(self.emit_wait_until_stmt(
                blocks,
                parent_id,
                condition,
                variables_map,
                lists_map,
                param_scope,
            )?)),
            Statement::Repeat { times, body, .. } => Ok(single(self.emit_repeat_stmt(
                blocks,
                parent_id,
                times,
                body,
                variables_map,
                lists_map,
                signatures,
                param_scope,
            )?)),
            Statement::ForEach {
                var_name,
                value,
                body,
                ..
            } => Ok(single(self.emit_for_each_stmt(
                blocks,
                parent_id,
                var_name,
                value,
                body,
                variables_map,
                lists_map,
                signatures,
                param_scope,
            )?)),
            Statement::While {
                condition, body, ..
            } => Ok(single(self.emit_while_stmt(
                blocks,
                parent_id,
                condition,
                body,
                variables_map,
                lists_map,
                signatures,
                param_scope,
            )?)),
            Statement::RepeatUntil {
                condition, body, ..
            } => Ok(single(self.emit_repeat_until_stmt(
                blocks,
                parent_id,
                condition,
                body,
                variables_map,
                lists_map,
                signatures,
                param_scope,
            )?)),
            Statement::Forever { body, .. } => Ok(single(self.emit_forever_stmt(
                blocks,
                parent_id,
                body,
                variables_map,
                lists_map,
                signatures,
                param_scope,
            )?)),
            Statement::If {
                condition,
                then_body,
                else_body,
                ..
            } => Ok(single(self.emit_if_stmt(
                blocks,
                parent_id,
                condition,
                then_body,
                else_body,
                variables_map,
                lists_map,
                signatures,
                param_scope,
            )?)),
            Statement::Stop { option, .. } => Ok(single(self.emit_stop_stmt(
                blocks,
                parent_id,
                option,
                variables_map,
                lists_map,
                param_scope,
            )?)),
            Statement::Ask { question, .. } => Ok(single(self.emit_single_input_stmt(
                blocks,
                parent_id,
                "sensing_askandwait",
                "QUESTION",
                question,
                variables_map,
                lists_map,
                param_scope,
                "string",
            )?)),
            Statement::StartSound { sound, .. } => Ok(single(self.emit_sound_menu_stmt(
                blocks,
                parent_id,
                "sound_play",
                sound,
                "sound_play",
            )?)),
            Statement::PlaySoundUntilDone { sound, .. } => Ok(single(self.emit_sound_menu_stmt(
                blocks,
                parent_id,
                "sound_playuntildone",
                sound,
                "sound_play",
            )?)),
            Statement::StopAllSounds { .. } => Ok(single(self.emit_no_input_stmt(
                blocks,
                parent_id,
                "sound_stopallsounds",
            )?)),
            Statement::SetSoundEffectTo { effect, value, .. } => {
                Ok(single(self.emit_sound_effect_stmt(
                    blocks,
                    parent_id,
                    effect,
                    value,
                    variables_map,
                    lists_map,
                    param_scope,
                )?))
            }
            Statement::SetVolumeTo { value, .. } => Ok(single(self.emit_single_input_stmt(
                blocks,
                parent_id,
                "sound_setvolumeto",
                "VOLUME",
                value,
                variables_map,
                lists_map,
                param_scope,
                "number",
            )?)),
            Statement::CreateCloneOf { target, .. } => Ok(single(
                self.emit_clone_target_menu_stmt(blocks, parent_id, target)?,
            )),
            Statement::DeleteThisClone { .. } => Ok(single(self.emit_no_input_stmt(
                blocks,
                parent_id,
                "control_delete_this_clone",
            )?)),
            Statement::ShowVariable { var_name, .. } => {
                Ok(single(self.emit_show_hide_variable_stmt(
                    blocks,
                    parent_id,
                    "data_showvariable",
                    var_name,
                    variables_map,
                )?))
            }
            Statement::HideVariable { var_name, .. } => {
                Ok(single(self.emit_show_hide_variable_stmt(
                    blocks,
                    parent_id,
                    "data_hidevariable",
                    var_name,
                    variables_map,
                )?))
            }
            Statement::ResetTimer { .. } => Ok(single(self.emit_no_input_stmt(
                blocks,
                parent_id,
                "sensing_resettimer",
            )?)),
            Statement::AddToList {
                list_name, item, ..
            } => Ok(single(self.emit_add_to_list_stmt(
                blocks,
                parent_id,
                list_name,
                item,
                variables_map,
                lists_map,
                param_scope,
            )?)),
            Statement::DeleteOfList {
                list_name, index, ..
            } => Ok(single(self.emit_delete_of_list_stmt(
                blocks,
                parent_id,
                list_name,
                index,
                variables_map,
                lists_map,
                param_scope,
            )?)),
            Statement::DeleteAllOfList { list_name, .. } => Ok(single(
                self.emit_delete_all_of_list_stmt(blocks, parent_id, list_name, lists_map)?,
            )),
            Statement::InsertAtList {
                list_name,
                item,
                index,
                ..
            } => Ok(single(self.emit_insert_at_list_stmt(
                blocks,
                parent_id,
                list_name,
                item,
                index,
                variables_map,
                lists_map,
                param_scope,
            )?)),
            Statement::ReplaceItemOfList {
                list_name,
                index,
                item,
                ..
            } => Ok(single(self.emit_replace_item_of_list_stmt(
                blocks,
                parent_id,
                list_name,
                index,
                item,
                variables_map,
                lists_map,
                param_scope,
            )?)),
            Statement::ProcedureCall { name, args, .. } => self.emit_call_stmt(
                blocks,
                parent_id,
                name,
                args,
                signatures,
                variables_map,
                lists_map,
                param_scope,
            ),
        }
    }

    fn emit_no_input_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        opcode: &str,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": opcode,
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {},
                "fields": {},
                "shadow": false,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn emit_single_input_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        opcode: &str,
        input_name: &str,
        value: &Expr,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        param_scope: &HashSet<String>,
        default_kind: &str,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        let input = self.expr_input(
            blocks,
            value,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            default_kind,
        )?;
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": opcode,
                "next": Value::Null,
                "parent": parent_id,
                "inputs": { input_name: input },
                "fields": {},
                "shadow": false,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn emit_pen_color_param_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        opcode: &str,
        param: &str,
        value: &Expr,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        param_scope: &HashSet<String>,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        let menu_id = self.new_block_id();
        let value_input = self.expr_input(
            blocks,
            value,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "number",
        )?;
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": opcode,
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {"COLOR_PARAM": [1, menu_id.clone()], "VALUE": value_input},
                "fields": {},
                "shadow": false,
                "topLevel": false
            }),
        );
        blocks.insert(
            menu_id.clone(),
            json!({
                "opcode": "pen_menu_colorParam",
                "next": Value::Null,
                "parent": block_id.clone(),
                "inputs": {},
                "fields": {"colorParam": [param, Value::Null]},
                "shadow": true,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn emit_say_for_seconds_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        message: &Expr,
        duration: &Expr,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        param_scope: &HashSet<String>,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        let message_input = self.expr_input(
            blocks,
            message,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "string",
        )?;
        let secs_input = self.expr_input(
            blocks,
            duration,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "number",
        )?;
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "looks_sayforsecs",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {"MESSAGE": message_input, "SECS": secs_input},
                "fields": {},
                "shadow": false,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn emit_wait_until_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        condition: &Expr,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        param_scope: &HashSet<String>,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        let cond_input = self.expr_input(
            blocks,
            condition,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "boolean",
        )?;
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "control_wait_until",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {"CONDITION": cond_input},
                "fields": {},
                "shadow": false,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn emit_go_to_xy_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        x: &Expr,
        y: &Expr,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        param_scope: &HashSet<String>,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        let x_input = self.expr_input(
            blocks,
            x,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "number",
        )?;
        let y_input = self.expr_input(
            blocks,
            y,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "number",
        )?;
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "motion_gotoxy",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": { "X": x_input, "Y": y_input },
                "fields": {},
                "shadow": false,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn emit_glide_to_xy_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        duration: &Expr,
        x: &Expr,
        y: &Expr,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        param_scope: &HashSet<String>,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        let secs_input = self.expr_input(
            blocks,
            duration,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "number",
        )?;
        let x_input = self.expr_input(
            blocks,
            x,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "number",
        )?;
        let y_input = self.expr_input(
            blocks,
            y,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "number",
        )?;
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "motion_glidesecstoxy",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": { "SECS": secs_input, "X": x_input, "Y": y_input },
                "fields": {},
                "shadow": false,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn emit_glide_to_target_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        duration: &Expr,
        target: &Expr,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        param_scope: &HashSet<String>,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        let menu_id = self.new_block_id();
        let secs_input = self.expr_input(
            blocks,
            duration,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "number",
        )?;
        let target_value = self.menu_text_from_expr(target, "_random_");
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "motion_glideto",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": { "SECS": secs_input, "TO": [1, menu_id.clone()] },
                "fields": {},
                "shadow": false,
                "topLevel": false
            }),
        );
        blocks.insert(
            menu_id,
            json!({
                "opcode": "motion_glideto_menu",
                "next": Value::Null,
                "parent": block_id.clone(),
                "inputs": {},
                "fields": {"TO": [target_value, Value::Null]},
                "shadow": true,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_motion_target_menu_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        opcode: &str,
        input_name: &str,
        menu_opcode: &str,
        field_name: &str,
        target: &Expr,
        fallback: &str,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        let menu_id = self.new_block_id();
        let target_value = self.menu_text_from_expr(target, fallback);
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": opcode,
                "next": Value::Null,
                "parent": parent_id,
                "inputs": { input_name: [1, menu_id.clone()] },
                "fields": {},
                "shadow": false,
                "topLevel": false
            }),
        );
        blocks.insert(
            menu_id,
            json!({
                "opcode": menu_opcode,
                "next": Value::Null,
                "parent": block_id.clone(),
                "inputs": {},
                "fields": {field_name: [target_value, Value::Null]},
                "shadow": true,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn emit_set_rotation_style_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        style: &str,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "motion_setrotationstyle",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {},
                "fields": {"STYLE": [style, Value::Null]},
                "shadow": false,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_looks_effect_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        opcode: &str,
        input_name: &str,
        field_name: &str,
        effect: &str,
        value: &Expr,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        param_scope: &HashSet<String>,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        let value_input = self.expr_input(
            blocks,
            value,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "number",
        )?;
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": opcode,
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {input_name: value_input},
                "fields": {field_name: [effect, Value::Null]},
                "shadow": false,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn emit_looks_layer_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        layer: &str,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "looks_gotofrontback",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {},
                "fields": {"FRONT_BACK": [layer, Value::Null]},
                "shadow": false,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn emit_looks_go_layers_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        direction: &str,
        layers: &Expr,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        param_scope: &HashSet<String>,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        let layers_input = self.expr_input(
            blocks,
            layers,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "number",
        )?;
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "looks_goforwardbackwardlayers",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {"NUM": layers_input},
                "fields": {"FORWARD_BACKWARD": [direction, Value::Null]},
                "shadow": false,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn emit_sound_menu_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        opcode: &str,
        sound: &Expr,
        fallback_sound: &str,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        let menu_id = self.new_block_id();
        let sound_value = self.menu_text_from_expr(sound, fallback_sound);
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": opcode,
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {"SOUND_MENU": [1, menu_id.clone()]},
                "fields": {},
                "shadow": false,
                "topLevel": false
            }),
        );
        blocks.insert(
            menu_id,
            json!({
                "opcode": "sound_sounds_menu",
                "next": Value::Null,
                "parent": block_id.clone(),
                "inputs": {},
                "fields": {"SOUND_MENU": [sound_value, Value::Null]},
                "shadow": true,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn emit_sound_effect_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        effect: &str,
        value: &Expr,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        param_scope: &HashSet<String>,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        let value_input = self.expr_input(
            blocks,
            value,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "number",
        )?;
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "sound_seteffectto",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {"VALUE": value_input},
                "fields": {"EFFECT": [effect, Value::Null]},
                "shadow": false,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn emit_clone_target_menu_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        target: &Expr,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        let menu_id = self.new_block_id();
        let target_value = self.menu_text_from_expr(target, "_myself_");
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "control_create_clone_of",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {"CLONE_OPTION": [1, menu_id.clone()]},
                "fields": {},
                "shadow": false,
                "topLevel": false
            }),
        );
        blocks.insert(
            menu_id,
            json!({
                "opcode": "control_create_clone_of_menu",
                "next": Value::Null,
                "parent": block_id.clone(),
                "inputs": {},
                "fields": {"CLONE_OPTION": [target_value, Value::Null]},
                "shadow": true,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn emit_show_hide_variable_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        opcode: &str,
        var_name: &str,
        variables_map: &HashMap<String, String>,
    ) -> Result<String> {
        let var_id = self.lookup_var_id(variables_map, var_name)?;
        let block_id = self.new_block_id();
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": opcode,
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {},
                "fields": {"VARIABLE": [var_name, var_id]},
                "shadow": false,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn emit_broadcast_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        message: &str,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        let menu_id = self.new_block_id();
        let bid = self.broadcast_id(message);
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "event_broadcast",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {"BROADCAST_INPUT": [1, menu_id.clone()]},
                "fields": {},
                "shadow": false,
                "topLevel": false
            }),
        );
        blocks.insert(
            menu_id.clone(),
            json!({
                "opcode": "event_broadcast_menu",
                "next": Value::Null,
                "parent": block_id,
                "inputs": {},
                "fields": {"BROADCAST_OPTION": [message, bid]},
                "shadow": true,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn emit_set_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        var_name: &str,
        value: &Expr,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        param_scope: &HashSet<String>,
    ) -> Result<String> {
        let var_id = self.lookup_var_id(variables_map, var_name)?;
        let block_id = self.new_block_id();
        let val_input = self.expr_input(
            blocks,
            value,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "number",
        )?;
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "data_setvariableto",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {"VALUE": val_input},
                "fields": {"VARIABLE": [var_name, var_id]},
                "shadow": false,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn emit_change_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        var_name: &str,
        value: &Expr,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        param_scope: &HashSet<String>,
    ) -> Result<String> {
        let var_id = self.lookup_var_id(variables_map, var_name)?;
        let block_id = self.new_block_id();
        let val_input = self.expr_input(
            blocks,
            value,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "number",
        )?;
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "data_changevariableby",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {"VALUE": val_input},
                "fields": {"VARIABLE": [var_name, var_id]},
                "shadow": false,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn emit_repeat_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        times: &Expr,
        body: &[Statement],
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        signatures: &HashMap<String, ProcedureSignature>,
        param_scope: &HashSet<String>,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        let times_input = self.expr_input(
            blocks,
            times,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "number",
        )?;
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "control_repeat",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {"TIMES": times_input},
                "fields": {},
                "shadow": false,
                "topLevel": false
            }),
        );
        let (sub_first, _) = self.emit_statement_chain(
            blocks,
            body,
            &block_id,
            variables_map,
            lists_map,
            signatures,
            param_scope,
        )?;
        if let Some(substack) = sub_first {
            set_block_input(blocks, &block_id, "SUBSTACK", json!([2, substack]))?;
        }
        Ok(block_id)
    }

    fn emit_for_each_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        var_name: &str,
        value: &Expr,
        body: &[Statement],
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        signatures: &HashMap<String, ProcedureSignature>,
        param_scope: &HashSet<String>,
    ) -> Result<String> {
        let var_id = self.lookup_var_id(variables_map, var_name)?;
        let block_id = self.new_block_id();
        let value_input = self.expr_input(
            blocks,
            value,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "number",
        )?;
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "control_for_each",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {"VALUE": value_input},
                "fields": {"VARIABLE": [var_name, var_id]},
                "shadow": false,
                "topLevel": false
            }),
        );
        let (sub_first, _) = self.emit_statement_chain(
            blocks,
            body,
            &block_id,
            variables_map,
            lists_map,
            signatures,
            param_scope,
        )?;
        if let Some(substack) = sub_first {
            set_block_input(blocks, &block_id, "SUBSTACK", json!([2, substack]))?;
        }
        Ok(block_id)
    }

    fn emit_while_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        condition: &Expr,
        body: &[Statement],
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        signatures: &HashMap<String, ProcedureSignature>,
        param_scope: &HashSet<String>,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        let cond_input = self.expr_input(
            blocks,
            condition,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "boolean",
        )?;
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "control_while",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {"CONDITION": cond_input},
                "fields": {},
                "shadow": false,
                "topLevel": false
            }),
        );
        let (sub_first, _) = self.emit_statement_chain(
            blocks,
            body,
            &block_id,
            variables_map,
            lists_map,
            signatures,
            param_scope,
        )?;
        if let Some(substack) = sub_first {
            set_block_input(blocks, &block_id, "SUBSTACK", json!([2, substack]))?;
        }
        Ok(block_id)
    }

    fn emit_repeat_until_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        condition: &Expr,
        body: &[Statement],
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        signatures: &HashMap<String, ProcedureSignature>,
        param_scope: &HashSet<String>,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        let cond_input = self.expr_input(
            blocks,
            condition,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "boolean",
        )?;
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "control_repeat_until",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {"CONDITION": cond_input},
                "fields": {},
                "shadow": false,
                "topLevel": false
            }),
        );
        let (sub_first, _) = self.emit_statement_chain(
            blocks,
            body,
            &block_id,
            variables_map,
            lists_map,
            signatures,
            param_scope,
        )?;
        if let Some(substack) = sub_first {
            set_block_input(blocks, &block_id, "SUBSTACK", json!([2, substack]))?;
        }
        Ok(block_id)
    }

    fn emit_forever_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        body: &[Statement],
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        signatures: &HashMap<String, ProcedureSignature>,
        param_scope: &HashSet<String>,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "control_forever",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {},
                "fields": {},
                "shadow": false,
                "topLevel": false
            }),
        );
        let (sub_first, _) = self.emit_statement_chain(
            blocks,
            body,
            &block_id,
            variables_map,
            lists_map,
            signatures,
            param_scope,
        )?;
        if let Some(substack) = sub_first {
            set_block_input(blocks, &block_id, "SUBSTACK", json!([2, substack]))?;
        }
        Ok(block_id)
    }

    fn emit_if_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        condition: &Expr,
        then_body: &[Statement],
        else_body: &[Statement],
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        signatures: &HashMap<String, ProcedureSignature>,
        param_scope: &HashSet<String>,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        let cond_input = self.expr_input(
            blocks,
            condition,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "boolean",
        )?;
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "control_if_else",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {"CONDITION": cond_input},
                "fields": {},
                "shadow": false,
                "topLevel": false
            }),
        );
        let (then_first, _) = self.emit_statement_chain(
            blocks,
            then_body,
            &block_id,
            variables_map,
            lists_map,
            signatures,
            param_scope,
        )?;
        let (else_first, _) = self.emit_statement_chain(
            blocks,
            else_body,
            &block_id,
            variables_map,
            lists_map,
            signatures,
            param_scope,
        )?;
        if let Some(first) = then_first {
            set_block_input(blocks, &block_id, "SUBSTACK", json!([2, first]))?;
        }
        if let Some(first) = else_first {
            set_block_input(blocks, &block_id, "SUBSTACK2", json!([2, first]))?;
        }
        Ok(block_id)
    }

    fn emit_stop_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        option: &Expr,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        param_scope: &HashSet<String>,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        let option_text = self
            .literal_input(option)
            .and_then(|v| v.get(1).and_then(|x| x.as_str()).map(|s| s.to_string()))
            .unwrap_or_else(|| "all".to_string());
        let _ = (variables_map, lists_map, param_scope);
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "control_stop",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {},
                "fields": { "STOP_OPTION": [option_text, Value::Null]},
                "shadow": false,
                "topLevel": false,
                "mutation": {"tagName": "mutation", "children": [], "hasnext": "false"}
            }),
        );
        Ok(block_id)
    }

    fn emit_call_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        name: &str,
        args: &[Expr],
        signatures: &HashMap<String, ProcedureSignature>,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        param_scope: &HashSet<String>,
    ) -> Result<EmittedStatement> {
        if !signatures.contains_key(&name.to_lowercase()) {
            if let Some((callee_target, callee_proc)) = split_qualified(name) {
                return self.emit_remote_call_stmt(
                    blocks,
                    parent_id,
                    callee_target,
                    callee_proc,
                    args,
                    variables_map,
                    lists_map,
                    param_scope,
                );
            }
            if is_ignored_noop_call(name) {
                let block_id = self.new_block_id();
                blocks.insert(
                    block_id.clone(),
                    json!({
                        "opcode": "control_wait",
                        "next": Value::Null,
                        "parent": parent_id,
                        "inputs": { "DURATION": [1, [4, "0"]] },
                        "fields": {},
                        "shadow": false,
                        "topLevel": false
                    }),
                );
                return Ok(EmittedStatement {
                    first: block_id.clone(),
                    last: block_id,
                });
            }
        }
        let sig = signatures
            .get(&name.to_lowercase())
            .ok_or_else(|| anyhow!("Unknown procedure '{}' during codegen.", name))?;
        let block_id = self.new_block_id();
        let mut inputs = Map::new();
        for (arg_id, expr) in sig.arg_ids.iter().zip(args.iter()) {
            let val = self.expr_input(
                blocks,
                expr,
                &block_id,
                variables_map,
                lists_map,
                param_scope,
                "string",
            )?;
            inputs.insert(arg_id.clone(), val);
        }
        blocks.insert(
            block_id.clone(),
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
                    "proccode": sig.proccode,
                    "argumentids": serde_json::to_string(&sig.arg_ids)?,
                    "warp": if sig.warp { "true" } else { "false" }
                }
            }),
        );
        Ok(EmittedStatement {
            first: block_id.clone(),
            last: block_id,
        })
    }

    fn emit_remote_call_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        callee_target: &str,
        callee_proc: &str,
        args: &[Expr],
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        param_scope: &HashSet<String>,
    ) -> Result<EmittedStatement> {
        let spec = self
            .lookup_remote_call_spec(callee_target, callee_proc, args.len())?
            .clone();
        let mut first: Option<String> = None;
        let mut prev: Option<String> = None;

        for (idx, expr) in args.iter().enumerate() {
            let arg_var_name = spec.arg_var_names.get(idx).ok_or_else(|| {
                anyhow!(
                    "Internal error: missing RPC arg variable for index {}.",
                    idx
                )
            })?;
            let arg_var_id = self.lookup_var_id(variables_map, arg_var_name)?;
            let block_id = self.new_block_id();
            let val_input = self.expr_input(
                blocks,
                expr,
                &block_id,
                variables_map,
                lists_map,
                param_scope,
                "string",
            )?;
            let parent = prev.clone().unwrap_or_else(|| parent_id.to_string());
            blocks.insert(
                block_id.clone(),
                json!({
                    "opcode": "data_setvariableto",
                    "next": Value::Null,
                    "parent": parent,
                    "inputs": {"VALUE": val_input},
                    "fields": {"VARIABLE": [arg_var_name, arg_var_id]},
                    "shadow": false,
                    "topLevel": false
                }),
            );
            if let Some(prev_id) = &prev {
                set_block_next(blocks, prev_id, Value::String(block_id.clone()))?;
            }
            if first.is_none() {
                first = Some(block_id.clone());
            }
            prev = Some(block_id);
        }

        let parent_for_broadcast = prev.clone().unwrap_or_else(|| parent_id.to_string());
        let broadcast_id =
            self.emit_broadcast_and_wait_stmt(blocks, &parent_for_broadcast, &spec.message)?;
        if let Some(prev_id) = &prev {
            set_block_next(blocks, prev_id, Value::String(broadcast_id.clone()))?;
        } else {
            first = Some(broadcast_id.clone());
        }

        Ok(EmittedStatement {
            first: first.unwrap_or_else(|| broadcast_id.clone()),
            last: broadcast_id,
        })
    }

    fn emit_broadcast_and_wait_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        message: &str,
    ) -> Result<String> {
        let block_id = self.new_block_id();
        let menu_id = self.new_block_id();
        let bid = self.broadcast_id(message);
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "event_broadcastandwait",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {"BROADCAST_INPUT": [1, menu_id.clone()]},
                "fields": {},
                "shadow": false,
                "topLevel": false
            }),
        );
        blocks.insert(
            menu_id,
            json!({
                "opcode": "event_broadcast_menu",
                "next": Value::Null,
                "parent": block_id.clone(),
                "inputs": {},
                "fields": {"BROADCAST_OPTION": [message, bid]},
                "shadow": true,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn emit_add_to_list_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        list_name: &str,
        item: &Expr,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        param_scope: &HashSet<String>,
    ) -> Result<String> {
        let list_id = self.lookup_list_id(lists_map, list_name)?;
        let block_id = self.new_block_id();
        let item_input = self.expr_input(
            blocks,
            item,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "string",
        )?;
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "data_addtolist",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {"ITEM": item_input},
                "fields": {"LIST": [list_name, list_id]},
                "shadow": false,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn emit_delete_of_list_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        list_name: &str,
        index: &Expr,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        param_scope: &HashSet<String>,
    ) -> Result<String> {
        let list_id = self.lookup_list_id(lists_map, list_name)?;
        let block_id = self.new_block_id();
        let index_input = self.expr_input(
            blocks,
            index,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "number",
        )?;
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "data_deleteoflist",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {"INDEX": index_input},
                "fields": {"LIST": [list_name, list_id]},
                "shadow": false,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn emit_delete_all_of_list_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        list_name: &str,
        lists_map: &HashMap<String, String>,
    ) -> Result<String> {
        let list_id = self.lookup_list_id(lists_map, list_name)?;
        let block_id = self.new_block_id();
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "data_deletealloflist",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {},
                "fields": {"LIST": [list_name, list_id]},
                "shadow": false,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn emit_insert_at_list_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        list_name: &str,
        item: &Expr,
        index: &Expr,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        param_scope: &HashSet<String>,
    ) -> Result<String> {
        let list_id = self.lookup_list_id(lists_map, list_name)?;
        let block_id = self.new_block_id();
        let item_input = self.expr_input(
            blocks,
            item,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "string",
        )?;
        let index_input = self.expr_input(
            blocks,
            index,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "number",
        )?;
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "data_insertatlist",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {"ITEM": item_input, "INDEX": index_input},
                "fields": {"LIST": [list_name, list_id]},
                "shadow": false,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn emit_replace_item_of_list_stmt(
        &mut self,
        blocks: &mut Map<String, Value>,
        parent_id: &str,
        list_name: &str,
        index: &Expr,
        item: &Expr,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        param_scope: &HashSet<String>,
    ) -> Result<String> {
        let list_id = self.lookup_list_id(lists_map, list_name)?;
        let block_id = self.new_block_id();
        let index_input = self.expr_input(
            blocks,
            index,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "number",
        )?;
        let item_input = self.expr_input(
            blocks,
            item,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            "string",
        )?;
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": "data_replaceitemoflist",
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {"INDEX": index_input, "ITEM": item_input},
                "fields": {"LIST": [list_name, list_id]},
                "shadow": false,
                "topLevel": false
            }),
        );
        Ok(block_id)
    }

    fn expr_input(
        &mut self,
        blocks: &mut Map<String, Value>,
        expr: &Expr,
        parent_id: &str,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        param_scope: &HashSet<String>,
        default_kind: &str,
    ) -> Result<Value> {
        if let Some(literal) = self.literal_input(expr) {
            return Ok(json!([1, literal]));
        }
        let reporter_id = self.emit_expr_reporter(
            blocks,
            expr,
            parent_id,
            variables_map,
            lists_map,
            param_scope,
        )?;
        if let Some(id) = reporter_id {
            Ok(json!([2, id]))
        } else {
            Ok(json!([1, default_shadow(default_kind)]))
        }
    }

    fn emit_expr_reporter(
        &mut self,
        blocks: &mut Map<String, Value>,
        expr: &Expr,
        parent_id: &str,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        param_scope: &HashSet<String>,
    ) -> Result<Option<String>> {
        match expr {
            Expr::Number { .. } | Expr::String { .. } => Ok(None),
            Expr::BuiltinReporter { kind, .. } => {
                let opcode = match kind.as_str() {
                    "answer" => "sensing_answer",
                    "mouse_x" => "sensing_mousex",
                    "mouse_y" => "sensing_mousey",
                    "timer" => "sensing_timer",
                    _ => bail!("Unsupported built-in reporter '{}'.", kind),
                };
                let block_id = self.new_block_id();
                blocks.insert(
                    block_id.clone(),
                    json!({
                        "opcode": opcode,
                        "next": Value::Null,
                        "parent": parent_id,
                        "inputs": {},
                        "fields": {},
                        "shadow": false,
                        "topLevel": false
                    }),
                );
                Ok(Some(block_id))
            }
            Expr::MathFunc { op, value, .. } => {
                let block_id = self.new_block_id();
                let opcode = if op == "round" {
                    "operator_round"
                } else if is_mathop_reporter(op) {
                    "operator_mathop"
                } else {
                    bail!("Unsupported math reporter '{}'.", op);
                };
                let fields = if opcode == "operator_mathop" {
                    json!({"OPERATOR": [op, Value::Null]})
                } else {
                    json!({})
                };
                blocks.insert(
                    block_id.clone(),
                    json!({
                        "opcode": opcode,
                        "next": Value::Null,
                        "parent": parent_id,
                        "inputs": {},
                        "fields": fields,
                        "shadow": false,
                        "topLevel": false
                    }),
                );
                let num_input = self.expr_input(
                    blocks,
                    value,
                    &block_id,
                    variables_map,
                    lists_map,
                    param_scope,
                    "number",
                )?;
                set_block_input(blocks, &block_id, "NUM", num_input)?;
                Ok(Some(block_id))
            }
            Expr::Var { name, .. } => {
                let lowered = name.to_lowercase();
                if param_scope.contains(&lowered) {
                    let block_id = self.new_block_id();
                    blocks.insert(
                        block_id.clone(),
                        json!({
                            "opcode": "argument_reporter_string_number",
                            "next": Value::Null,
                            "parent": parent_id,
                            "inputs": {},
                            "fields": {"VALUE": [name, Value::Null]},
                            "shadow": false,
                            "topLevel": false
                        }),
                    );
                    return Ok(Some(block_id));
                }
                if let Some(var_id) = variables_map.get(&lowered).cloned() {
                    let block_id = self.new_block_id();
                    blocks.insert(
                        block_id.clone(),
                        json!({
                            "opcode": "data_variable",
                            "next": Value::Null,
                            "parent": parent_id,
                            "inputs": {},
                            "fields": {"VARIABLE": [name, var_id]},
                            "shadow": false,
                            "topLevel": false
                        }),
                    );
                    return Ok(Some(block_id));
                }
                if let Some((remote_target, remote_var)) = split_qualified(name) {
                    let block_id = self.new_block_id();
                    let menu_id = self.new_block_id();
                    blocks.insert(
                        block_id.clone(),
                        json!({
                            "opcode": "sensing_of",
                            "next": Value::Null,
                            "parent": parent_id,
                            "inputs": {"OBJECT": [1, menu_id.clone()]},
                            "fields": {"PROPERTY": [remote_var, Value::Null]},
                            "shadow": false,
                            "topLevel": false
                        }),
                    );
                    blocks.insert(
                        menu_id,
                        json!({
                            "opcode": "sensing_of_object_menu",
                            "next": Value::Null,
                            "parent": block_id.clone(),
                            "inputs": {},
                            "fields": {"OBJECT": [remote_target, Value::Null]},
                            "shadow": true,
                            "topLevel": false
                        }),
                    );
                    return Ok(Some(block_id));
                }
                let var_id = self.lookup_var_id(variables_map, name)?;
                let block_id = self.new_block_id();
                blocks.insert(
                    block_id.clone(),
                    json!({
                        "opcode": "data_variable",
                        "next": Value::Null,
                        "parent": parent_id,
                        "inputs": {},
                        "fields": {"VARIABLE": [name, var_id]},
                        "shadow": false,
                        "topLevel": false
                    }),
                );
                Ok(Some(block_id))
            }
            Expr::PickRandom { start, end, .. } => {
                let block_id = self.new_block_id();
                blocks.insert(
                    block_id.clone(),
                    json!({
                        "opcode": "operator_random",
                        "next": Value::Null,
                        "parent": parent_id,
                        "inputs": {},
                        "fields": {},
                        "shadow": false,
                        "topLevel": false
                    }),
                );
                let from_input = self.expr_input(
                    blocks,
                    start,
                    &block_id,
                    variables_map,
                    lists_map,
                    param_scope,
                    "number",
                )?;
                let to_input = self.expr_input(
                    blocks,
                    end,
                    &block_id,
                    variables_map,
                    lists_map,
                    param_scope,
                    "number",
                )?;
                set_block_input(blocks, &block_id, "FROM", from_input)?;
                set_block_input(blocks, &block_id, "TO", to_input)?;
                Ok(Some(block_id))
            }
            Expr::ListItem {
                list_name, index, ..
            } => {
                let list_id = self.lookup_list_id(lists_map, list_name)?;
                let block_id = self.new_block_id();
                blocks.insert(
                    block_id.clone(),
                    json!({
                        "opcode": "data_itemoflist",
                        "next": Value::Null,
                        "parent": parent_id,
                        "inputs": {},
                        "fields": {"LIST": [list_name, list_id]},
                        "shadow": false,
                        "topLevel": false
                    }),
                );
                let index_input = self.expr_input(
                    blocks,
                    index,
                    &block_id,
                    variables_map,
                    lists_map,
                    param_scope,
                    "number",
                )?;
                set_block_input(blocks, &block_id, "INDEX", index_input)?;
                Ok(Some(block_id))
            }
            Expr::ListLength { list_name, .. } => {
                let list_id = self.lookup_list_id(lists_map, list_name)?;
                let block_id = self.new_block_id();
                blocks.insert(
                    block_id.clone(),
                    json!({
                        "opcode": "data_lengthoflist",
                        "next": Value::Null,
                        "parent": parent_id,
                        "inputs": {},
                        "fields": {"LIST": [list_name, list_id]},
                        "shadow": false,
                        "topLevel": false
                    }),
                );
                Ok(Some(block_id))
            }
            Expr::ListContents { list_name, .. } => {
                let list_id = self.lookup_list_id(lists_map, list_name)?;
                let block_id = self.new_block_id();
                blocks.insert(
                    block_id.clone(),
                    json!({
                        "opcode": "data_listcontents",
                        "next": Value::Null,
                        "parent": parent_id,
                        "inputs": {},
                        "fields": {"LIST": [list_name, list_id]},
                        "shadow": false,
                        "topLevel": false
                    }),
                );
                Ok(Some(block_id))
            }
            Expr::ListContains {
                list_name, item, ..
            } => {
                let list_id = self.lookup_list_id(lists_map, list_name)?;
                let block_id = self.new_block_id();
                blocks.insert(
                    block_id.clone(),
                    json!({
                        "opcode": "data_listcontainsitem",
                        "next": Value::Null,
                        "parent": parent_id,
                        "inputs": {},
                        "fields": {"LIST": [list_name, list_id]},
                        "shadow": false,
                        "topLevel": false
                    }),
                );
                let item_input = self.expr_input(
                    blocks,
                    item,
                    &block_id,
                    variables_map,
                    lists_map,
                    param_scope,
                    "string",
                )?;
                set_block_input(blocks, &block_id, "ITEM", item_input)?;
                Ok(Some(block_id))
            }
            Expr::KeyPressed { key, .. } => {
                let block_id = self.new_block_id();
                let menu_id = self.new_block_id();
                blocks.insert(
                    block_id.clone(),
                    json!({
                        "opcode": "sensing_keypressed",
                        "next": Value::Null,
                        "parent": parent_id,
                        "inputs": {"KEY_OPTION": [1, menu_id.clone()]},
                        "fields": {},
                        "shadow": false,
                        "topLevel": false
                    }),
                );
                let key_value = match self.literal_input(key) {
                    Some(Value::Array(v)) if v.len() >= 2 => {
                        let code = v[0].as_i64().unwrap_or_default();
                        if code == 10 {
                            v[1].as_str().unwrap_or("space").to_string()
                        } else {
                            "space".to_string()
                        }
                    }
                    _ => "space".to_string(),
                };
                blocks.insert(
                    menu_id,
                    json!({
                        "opcode": "sensing_keyoptions",
                        "next": Value::Null,
                        "parent": block_id.clone(),
                        "inputs": {},
                        "fields": {"KEY_OPTION": [key_value, Value::Null]},
                        "shadow": true,
                        "topLevel": false
                    }),
                );
                Ok(Some(block_id))
            }
            Expr::Unary { op, operand, .. } => {
                if op == "-" {
                    let block_id = self.new_block_id();
                    blocks.insert(
                        block_id.clone(),
                        json!({
                            "opcode": "operator_subtract",
                            "next": Value::Null,
                            "parent": parent_id,
                            "inputs": {},
                            "fields": {},
                            "shadow": false,
                            "topLevel": false
                        }),
                    );
                    set_block_input(blocks, &block_id, "NUM1", json!([1, [4, "0"]]))?;
                    let right_input = self.expr_input(
                        blocks,
                        operand,
                        &block_id,
                        variables_map,
                        lists_map,
                        param_scope,
                        "number",
                    )?;
                    set_block_input(blocks, &block_id, "NUM2", right_input)?;
                    return Ok(Some(block_id));
                }
                if op == "not" {
                    let block_id = self.new_block_id();
                    let operand_input = self.expr_input(
                        blocks,
                        operand,
                        &block_id,
                        variables_map,
                        lists_map,
                        param_scope,
                        "boolean",
                    )?;
                    blocks.insert(
                        block_id.clone(),
                        json!({
                            "opcode": "operator_not",
                            "next": Value::Null,
                            "parent": parent_id,
                            "inputs": {"OPERAND": operand_input},
                            "fields": {},
                            "shadow": false,
                            "topLevel": false
                        }),
                    );
                    return Ok(Some(block_id));
                }
                bail!("Unsupported unary operator '{}'.", op)
            }
            Expr::Binary {
                op,
                left,
                right,
                pos,
            } => {
                let id = self.emit_binary_expr(
                    blocks,
                    op,
                    left,
                    right,
                    *pos,
                    parent_id,
                    variables_map,
                    lists_map,
                    param_scope,
                )?;
                Ok(Some(id))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_binary_expr(
        &mut self,
        blocks: &mut Map<String, Value>,
        op: &str,
        left: &Expr,
        right: &Expr,
        pos: Position,
        parent_id: &str,
        variables_map: &HashMap<String, String>,
        lists_map: &HashMap<String, String>,
        param_scope: &HashSet<String>,
    ) -> Result<String> {
        if op == "<=" || op == ">=" {
            let op_first = if op == "<=" { "<" } else { ">" }.to_string();
            let first = Expr::Binary {
                pos,
                op: op_first,
                left: Box::new(left.clone()),
                right: Box::new(right.clone()),
            };
            let second = Expr::Binary {
                pos,
                op: "=".to_string(),
                left: Box::new(left.clone()),
                right: Box::new(right.clone()),
            };
            let rewritten = Expr::Binary {
                pos,
                op: "or".to_string(),
                left: Box::new(first),
                right: Box::new(second),
            };
            if let Some(id) = self.emit_expr_reporter(
                blocks,
                &rewritten,
                parent_id,
                variables_map,
                lists_map,
                param_scope,
            )? {
                return Ok(id);
            }
            bail!("Failed to emit rewritten '{}' expression.", op);
        }

        if op == "!=" {
            let eq_expr = Expr::Binary {
                pos,
                op: "=".to_string(),
                left: Box::new(left.clone()),
                right: Box::new(right.clone()),
            };
            let not_expr = Expr::Unary {
                pos,
                op: "not".to_string(),
                operand: Box::new(eq_expr),
            };
            if let Some(id) = self.emit_expr_reporter(
                blocks,
                &not_expr,
                parent_id,
                variables_map,
                lists_map,
                param_scope,
            )? {
                return Ok(id);
            }
            bail!("Failed to emit inequality expression.");
        }

        let opcode = match op {
            "+" => "operator_add",
            "-" => "operator_subtract",
            "*" => "operator_multiply",
            "/" => "operator_divide",
            "%" => "operator_mod",
            "<" => "operator_lt",
            ">" => "operator_gt",
            "=" | "==" => "operator_equals",
            "and" => "operator_and",
            "or" => "operator_or",
            _ => bail!("Unsupported binary operator '{}'.", op),
        };
        let (left_key, right_key, kind) = match opcode {
            "operator_add" | "operator_subtract" | "operator_multiply" | "operator_divide"
            | "operator_mod" => ("NUM1", "NUM2", "number"),
            "operator_lt" | "operator_gt" => ("OPERAND1", "OPERAND2", "number"),
            "operator_equals" => ("OPERAND1", "OPERAND2", "string"),
            "operator_and" | "operator_or" => ("OPERAND1", "OPERAND2", "boolean"),
            _ => bail!("Unsupported operator opcode '{}'.", opcode),
        };

        let block_id = self.new_block_id();
        blocks.insert(
            block_id.clone(),
            json!({
                "opcode": opcode,
                "next": Value::Null,
                "parent": parent_id,
                "inputs": {},
                "fields": {},
                "shadow": false,
                "topLevel": false
            }),
        );
        let left_input = self.expr_input(
            blocks,
            left,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            kind,
        )?;
        let right_input = self.expr_input(
            blocks,
            right,
            &block_id,
            variables_map,
            lists_map,
            param_scope,
            kind,
        )?;
        set_block_input(blocks, &block_id, left_key, left_input)?;
        set_block_input(blocks, &block_id, right_key, right_input)?;
        Ok(block_id)
    }

    fn literal_input(&self, expr: &Expr) -> Option<Value> {
        match expr {
            Expr::Number { value, .. } => Some(json!([4, format_num(*value)])),
            Expr::String { value, .. } => Some(json!([10, value])),
            _ => None,
        }
    }

    fn menu_text_from_expr(&self, expr: &Expr, fallback: &str) -> String {
        match expr {
            Expr::String { value, .. } => value.clone(),
            Expr::Number { value, .. } => format_num(*value),
            Expr::Var { name, .. } => name.clone(),
            _ => fallback.to_string(),
        }
    }

    fn lookup_var_id(
        &self,
        variables_map: &HashMap<String, String>,
        var_name: &str,
    ) -> Result<String> {
        variables_map
            .get(&var_name.to_lowercase())
            .cloned()
            .ok_or_else(|| anyhow!("Variable '{}' is not declared.", var_name))
    }

    fn lookup_list_id(
        &self,
        lists_map: &HashMap<String, String>,
        list_name: &str,
    ) -> Result<String> {
        lists_map
            .get(&list_name.to_lowercase())
            .cloned()
            .ok_or_else(|| anyhow!("List '{}' is not declared.", list_name))
    }

    fn build_costumes(&mut self, target: &Target) -> Result<Vec<Value>> {
        let mut costumes = target.costumes.clone();
        if costumes.is_empty() {
            let default_path = if target.is_stage {
                "__default_stage_backdrop__.svg"
            } else {
                "__default_sprite_costume__.svg"
            };
            costumes.push(crate::ast::CostumeDecl {
                pos: target.pos,
                path: default_path.to_string(),
            });
        }

        let mut out = Vec::new();
        let mut used_names: HashSet<String> = HashSet::new();
        for (idx, costume) in costumes.iter().enumerate() {
            let mut rotation_center_x = 0.0;
            let mut rotation_center_y = 0.0;
            let (mut data, ext, base_name) = if costume.path == "__default_stage_backdrop__.svg" {
                (
                    DEFAULT_STAGE_SVG.as_bytes().to_vec(),
                    "svg".to_string(),
                    format!("backdrop{}", idx + 1),
                )
            } else if costume.path == "__default_sprite_costume__.svg" {
                (
                    DEFAULT_SPRITE_SVG.as_bytes().to_vec(),
                    "svg".to_string(),
                    format!("costume{}", idx + 1),
                )
            } else {
                let mut file_path = Path::new(&costume.path).to_path_buf();
                if !file_path.is_absolute() {
                    let mut candidates = Vec::new();
                    candidates.push(self.source_dir.join(&file_path));
                    if let Some(parent) = self.source_dir.parent() {
                        candidates.push(parent.join(&file_path));
                    }
                    if let Ok(cwd) = std::env::current_dir() {
                        candidates.push(cwd.join(&file_path));
                    }
                    if let Some(found) = candidates.iter().find(|p| p.exists()) {
                        file_path = found.clone();
                    } else if let Some(first) = candidates.first() {
                        file_path = first.clone();
                    }
                }
                if !file_path.exists() || !file_path.is_file() {
                    bail!(
                        "Costume file not found for target '{}': '{}' resolved to '{}'.",
                        target.name,
                        costume.path,
                        file_path.display()
                    );
                }
                let ext = file_path
                    .extension()
                    .and_then(|x| x.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if ext != "svg" && ext != "png" {
                    bail!(
                        "Unsupported costume format '.{}' for '{}'. Only .svg and .png are supported.",
                        ext,
                        file_path.display()
                    );
                }
                let data = fs::read(&file_path)?;
                let name = file_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("costume")
                    .to_string();
                (data, ext, name)
            };
            let name = uniquify_costume_name(&base_name, &mut used_names);

            if ext == "svg" {
                match self.prepare_svg(&data, &costume.path) {
                    Ok((prepared, cx, cy)) => {
                        data = prepared;
                        rotation_center_x = cx;
                        rotation_center_y = cy;
                    }
                    Err(err) if is_nonpositive_viewbox_error(&err) => {
                        eprintln!(
                            "Skipping SVG costume '{}' for target '{}' due to non-positive viewBox dimensions.",
                            costume.path, target.name
                        );
                        continue;
                    }
                    Err(err) => return Err(err),
                }
            }

            let digest = format!("{:x}", md5::compute(&data));
            let md5ext = format!("{}.{}", digest, ext);
            self.assets.insert(md5ext.clone(), data);
            let mut entry = json!({
                "name": name,
                "assetId": digest,
                "md5ext": md5ext,
                "dataFormat": ext,
                "rotationCenterX": rotation_center_x,
                "rotationCenterY": rotation_center_y
            });
            if ext == "png" {
                set_value_key(&mut entry, "bitmapResolution", json!(1))?;
            }
            out.push(entry);
        }
        if out.is_empty() {
            let fallback_svg = if target.is_stage {
                DEFAULT_STAGE_SVG.as_bytes()
            } else {
                DEFAULT_SPRITE_SVG.as_bytes()
            };
            let (prepared, cx, cy) = self.prepare_svg(fallback_svg, "__fallback_default__.svg")?;
            let digest = format!("{:x}", md5::compute(&prepared));
            let md5ext = format!("{}.svg", digest);
            let fallback_name = uniquify_costume_name(
                if target.is_stage {
                    "backdrop1"
                } else {
                    "costume1"
                },
                &mut used_names,
            );
            self.assets.insert(md5ext.clone(), prepared);
            out.push(json!({
                "name": fallback_name,
                "assetId": digest,
                "md5ext": md5ext,
                "dataFormat": "svg",
                "rotationCenterX": cx,
                "rotationCenterY": cy
            }));
        }
        Ok(out)
    }

    fn prepare_svg(&self, data: &[u8], source_name: &str) -> Result<(Vec<u8>, f64, f64)> {
        let mut root = Element::parse(Cursor::new(data))
            .map_err(|e| anyhow!("Invalid SVG file '{}': {}.", source_name, e))?;
        let (min_x, min_y, width, height) = self.read_svg_bounds(&root, source_name)?;
        if self.options.scale_svgs {
            self.normalize_svg_root(
                &mut root,
                min_x,
                min_y,
                width,
                height,
                DEFAULT_SVG_TARGET_SIZE,
            )?;
            let centered = DEFAULT_SVG_TARGET_SIZE / 2.0;
            let mut out = Vec::new();
            root.write(&mut out)?;
            return Ok((out, centered, centered));
        }
        let mut out = Vec::new();
        root.write(&mut out)?;
        Ok((out, width / 2.0, height / 2.0))
    }

    fn normalize_svg_root(
        &self,
        root: &mut Element,
        min_x: f64,
        min_y: f64,
        width: f64,
        height: f64,
        target_size: f64,
    ) -> Result<()> {
        if width <= 0.0 || height <= 0.0 {
            bail!("SVG width/height must be positive before normalization.");
        }
        let scale_x = target_size / width;
        let scale_y = target_size / height;
        let transform = format!(
            "translate({} {}) scale({} {})",
            format_num(-min_x),
            format_num(-min_y),
            format_num(scale_x),
            format_num(scale_y)
        );

        let mut wrapper = Element::new("g");
        wrapper.prefix = root.prefix.clone();
        wrapper.namespace = root.namespace.clone();
        wrapper
            .attributes
            .insert("transform".to_string(), transform);
        wrapper.children = std::mem::take(&mut root.children);

        root.attributes.insert(
            "viewBox".to_string(),
            format!(
                "0 0 {} {}",
                format_num(target_size),
                format_num(target_size)
            ),
        );
        root.attributes
            .insert("width".to_string(), format_num(target_size));
        root.attributes
            .insert("height".to_string(), format_num(target_size));
        root.children.push(XMLNode::Element(wrapper));
        Ok(())
    }

    fn read_svg_bounds(&self, root: &Element, source_name: &str) -> Result<(f64, f64, f64, f64)> {
        if let Some(view_box) = root.attributes.get("viewBox") {
            if let Some(parsed) = self.parse_view_box(view_box, source_name)? {
                return Ok(parsed);
            }
        }

        let width = self.parse_svg_length(root.attributes.get("width").map(|s| s.as_str()));
        let height = self.parse_svg_length(root.attributes.get("height").map(|s| s.as_str()));
        if let (Some(w), Some(h)) = (width, height) {
            if w > 0.0 && h > 0.0 {
                return Ok((0.0, 0.0, w, h));
            }
        }
        Ok((0.0, 0.0, DEFAULT_SVG_TARGET_SIZE, DEFAULT_SVG_TARGET_SIZE))
    }

    fn parse_view_box(
        &self,
        view_box: &str,
        source_name: &str,
    ) -> Result<Option<(f64, f64, f64, f64)>> {
        let parts = view_box
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        if parts.len() != 4 {
            return Ok(None);
        }
        let min_x = parts[0]
            .parse::<f64>()
            .map_err(|_| anyhow!("Invalid SVG viewBox in '{}': '{}'.", source_name, view_box))?;
        let min_y = parts[1]
            .parse::<f64>()
            .map_err(|_| anyhow!("Invalid SVG viewBox in '{}': '{}'.", source_name, view_box))?;
        let width = parts[2]
            .parse::<f64>()
            .map_err(|_| anyhow!("Invalid SVG viewBox in '{}': '{}'.", source_name, view_box))?;
        let height = parts[3]
            .parse::<f64>()
            .map_err(|_| anyhow!("Invalid SVG viewBox in '{}': '{}'.", source_name, view_box))?;
        if width <= 0.0 || height <= 0.0 {
            bail!(
                "SVG viewBox must have positive width/height in '{}'.",
                source_name
            );
        }
        Ok(Some((min_x, min_y, width, height)))
    }

    fn parse_svg_length(&self, value: Option<&str>) -> Option<f64> {
        let s = value?.trim_start();
        if s.is_empty() {
            return None;
        }
        let mut chars = s.char_indices().peekable();
        let mut end = 0usize;
        if let Some((_, sign)) = chars.peek().copied() {
            if sign == '+' || sign == '-' {
                end = 1;
                chars.next();
            }
        }
        let mut saw_digit = false;
        while let Some((idx, ch)) = chars.peek().copied() {
            if ch.is_ascii_digit() {
                saw_digit = true;
                end = idx + ch.len_utf8();
                chars.next();
            } else {
                break;
            }
        }
        if let Some((idx, '.')) = chars.peek().copied() {
            end = idx + 1;
            chars.next();
            while let Some((idx2, ch2)) = chars.peek().copied() {
                if ch2.is_ascii_digit() {
                    saw_digit = true;
                    end = idx2 + ch2.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
        }
        if !saw_digit || end == 0 || end > s.len() {
            return None;
        }
        let n = s[..end].parse::<f64>().ok()?;
        if n > 0.0 {
            Some(n)
        } else {
            None
        }
    }
}

fn collect_messages_from_statements(statements: &[Statement], out: &mut HashSet<String>) {
    for stmt in statements {
        match stmt {
            Statement::Broadcast { message, .. } => {
                out.insert(message.clone());
            }
            Statement::BroadcastAndWait { message, .. } => {
                out.insert(message.clone());
            }
            Statement::Repeat { body, .. }
            | Statement::ForEach { body, .. }
            | Statement::While { body, .. }
            | Statement::RepeatUntil { body, .. }
            | Statement::Forever { body, .. } => {
                collect_messages_from_statements(body, out);
            }
            Statement::If {
                then_body,
                else_body,
                ..
            } => {
                collect_messages_from_statements(then_body, out);
                collect_messages_from_statements(else_body, out);
            }
            _ => {}
        }
    }
}

fn target_uses_pen_extension(target: &Target) -> bool {
    target
        .scripts
        .iter()
        .any(|script| statements_use_pen_extension(&script.body))
        || target
            .procedures
            .iter()
            .any(|procedure| statements_use_pen_extension(&procedure.body))
}

fn statements_use_pen_extension(statements: &[Statement]) -> bool {
    for stmt in statements {
        match stmt {
            Statement::PenDown { .. }
            | Statement::PenUp { .. }
            | Statement::PenClear { .. }
            | Statement::PenStamp { .. }
            | Statement::ChangePenSizeBy { .. }
            | Statement::SetPenSizeTo { .. }
            | Statement::ChangePenColorParamBy { .. }
            | Statement::SetPenColorParamTo { .. } => return true,
            Statement::Repeat { body, .. }
            | Statement::ForEach { body, .. }
            | Statement::While { body, .. }
            | Statement::RepeatUntil { body, .. }
            | Statement::Forever { body, .. } => {
                if statements_use_pen_extension(body) {
                    return true;
                }
            }
            Statement::If {
                then_body,
                else_body,
                ..
            } => {
                if statements_use_pen_extension(then_body)
                    || statements_use_pen_extension(else_body)
                {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn merge_object(dst: &mut Value, add: Value) -> Result<()> {
    let dst_obj = dst
        .as_object_mut()
        .ok_or_else(|| anyhow!("Expected object in merge_object dst"))?;
    let add_obj = add
        .as_object()
        .ok_or_else(|| anyhow!("Expected object in merge_object add"))?;
    for (k, v) in add_obj {
        dst_obj.insert(k.clone(), v.clone());
    }
    Ok(())
}

fn format_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{:.6}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn is_mathop_reporter(op: &str) -> bool {
    matches!(
        op,
        "abs"
            | "floor"
            | "ceiling"
            | "sqrt"
            | "sin"
            | "cos"
            | "tan"
            | "asin"
            | "acos"
            | "atan"
            | "ln"
            | "log"
    )
}

fn is_ignored_noop_call(name: &str) -> bool {
    name.eq_ignore_ascii_case("log")
}

fn default_shadow(kind: &str) -> Value {
    if kind == "number" {
        json!([4, "0"])
    } else {
        json!([10, ""])
    }
}

fn split_qualified(name: &str) -> Option<(&str, &str)> {
    let (left, right) = name.split_once('.')?;
    if left.is_empty() || right.is_empty() {
        return None;
    }
    if right.contains('.') {
        return None;
    }
    Some((left, right))
}

fn set_block_next(blocks: &mut Map<String, Value>, block_id: &str, next: Value) -> Result<()> {
    let block = blocks
        .get_mut(block_id)
        .ok_or_else(|| anyhow!("Missing block '{}'.", block_id))?;
    let obj = block
        .as_object_mut()
        .ok_or_else(|| anyhow!("Block '{}' is not an object.", block_id))?;
    obj.insert("next".to_string(), next);
    Ok(())
}

fn set_block_input(
    blocks: &mut Map<String, Value>,
    block_id: &str,
    key: &str,
    value: Value,
) -> Result<()> {
    let block = blocks
        .get_mut(block_id)
        .ok_or_else(|| anyhow!("Missing block '{}'.", block_id))?;
    let obj = block
        .as_object_mut()
        .ok_or_else(|| anyhow!("Block '{}' is not an object.", block_id))?;
    let inputs = obj
        .entry("inputs")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| anyhow!("Block '{}' has invalid inputs shape.", block_id))?;
    inputs.insert(key.to_string(), value);
    Ok(())
}

fn set_value_key(value: &mut Value, key: &str, entry: Value) -> Result<()> {
    let obj = value
        .as_object_mut()
        .ok_or_else(|| anyhow!("Expected object while setting key '{}'.", key))?;
    obj.insert(key.to_string(), entry);
    Ok(())
}

fn is_nonpositive_viewbox_error(err: &anyhow::Error) -> bool {
    err.to_string()
        .contains("SVG viewBox must have positive width/height")
}

fn uniquify_costume_name(base: &str, used: &mut HashSet<String>) -> String {
    let trimmed = base.trim();
    let base_name = if trimmed.is_empty() {
        "costume"
    } else {
        trimmed
    };
    let mut candidate = base_name.to_string();
    let mut suffix = 2usize;
    while !used.insert(candidate.to_lowercase()) {
        candidate = format!("{} {}", base_name, suffix);
        suffix += 1;
    }
    candidate
}
