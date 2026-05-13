pub mod config;
pub mod context;
pub mod flatten;
pub mod ids;
pub mod inspect;
pub mod junk;
pub mod layout;
pub mod names;
pub mod pass;
pub mod procedures;
pub mod protect;
pub mod rename;
pub mod wrap;

use self::config::{ObfuscationConfig, ObfuscationPreset};
use self::context::ObfuscationContext;
use self::flatten::FlattenControlFlowPass;
use self::ids::RandomizeBlockIdsPass;
use self::inspect::InspectReport;
use self::junk::InjectJunkPass;
use self::layout::ScrambleLayoutPass;
use self::names::clicker_like_name;
use self::pass::ObfuscationPass;
use self::protect::ProtectVariablesPass;
use self::rename::RenamePass;
use self::wrap::WrapProceduresPass;
use crate::sb3::{read_sb3_file, write_sb3_file};
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ObfuscationRunResult {
    pub seed: u64,
    pub applied_passes: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn obfuscate_sb3_file(
    input: &Path,
    output: &Path,
    config: ObfuscationConfig,
) -> Result<ObfuscationRunResult> {
    let mut archive = read_sb3_file(input)?;
    let result = obfuscate_project(&mut archive.project, &config)?;
    write_sb3_file(output, &archive)?;
    Ok(result)
}

pub fn obfuscate_project(
    project: &mut Value,
    config: &ObfuscationConfig,
) -> Result<ObfuscationRunResult> {
    let effective = effective_config(project, config)?;
    let seed = effective.seed.unwrap_or_else(rand::random::<u64>);
    let used_names = collect_existing_names(project);
    let used_ids = collect_existing_ids(project);
    let protected_variable_names = effective
        .protect_vars
        .iter()
        .map(|name| name.trim().to_ascii_lowercase())
        .filter(|name| !name.is_empty())
        .collect::<HashSet<_>>();
    let mut ctx = ObfuscationContext::new(
        seed,
        effective.level,
        used_names,
        used_ids,
        protected_variable_names,
    );

    let mut passes: Vec<Box<dyn ObfuscationPass>> = Vec::new();
    if effective.rename {
        passes.push(Box::new(RenamePass));
    }
    if !effective.protect_vars.is_empty() {
        passes.push(Box::new(ProtectVariablesPass));
    }
    if effective.wrap_procedures {
        passes.push(Box::new(WrapProceduresPass));
    }
    if effective.flatten_control_flow {
        passes.push(Box::new(FlattenControlFlowPass));
    }
    if effective.randomize_ids {
        passes.push(Box::new(RandomizeBlockIdsPass));
    }
    if effective.scramble_layout {
        passes.push(Box::new(ScrambleLayoutPass));
    }
    if effective.inject_junk {
        passes.push(Box::new(InjectJunkPass));
    }

    for pass in passes {
        pass.run(project, &mut ctx)?;
    }

    Ok(ObfuscationRunResult {
        seed,
        applied_passes: ctx.applied_passes,
        warnings: ctx.warnings,
    })
}

pub fn inspect_sb3_file(path: &Path) -> Result<InspectReport> {
    let archive = read_sb3_file(path)?;
    inspect::inspect_project(&archive.project)
}

pub fn parse_protect_list(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in raw.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                let item = current.trim();
                if !item.is_empty() {
                    out.push(item.to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let item = current.trim();
    if !item.is_empty() {
        out.push(item.to_string());
    }
    out
}

fn effective_config(project: &Value, config: &ObfuscationConfig) -> Result<ObfuscationConfig> {
    let mut resolved = config.clone();
    let has_explicit_pass_selection = resolved.rename
        || resolved.wrap_procedures
        || resolved.flatten_control_flow
        || resolved.randomize_ids
        || resolved.scramble_layout
        || resolved.inject_junk
        || !resolved.protect_vars.is_empty();

    if !has_explicit_pass_selection {
        let (rename, wrap, flatten, ids, layout, junk) = resolved.level.defaults();
        resolved.rename = rename;
        resolved.wrap_procedures = wrap;
        resolved.flatten_control_flow = flatten;
        resolved.randomize_ids = ids;
        resolved.scramble_layout = layout;
        resolved.inject_junk = junk;
    }

    if matches!(resolved.preset, Some(ObfuscationPreset::Clicker)) {
        resolved.rename = true;
        resolved.randomize_ids = true;
        resolved.scramble_layout = true;
        resolved.inject_junk = true;
        for name in clicker_candidates(project)? {
            if !resolved
                .protect_vars
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&name))
            {
                resolved.protect_vars.push(name);
            }
        }
    }

    Ok(resolved)
}

fn clicker_candidates(project: &Value) -> Result<Vec<String>> {
    let targets = project
        .get("targets")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("Invalid project.json: missing 'targets' array."))?;
    let mut names = Vec::new();
    for target in targets {
        let Some(variables) = target.get("variables").and_then(Value::as_object) else {
            continue;
        };
        for entry in variables.values() {
            let Some(name) = entry
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(Value::as_str)
            else {
                continue;
            };
            if clicker_like_name(name)
                && !names
                    .iter()
                    .any(|existing: &String| existing.eq_ignore_ascii_case(name))
            {
                names.push(name.to_string());
            }
        }
    }
    Ok(names)
}

