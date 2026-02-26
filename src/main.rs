#[cfg(not(target_arch = "wasm32"))]
use anyhow::Result;
#[cfg(not(target_arch = "wasm32"))]
use clap::Parser;
#[cfg(not(target_arch = "wasm32"))]
use sbtext_rs_core::cli::Args;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    let args = Args::parse();
    sbtext_rs_core::run_cli(&args)
}

#[cfg(target_arch = "wasm32")]
fn main() {}
