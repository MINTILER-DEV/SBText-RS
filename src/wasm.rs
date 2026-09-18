use wasm_bindgen::prelude::*;

use crate::obfuscator::config::{ObfuscationConfig, ObfuscationLevel, ObfuscationPreset};

#[wasm_bindgen]
pub fn compile_source_to_sb3(source: &str) -> Result<Vec<u8>, JsValue> {
    compile_source_to_sb3_with_options(source, ".", true)
}

#[wasm_bindgen]
pub fn compile_source_to_sb3_with_options(
    source: &str,
    source_dir: &str,
    scale_svgs: bool,
) -> Result<Vec<u8>, JsValue> {
    crate::compile_source_to_sb3_bytes(source, std::path::Path::new(source_dir), scale_svgs)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen]
pub fn compile_sbtc_to_sb3(sbtc_bytes: &[u8]) -> Result<Vec<u8>, JsValue> {
    compile_sbtc_to_sb3_with_options(sbtc_bytes, ".", true)
}

#[wasm_bindgen]
pub fn compile_sbtc_to_sb3_with_options(
    sbtc_bytes: &[u8],
    fallback_source_dir: &str,
    scale_svgs: bool,
) -> Result<Vec<u8>, JsValue> {
    crate::compile_sbtc_bytes_to_sb3_bytes(
        sbtc_bytes,
        std::path::Path::new(fallback_source_dir),
        scale_svgs,
    )
    .map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen]
pub fn obfuscate_sb3_with_options(
    sb3_bytes: &[u8],
    level: &str,
    rename: bool,
    wrap_procedures: bool,
    flatten_control_flow: bool,
    randomize_ids: bool,
    scramble_layout: bool,
    inject_junk: bool,
    protect_csv: &str,
    preset: &str,
    seed: &str,
) -> Result<Vec<u8>, JsValue> {
    let config = ObfuscationConfig {
        level: parse_level(level)?,
        rename,
        wrap_procedures,
        flatten_control_flow,
        randomize_ids,
        scramble_layout,
        inject_junk,
        protect_vars: crate::obfuscator::parse_protect_list(protect_csv),
        preset: parse_preset(preset)?,
        seed: parse_seed(seed)?,
    };
    crate::obfuscator::obfuscate_sb3_bytes(sb3_bytes, config)
        .map(|(bytes, _)| bytes)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

fn parse_level(raw: &str) -> Result<ObfuscationLevel, JsValue> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" | "medium" => Ok(ObfuscationLevel::Medium),
        "low" => Ok(ObfuscationLevel::Low),
        "high" => Ok(ObfuscationLevel::High),
        "insane" => Ok(ObfuscationLevel::Insane),
        other => Err(JsValue::from_str(&format!(
            "Unknown obfuscation level '{}'. Expected low, medium, high, or insane.",
            other
        ))),
    }
}

fn parse_preset(raw: &str) -> Result<Option<ObfuscationPreset>, JsValue> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" | "none" => Ok(None),
        "clicker" => Ok(Some(ObfuscationPreset::Clicker)),
        other => Err(JsValue::from_str(&format!(
            "Unknown obfuscation preset '{}'. Expected none or clicker.",
            other
        ))),
    }
}

fn parse_seed(raw: &str) -> Result<Option<u64>, JsValue> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed.parse::<u64>().map(Some).map_err(|_| {
        JsValue::from_str("Seed must be an unsigned integer if provided.")
    })
}
