use super::names::clicker_like_name;
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Default)]
pub struct InspectReport {
    pub targets: Vec<String>,
    pub variables: Vec<String>,
    pub lists: Vec<String>,
    pub broadcasts: Vec<String>,
    pub custom_blocks: Vec<String>,
    pub monitors: Vec<String>,
    pub suggested_protect: Vec<String>,
}

pub fn inspect_project(project: &Value) -> Result<InspectReport> {
    let targets = project
        .get("targets")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("Invalid project.json: missing 'targets' array."))?;

    let mut report = InspectReport::default();
    let mut variables = BTreeSet::new();
    let mut lists = BTreeSet::new();
    let mut broadcasts = BTreeSet::new();
    let mut procedures = BTreeSet::new();
    let mut suggested = BTreeSet::new();

    for target in targets {
        if let Some(name) = target.get("name").and_then(Value::as_str) {
            report.targets.push(name.to_string());
        }

        if let Some(entries) = target.get("variables").and_then(Value::as_object) {
            for entry in entries.values() {
                if let Some(name) = entry
                    .as_array()
                    .and_then(|arr| arr.first())
                    .and_then(Value::as_str)
                {
                    variables.insert(name.to_string());
                    if clicker_like_name(name) {
                        suggested.insert(name.to_string());
                    }
                }
            }
        }

        if let Some(entries) = target.get("lists").and_then(Value::as_object) {
            for entry in entries.values() {
                if let Some(name) = entry
                    .as_array()
                    .and_then(|arr| arr.first())
                    .and_then(Value::as_str)
                {
                    lists.insert(name.to_string());
                }
            }
        }

        if let Some(entries) = target.get("broadcasts").and_then(Value::as_object) {
            for value in entries.values() {
                if let Some(name) = value.as_str() {
                    broadcasts.insert(name.to_string());
                }
            }
        }

        if let Some(blocks) = target.get("blocks").and_then(Value::as_object) {
            for block in blocks.values() {
                let Some(opcode) = block.get("opcode").and_then(Value::as_str) else {
                    continue;
                };
                if !matches!(opcode, "procedures_prototype" | "procedures_call") {
                    continue;
                }
                if let Some(name) = block
                    .get("mutation")
                    .and_then(Value::as_object)
                    .and_then(|mutation| mutation.get("proccode"))
                    .and_then(Value::as_str)
                {
                    procedures.insert(name.to_string());
                }
            }
        }
    }

    if let Some(monitors) = project.get("monitors").and_then(Value::as_array) {
        for monitor in monitors {
            if let Some(opcode) = monitor.get("opcode").and_then(Value::as_str) {
                report.monitors.push(opcode.to_string());
            } else if let Some(id) = monitor.get("id").and_then(Value::as_str) {
                report.monitors.push(id.to_string());
            }
        }
    }

    report.variables = variables.into_iter().collect();
    report.lists = lists.into_iter().collect();
    report.broadcasts = broadcasts.into_iter().collect();
    report.custom_blocks = procedures.into_iter().collect();
    report.suggested_protect = suggested.into_iter().collect();
    Ok(report)
}

pub fn render_inspect_report(path_label: &str, report: &InspectReport) -> String {
    let mut lines = Vec::new();
    lines.push(format!("Project: {}", path_label));
    lines.push(String::new());
    lines.push("Targets:".to_string());
    push_section(&mut lines, &report.targets);
    lines.push(String::new());
    lines.push("Variables:".to_string());
    push_section(&mut lines, &report.variables);
    lines.push(String::new());
    lines.push("Lists:".to_string());
    push_section(&mut lines, &report.lists);
    lines.push(String::new());
    lines.push("Broadcasts:".to_string());
    push_section(&mut lines, &report.broadcasts);
    lines.push(String::new());
    lines.push("Custom Blocks:".to_string());
    push_section(&mut lines, &report.custom_blocks);
    if !report.monitors.is_empty() {
        lines.push(String::new());
        lines.push("Monitors:".to_string());
        push_section(&mut lines, &report.monitors);
    }
    if !report.suggested_protect.is_empty() {
        lines.push(String::new());
        lines.push("Suggested clicker protection:".to_string());
        lines.push(format!(
            "--protect {}",
            report
                .suggested_protect
                .iter()
                .map(|name| {
                    if name.contains(',') || name.contains(' ') {
                        format!("\"{}\"", name)
                    } else {
                        name.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    lines.join("\n")
}

fn push_section(lines: &mut Vec<String>, entries: &[String]) {
    if entries.is_empty() {
        lines.push("- (none)".to_string());
        return;
    }
    for entry in entries {
        lines.push(format!("- {}", entry));
    }
}
