use crate::obfuscator::config::{ObfuscationLevel, ObfuscationPreset};
use clap::{Args as ClapArgs, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "sbtext-rs",
    about = "Rust SBText compiler with SB3 decompile, inspect, and obfuscation support.",
    subcommand_negates_reqs = true,
    subcommand_precedence_over_arg = true
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub compile: CompileArgs,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    Obfuscate(ObfuscateArgs),
    Inspect(InspectArgs),
}

#[derive(ClapArgs, Debug, Default)]
pub struct CompileArgs {
    #[arg(value_name = "INPUT")]
    pub input: Option<PathBuf>,

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
        help = "Write merged/compiled SBText bundle (.sbtc) to this path."
    )]
    pub emit_sbtc: Option<PathBuf>,

    #[arg(
        long,
        help = "Treat INPUT as an .sbtc bundle (command alias for .sbtc input mode)."
    )]
    pub compile_sbtc: bool,

    #[arg(
        long,
        value_name = "NAME",
        help = "Sprite name to export when OUTPUT is .sprite3."
    )]
    pub sprite_name: Option<String>,

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

#[derive(ClapArgs, Debug, Clone)]
pub struct ObfuscateArgs {
    #[arg(value_name = "INPUT")]
    pub input: PathBuf,

    #[arg(short, long, value_name = "OUTPUT")]
    pub output: PathBuf,

    #[arg(long, value_enum, default_value_t = ObfuscationLevel::Medium)]
    pub level: ObfuscationLevel,

    #[arg(long, help = "Rename variables, lists, broadcasts, and procedures.")]
    pub rename: bool,

    #[arg(
        long,
        help = "Wrap custom procedure definitions in generated forwarding procedures."
    )]
    pub wrap_procedures: bool,

    #[arg(
        long,
        help = "Flatten control flow by extracting chains and substacks into generated helper procedures."
    )]
    pub flatten: bool,

    #[arg(long = "ids", help = "Randomize Scratch block IDs.")]
    pub ids: bool,

    #[arg(long, help = "Scramble top-level script layout coordinates.")]
    pub layout: bool,

    #[arg(long, help = "Inject bait variables.")]
    pub junk: bool,

    #[arg(
        long,
        value_name = "NAMES",
        help = "Comma-separated variable names to protect."
    )]
    pub protect: Option<String>,

    #[arg(long, value_enum)]
    pub preset: Option<ObfuscationPreset>,

    #[arg(long, value_name = "SEED")]
    pub seed: Option<u64>,
}

#[derive(ClapArgs, Debug, Clone)]
pub struct InspectArgs {
    #[arg(value_name = "INPUT")]
    pub input: PathBuf,
}
