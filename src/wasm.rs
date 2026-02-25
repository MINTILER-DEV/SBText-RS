use wasm_bindgen::prelude::*;

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