fn collect_existing_names(project: &Value) -> HashSet<String> {
    let mut out = HashSet::new();
    if let Some(targets) = project.get("targets").and_then(Value::as_array) {
        for target in targets {
            if let Some(name) = target.get("name").and_then(Value::as_str) {
                out.insert(name.to_ascii_lowercase());
            }
            collect_named_entries(target.get("variables"), &mut out);
            collect_named_entries(target.get("lists"), &mut out);
            if let Some(entries) = target.get("broadcasts").and_then(Value::as_object) {
                for value in entries.values() {
                    if let Some(name) = value.as_str() {
                        out.insert(name.to_ascii_lowercase());
                    }
                }
            }
            if let Some(blocks) = target.get("blocks").and_then(Value::as_object) {
                for block in blocks.values() {
                    if let Some(proccode) = block
                        .get("mutation")
                        .and_then(Value::as_object)
                        .and_then(|mutation| mutation.get("proccode"))
                        .and_then(Value::as_str)
                    {
                        out.insert(proccode.to_ascii_lowercase());
                    }
                }
            }
        }
    }
    out
}

fn collect_named_entries(node: Option<&Value>, out: &mut HashSet<String>) {
    let Some(entries) = node.and_then(Value::as_object) else {
        return;
    };
    for value in entries.values() {
        if let Some(name) = value
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(Value::as_str)
        {
            out.insert(name.to_ascii_lowercase());
        }
    }
}

fn collect_existing_ids(project: &Value) -> HashSet<String> {
    let mut out = HashSet::new();
    if let Some(targets) = project.get("targets").and_then(Value::as_array) {
        for target in targets {
            collect_object_keys(target.get("variables"), &mut out);
            collect_object_keys(target.get("lists"), &mut out);
            collect_object_keys(target.get("broadcasts"), &mut out);
            collect_object_keys(target.get("blocks"), &mut out);
            collect_object_keys(target.get("comments"), &mut out);
        }
    }
    out
}

