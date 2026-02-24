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
use imports::{resolve_merged_source_with_map, MergedSource};
use lexer::Lexer;
use parser::Parser as SbParser;
use semantic::analyze as semantic_analyze;
use std::path::{Path, PathBuf};

fn main() -> Result<()> {
    let args = Args::parse();
    let input = canonicalize_file(&args.input)?;
    let merged = resolve_merged_source_with_map(&input)?;
    let project = parse_and_validate_project(&merged)?;

    if let Some(emit_path) = args.emit_merged {
        std::fs::write(&emit_path, merged.source.as_bytes())?;
    }

    if let Some(output) = args.output {
        if args.python_backend {
            python_backend::compile_with_python(&input, &merged.source, &output, args.no_svg_scale)?;
        } else {
            let options = CodegenOptions {
                scale_svgs: !args.no_svg_scale,
            };
            codegen::write_sb3(&project, &input.parent().unwrap_or(input.as_path()), &output, options)?;
        }
    }

    Ok(())
}

fn parse_and_validate_project(merged: &MergedSource) -> Result<ast::Project> {
    let mut lexer = Lexer::new(&merged.source);
    let tokens = lexer.tokenize().map_err(|e| {
        anyhow::anyhow!(format_source_error(
            "Lex error",
            &e.message,
            e.pos.line,
            e.pos.column,
            merged,
        ))
    })?;
    let mut parser = SbParser::new(tokens);
    let project = parser.parse_project().map_err(|e| {
        anyhow::anyhow!(format_source_error(
            "Parse error",
            &e.message,
            e.pos.line,
            e.pos.column,
            merged,
        ))
    })?;
    semantic_analyze(&project).map_err(|e| anyhow::anyhow!(format_semantic_error(&e.message, merged)))?;
    Ok(project)
}

fn format_source_error(kind: &str, message: &str, line: usize, column: usize, merged: &MergedSource) -> String {
    let mapped = merged.map_position(line, column);
    format!(
        "{}: {} (file '{}', line {}, column {})",
        kind,
        message,
        pretty_path(&mapped.file),
        mapped.line,
        mapped.column
    )
}

fn format_semantic_error(message: &str, merged: &MergedSource) -> String {
    if let Some((line, column)) = extract_line_column(message) {
        let mapped = merged.map_position(line, column);
        return format!(
            "{} (file '{}', mapped line {}, column {})",
            message,
            pretty_path(&mapped.file),
            mapped.line,
            mapped.column
        );
    }
    message.to_string()
}

fn extract_line_column(message: &str) -> Option<(usize, usize)> {
    let line_marker = "line ";
    let col_marker = ", column ";
    let line_start = message.find(line_marker)? + line_marker.len();
    let line_tail = &message[line_start..];
    let line_digits = line_tail
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>();
    if line_digits.is_empty() {
        return None;
    }
    let line = line_digits.parse::<usize>().ok()?;
    let after_line = &line_tail[line_digits.len()..];
    let col_start = after_line.find(col_marker)? + col_marker.len();
    let col_tail = &after_line[col_start..];
    let col_digits = col_tail
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>();
    if col_digits.is_empty() {
        return None;
    }
    let column = col_digits.parse::<usize>().ok()?;
    Some((line, column))
}

fn pretty_path(path: &Path) -> String {
    let raw = path.display().to_string();
    if let Some(stripped) = raw.strip_prefix(r"\\?\") {
        stripped.to_string()
    } else {
        raw
    }
}

fn canonicalize_file(path: &PathBuf) -> Result<PathBuf> {
    if !path.exists() || !path.is_file() {
        return Err(anyhow::anyhow!("Input file not found: '{}'.", path.display()));
    }
    Ok(path.canonicalize()?)
}
