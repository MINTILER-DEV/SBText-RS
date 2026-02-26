use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "sbtext-rs",
    about = "Rust SBText compiler (native backend by default, optional Python parity backend)."
)]
pub struct Args {
    #[arg(value_name = "INPUT")]
    pub input: PathBuf,

    #[arg(value_name = "OUTPUT")]
    pub output: Option<PathBuf>,

    #[arg(long, help = "Disable automatic SVG normalization to 64x64.")]
    pub no_svg_scale: bool,

    #[arg(
        long,
        help = "Write merged source after resolving imports to this path."
    )]
    pub emit_merged: Option<PathBuf>,

    #[arg(
        long,
        help = "Use Python backend instead of native Rust backend (parity checks only)."
    )]
    pub python_backend: bool,

    #[arg(long, help = "Decompile .sb3 input into .sbtext source.")]
    pub decompile: bool,

    #[arg(
        long,
        help = "When used with --decompile, writes output as multiple sprite files plus main.sbtext (stage)."
    )]
    pub split_sprites: bool,

    #[arg(
        long,
        help = "Allow unresolved procedure calls. Unknown procedure calls compile as no-op wait(0) blocks."
    )]
    pub allow_unknown_procedures: bool,
}
