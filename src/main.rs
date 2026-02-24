mod cli;
mod codegen;
mod ast;
mod imports;
mod lexer;
mod parser;
mod python_backend;
mod semantic;

use anyhow::Result;
use clap::Parser;
use codegen::CodegenOptions;
use cli::Args;
use imports::resolve_merged_source;
use lexer::Lexer;
use parser::Parser as SbParser;
use semantic::analyze as semantic_analyze;
use std::path::PathBuf;

fn main() -> Result<()> {
    let args = Args::parse();
    let input = canonicalize_file(&args.input)?;
    let merged = resolve_merged_source(&input)?;
    let project = parse_and_validate_project(&merged)?;

    if let Some(emit_path) = args.emit_merged {
        std::fs::write(&emit_path, merged.as_bytes())?;
    }

    if let Some(output) = args.output {
        if args.python_backend {
            python_backend::compile_with_python(&input, &merged, &output, args.no_svg_scale)?;
        } else {
            let options = CodegenOptions {
                scale_svgs: !args.no_svg_scale,
            };
            codegen::write_sb3(&project, &input.parent().unwrap_or(input.as_path()), &output, options)?;
        }
    }

    Ok(())
}

fn parse_and_validate_project(source: &str) -> Result<ast::Project> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize()?;
    let mut parser = SbParser::new(tokens);
    let project = parser.parse_project()?;
    semantic_analyze(&project)?;
    Ok(project)
}

fn canonicalize_file(path: &PathBuf) -> Result<PathBuf> {
    if !path.exists() || !path.is_file() {
        return Err(anyhow::anyhow!("Input file not found: '{}'.", path.display()));
    }
    Ok(path.canonicalize()?)
}
