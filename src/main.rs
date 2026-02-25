use anyhow::Result;
use clap::Parser;
use sbtext_rs_core::cli::Args;

fn main() -> Result<()> {
    let args = Args::parse();
    sbtext_rs_core::run_cli(&args)
}
