use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct Sb3Archive {
    pub project: Value,
    pub assets: BTreeMap<String, Vec<u8>>,
}

impl Sb3Archive {
    pub fn new(project: Value, assets: BTreeMap<String, Vec<u8>>) -> Self {
        Self { project, assets }
    }
}