fn collect_object_keys(node: Option<&Value>, out: &mut HashSet<String>) {
    let Some(obj) = node.and_then(Value::as_object) else {
        return;
    };
    for key in obj.keys() {
        out.insert(key.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::obfuscator::config::{ObfuscationLevel, ObfuscationPreset};
    use crate::obfuscator::ids::rewrite_block_references;
    use crate::obfuscator::inspect::render_inspect_report;
    use crate::sb3::{build_sb3_bytes, read_sb3_bytes, Sb3Archive};
    use serde_json::json;
    use std::collections::BTreeMap;

    fn sample_project() -> Value {
        json!({
            "targets": [
                {
                    "isStage": true,
                    "name": "Stage",
                    "variables": {
                        "var1": ["coins", 0],
                        "var2": ["coins per click", 1]
                    },
                    "lists": {
                        "list1": ["upgrades", []]
                    },
                    "broadcasts": {
                        "bc1": "buy upgrade"
                    },
                    "blocks": {
                        "block1": {
                            "opcode": "event_whenflagclicked",
                            "next": "block2",
                            "parent": null,
                            "inputs": {
                                "VALUE": [3, "block3", [10, "fallback"]]
                            },
                            "fields": {},
                            "shadow": false,
                            "topLevel": true,
                            "x": 100,
                            "y": 100
                        },
                        "block2": {
                            "opcode": "data_changevariableby",
                            "next": null,
                            "parent": "block1",
                            "inputs": {
                                "VALUE": [1, [4, "1"]]
                            },
                            "fields": {
                                "VARIABLE": ["coins", "var1"],
                                "LIST": ["upgrades", "list1"],
                                "BROADCAST_OPTION": ["buy upgrade", "bc1"]
                            },
                            "shadow": false,
                            "topLevel": false
                        },
                        "block3": {
                            "opcode": "procedures_call",
                            "next": null,
                            "parent": null,
                            "inputs": {},
                            "fields": {},
                            "mutation": {
                                "proccode": "add coins %s",
                                "argumentids": "[\"arg1\"]"
                            },
                            "shadow": false,
                            "topLevel": false
                        },
                        "block4": {
                            "opcode": "procedures_prototype",
                            "next": null,
                            "parent": "block5",
                            "inputs": {},
                            "fields": {},
                            "mutation": {
                                "proccode": "add coins %s",
                                "argumentnames": "[\"amount\"]",
                                "argumentids": "[\"arg1\"]"
                            },
                            "shadow": false,
                            "topLevel": false
                        },
                        "block5": {
                            "opcode": "procedures_definition",
                            "next": "block6",
                            "parent": null,
                            "inputs": {
                                "custom_block": [1, "block4"]
                            },
                            "fields": {},
                            "shadow": false,
                            "topLevel": true,
                            "x": 160,
                            "y": 180
                        },
                        "block6": {
                            "opcode": "data_changevariableby",
                            "next": null,
                            "parent": "block5",
                            "inputs": {
                                "VALUE": [1, [4, "2"]]
                            },
                            "fields": {
                                "VARIABLE": ["coins", "var1"]
                            },
                            "shadow": false,
                            "topLevel": false
                        }
                    },
                    "comments": {
                        "comment1": {
                            "blockId": "block2"
                        }
                    }
                }
            ],
            "monitors": [
                {
                    "id": "var1",
                    "opcode": "data_variable",
                    "params": {
                        "VARIABLE": "coins"
                    }
                }
            ],
            "extensions": [],
            "meta": {
                "semver": "3.0.0",
                "vm": "0.2.0",
                "agent": "sbtext-rs"
            }
        })
    }

    fn flatten_fixture_project() -> Value {
        json!({
            "targets": [
                {
                    "isStage": true,
                    "name": "Stage",
                    "variables": {
                        "var1": ["coins", 0]
                    },
                    "lists": {},
                    "broadcasts": {},
                    "blocks": {
                        "hat1": {
                            "opcode": "event_whenflagclicked",
                            "next": "stmt1",
                            "parent": null,
                            "inputs": {},
                            "fields": {},
                            "shadow": false,
                            "topLevel": true,
                            "x": 20,
                            "y": 20
                        },
                        "stmt1": {
                            "opcode": "data_changevariableby",
                            "next": "stmt2",
                            "parent": "hat1",
                            "inputs": {
                                "VALUE": [1, [4, "1"]]
                            },
                            "fields": {
                                "VARIABLE": ["coins", "var1"]
                            },
                            "shadow": false,
                            "topLevel": false
                        },
                        "stmt2": {
                            "opcode": "data_changevariableby",
                            "next": null,
                            "parent": "stmt1",
                            "inputs": {
                                "VALUE": [1, [4, "2"]]
                            },
                            "fields": {
                                "VARIABLE": ["coins", "var1"]
                            },
                            "shadow": false,
                            "topLevel": false
                        }
                    },
                    "comments": {}
                }
            ],
            "monitors": [],
            "extensions": [],
            "meta": {
                "semver": "3.0.0",
                "vm": "0.2.0",
                "agent": "sbtext-rs"
            }
        })
    }

    #[test]
    fn reads_and_writes_sb3_archives_preserving_assets() {
        let archive = Sb3Archive::new(
            sample_project(),
            BTreeMap::from([
                ("a.txt".to_string(), b"alpha".to_vec()),
                ("nested/b.txt".to_string(), b"beta".to_vec()),
            ]),
        );
        let bytes = build_sb3_bytes(&archive).expect("build bytes");
        let roundtrip = read_sb3_bytes(&bytes).expect("read bytes");
        assert_eq!(roundtrip.assets.get("a.txt"), Some(&b"alpha".to_vec()));
        assert_eq!(
            roundtrip.assets.get("nested/b.txt"),
            Some(&b"beta".to_vec())
        );
        assert_eq!(roundtrip.project["targets"][0]["name"], "Stage");
    }

    #[test]
    fn rename_updates_variables_lists_broadcasts_and_procedures() {
        let mut project = sample_project();
        let result = obfuscate_project(
            &mut project,
            &ObfuscationConfig {
                level: ObfuscationLevel::Low,
                rename: true,
                wrap_procedures: false,
                flatten_control_flow: false,
                randomize_ids: false,
                scramble_layout: false,
                inject_junk: false,
                protect_vars: Vec::new(),
                preset: None,
                seed: Some(7),
            },
        )
        .expect("obfuscate");
        assert_eq!(result.applied_passes.len(), 1);
        assert_ne!(project["targets"][0]["variables"]["var1"][0], "coins");
        assert_ne!(project["targets"][0]["lists"]["list1"][0], "upgrades");
        assert_ne!(project["targets"][0]["broadcasts"]["bc1"], "buy upgrade");
        assert_eq!(
            project["targets"][0]["blocks"]["block2"]["fields"]["VARIABLE"][1],
            "var1"
        );
        assert_ne!(
            project["targets"][0]["blocks"]["block2"]["fields"]["VARIABLE"][0],
            "coins"
        );
        assert_ne!(
            project["targets"][0]["blocks"]["block3"]["mutation"]["proccode"],
            "add coins %s"
        );
    }

    #[test]
    fn protect_mode_keeps_bait_name_and_metadata() {
        let mut project = sample_project();
        obfuscate_project(
            &mut project,
            &ObfuscationConfig {
                level: ObfuscationLevel::Low,
                rename: false,
                wrap_procedures: false,
                flatten_control_flow: false,
                randomize_ids: false,
                scramble_layout: false,
                inject_junk: false,
                protect_vars: vec!["coins".to_string()],
                preset: None,
                seed: Some(11),
            },
        )
        .expect("protect");
        let vars = project["targets"][0]["variables"]
            .as_object()
            .expect("variables object");
        assert!(vars.values().any(|entry| entry[0] == "coins"));
        assert!(project["meta"]["sbtextObfuscation"]["protectedVariables"].is_array());
    }

    #[test]
    fn randomize_block_ids_updates_references_and_comments() {
        let mut project = sample_project();
        obfuscate_project(
            &mut project,
            &ObfuscationConfig {
                level: ObfuscationLevel::Low,
                rename: false,
                wrap_procedures: false,
                flatten_control_flow: false,
                randomize_ids: true,
                scramble_layout: false,
                inject_junk: false,
                protect_vars: Vec::new(),
                preset: None,
                seed: Some(5),
            },
        )
        .expect("ids");
        let blocks = project["targets"][0]["blocks"]
            .as_object()
            .expect("blocks object");
        assert!(!blocks.contains_key("block1"));
        let any_child_parent_matches = blocks.values().any(|block| {
            block
                .get("parent")
                .and_then(Value::as_str)
                .map(|parent| blocks.contains_key(parent))
                == Some(true)
        });
        assert!(any_child_parent_matches);
        let comment_block = project["targets"][0]["comments"]["comment1"]["blockId"]
            .as_str()
            .expect("comment block id");
        assert!(blocks.contains_key(comment_block));
    }

    #[test]
    fn rewrite_block_references_handles_nested_inputs() {
        let mut value = json!([3, "old_block_id", [10, "fallback"], {"nested": "old_block_id"}]);
        rewrite_block_references(
            &mut value,
            &std::collections::HashMap::from([(
                "old_block_id".to_string(),
                "new_block_id".to_string(),
            )]),
        );
        assert_eq!(value[1], "new_block_id");
        assert_eq!(value[3]["nested"], "new_block_id");
    }

    #[test]
    fn layout_scramble_only_changes_top_level_blocks() {
        let mut project = sample_project();
        obfuscate_project(
            &mut project,
            &ObfuscationConfig {
                level: ObfuscationLevel::Medium,
                rename: false,
                wrap_procedures: false,
                flatten_control_flow: false,
                randomize_ids: false,
                scramble_layout: true,
                inject_junk: false,
                protect_vars: Vec::new(),
                preset: None,
                seed: Some(99),
            },
        )
        .expect("layout");
        assert_ne!(project["targets"][0]["blocks"]["block1"]["x"], 100);
        assert_eq!(project["targets"][0]["blocks"]["block2"].get("x"), None);
    }

    #[test]
    fn fixed_seed_produces_deterministic_output() {
        let mut left = sample_project();
        let mut right = sample_project();
        let config = ObfuscationConfig {
            level: ObfuscationLevel::Medium,
            rename: true,
            wrap_procedures: false,
            flatten_control_flow: false,
            randomize_ids: true,
            scramble_layout: true,
            inject_junk: true,
            protect_vars: vec!["coins".to_string()],
            preset: None,
            seed: Some(12345),
        };
        obfuscate_project(&mut left, &config).expect("left");
        obfuscate_project(&mut right, &config).expect("right");
        assert_eq!(left, right);
    }

    #[test]
    fn clicker_preset_detects_relevant_variables() {
        let mut project = sample_project();
        let result = obfuscate_project(
            &mut project,
            &ObfuscationConfig {
                level: ObfuscationLevel::Low,
                rename: false,
                wrap_procedures: false,
                flatten_control_flow: false,
                randomize_ids: false,
                scramble_layout: false,
                inject_junk: false,
                protect_vars: Vec::new(),
                preset: Some(ObfuscationPreset::Clicker),
                seed: Some(21),
            },
        )
        .expect("clicker");
        assert!(result
            .applied_passes
            .iter()
            .any(|name| name.contains("protect")));
    }

    #[test]
    fn procedure_wrapping_reroutes_calls_through_generated_wrapper() {
        let mut project = sample_project();
        obfuscate_project(
            &mut project,
            &ObfuscationConfig {
                level: ObfuscationLevel::High,
                rename: false,
                wrap_procedures: true,
                flatten_control_flow: false,
                randomize_ids: false,
                scramble_layout: false,
                inject_junk: false,
                protect_vars: Vec::new(),
                preset: None,
                seed: Some(41),
            },
        )
        .expect("wrap");

        let blocks = project["targets"][0]["blocks"]
            .as_object()
            .expect("blocks object");
        let procedure_definition_count = blocks
            .values()
            .filter(|block| {
                block.get("opcode").and_then(Value::as_str) == Some("procedures_definition")
            })
            .count();
        assert!(procedure_definition_count >= 2);
        assert_ne!(
            blocks["block3"]["mutation"]["proccode"],
            Value::String("add coins %s".to_string())
        );
        assert_ne!(
            blocks["block4"]["mutation"]["proccode"],
            blocks["block3"]["mutation"]["proccode"]
        );
    }

    #[test]
    fn control_flow_flattening_replaces_direct_chain_with_helper_calls() {
        let mut project = flatten_fixture_project();
        obfuscate_project(
            &mut project,
            &ObfuscationConfig {
                level: ObfuscationLevel::High,
                rename: false,
                wrap_procedures: false,
                flatten_control_flow: true,
                randomize_ids: false,
                scramble_layout: false,
                inject_junk: false,
                protect_vars: Vec::new(),
                preset: None,
                seed: Some(77),
            },
        )
        .expect("flatten");

        let blocks = project["targets"][0]["blocks"]
            .as_object()
            .expect("blocks object");
        let hat_next = project["targets"][0]["blocks"]["hat1"]["next"]
            .as_str()
            .expect("hat next");
        assert_eq!(
            blocks[hat_next]["opcode"],
            Value::String("procedures_call".to_string())
        );
        assert!(
            blocks
                .values()
                .filter(|block| block.get("opcode").and_then(Value::as_str)
                    == Some("procedures_definition"))
                .count()
                >= 2
        );
        assert_ne!(
            project["targets"][0]["blocks"]["stmt1"]["parent"],
            Value::String("hat1".to_string())
        );
    }

    #[test]
    fn junk_uses_original_renamed_variable_names_as_bait() {
        let mut project = sample_project();
        obfuscate_project(
            &mut project,
            &ObfuscationConfig {
                level: ObfuscationLevel::Low,
                rename: true,
                wrap_procedures: false,
                flatten_control_flow: false,
                randomize_ids: false,
                scramble_layout: false,
                inject_junk: true,
                protect_vars: Vec::new(),
                preset: None,
                seed: Some(31),
            },
        )
        .expect("junk");

        let variable_names = project["targets"][0]["variables"]
            .as_object()
            .expect("variables object")
            .values()
            .filter_map(|entry| entry.as_array())
            .filter_map(|arr| arr.first())
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();

        assert!(variable_names.contains(&"coins"));
        assert!(variable_names.contains(&"coins per click"));
    }

    #[test]
    fn inspect_report_suggests_clicker_protection() {
        let report = inspect::inspect_project(&sample_project()).expect("inspect");
        assert!(report
            .suggested_protect
            .iter()
            .any(|name| name.eq_ignore_ascii_case("coins")));
        let rendered = render_inspect_report("sample.sb3", &report);
        assert!(rendered.contains("--protect"));
    }
}
