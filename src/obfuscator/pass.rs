use super::context::ObfuscationContext;
use anyhow::Result;
use serde_json::Value;

pub trait ObfuscationPass {
    fn name(&self) -> &'static str;
    fn run(&self, project: &mut Value, ctx: &mut ObfuscationContext) -> Result<()>;
}
