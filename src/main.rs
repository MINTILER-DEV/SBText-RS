use anyhow::Result;
use clap::Parser;
use sbtext_rs::cli::Args;

fn main() -> Result<()> {
    let args = Args::parse();
    sbtext_rs::run_cli(&args)
}
