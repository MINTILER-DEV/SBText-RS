pub mod ast;
pub mod codegen;
pub mod imports;
pub mod lexer;
pub mod parser;
pub mod semantic;

#[cfg(not(target_arch = "wasm32"))]
pub mod cli;

#[cfg(not(target_arch = "wasm32"))]
pub mod python_backend;

#[cfg(not(target_arch = "wasm32"))]
pub mod decompile;

use anyhow::Result;
use codegen::CodegenOptions;
use imports::{resolve_merged_source_with_map, MergedSource};
use lexer::Lexer;
use parser::Parser as SbParser;
use semantic::analyze as semantic_analyze;
use std::path::{Path, PathBuf};

#[cfg(all(target_arch = "wasm32", feature = "wasm-bindings"))]
pub mod wasm;

#[cfg(not(target_arch = "wasm32"))]
pub fn run_cli(args: &cli::Args) -> Result<()> {
    if args.decompile {
        if args.python_backend {
            anyhow::bail!("--python-backend cannot be used with --decompile.");
        }
        if args.emit_merged.is_some() {
            anyhow::bail!("--emit-merged cannot be used with --decompile.");
        }
        let progress = CliProgress::new("Decompile", 5);
        progress.emit(1, "Resolving input path");
        let input = canonicalize_file(&args.input)?;
        let mut decomp_stage_cb = |step: usize, total: usize, label: &str| {
            let mapped = 1 + step;
            let expected_total = 1 + total;
            progress.emit_with_total(mapped, expected_total, label);
        };
        return decompile::decompile_sb3_with_progress(
            &input,
            args.output.as_deref(),
            args.split_sprites,
            Some(&mut decomp_stage_cb),
        );
    }

    if args.split_sprites {
        anyhow::bail!("--split-sprites requires --decompile.");
    }

    let total_stages = 3
        + usize::from(args.emit_merged.is_some())
        + usize::from(args.output.is_some());
    let progress = CliProgress::new("Compile", total_stages);
    let mut stage = 0usize;

    stage += 1;
    progress.emit(stage, "Resolving input path");
    let input = canonicalize_file(&args.input)?;

    stage += 1;
    progress.emit(stage, "Resolving imports");
    let merged = resolve_merged_source_with_map(&input)?;

    stage += 1;
    progress.emit(stage, "Lexing, parsing, and semantic checks");
    let project = parse_and_validate_project(&merged)?;

    if let Some(emit_path) = &args.emit_merged {
        stage += 1;
        progress.emit(stage, "Writing merged source");
        std::fs::write(emit_path, merged.source.as_bytes())?;
    }

    if let Some(output) = &args.output {
        stage += 1;
        if args.python_backend {
            progress.emit(stage, "Building .sb3 (Python backend)");
            python_backend::compile_with_python(&input, &merged.source, output, args.no_svg_scale)?;
        } else {
            progress.emit(stage, "Building .sb3 (native backend)");
            let options = CodegenOptions {
                scale_svgs: !args.no_svg_scale,
            };
            codegen::write_sb3(&project, &input.parent().unwrap_or(input.as_path()), output, options)?;
        }
    }

    Ok(())
}

pub fn compile_entry_to_sb3_bytes(input: &Path, scale_svgs: bool) -> Result<Vec<u8>> {
    let input = canonicalize_file(input)?;
    let merged = resolve_merged_source_with_map(&input)?;
    let project = parse_and_validate_project(&merged)?;
    codegen::build_sb3_bytes(
        &project,
        &input.parent().unwrap_or(input.as_path()),
        CodegenOptions { scale_svgs },
    )
}

pub fn compile_source_to_sb3_bytes(source: &str, source_dir: &Path, scale_svgs: bool) -> Result<Vec<u8>> {
    let project = parse_and_validate_source(source)?;
    codegen::build_sb3_bytes(&project, source_dir, CodegenOptions { scale_svgs })
}

pub fn parse_and_validate_project(merged: &MergedSource) -> Result<ast::Project> {
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

pub fn parse_and_validate_source(source: &str) -> Result<ast::Project> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().map_err(|e| {
        anyhow::anyhow!(
            "Lex error: {} (line {}, column {})",
            e.message,
            e.pos.line,
            e.pos.column
        )
    })?;
    let mut parser = SbParser::new(tokens);
    let project = parser.parse_project().map_err(|e| {
        anyhow::anyhow!(
            "Parse error: {} (line {}, column {})",
            e.message,
            e.pos.line,
            e.pos.column
        )
    })?;
    semantic_analyze(&project)?;
    Ok(project)
}

pub fn canonicalize_file(path: &Path) -> Result<PathBuf> {
    if !path.exists() || !path.is_file() {
        return Err(anyhow::anyhow!("Input file not found: '{}'.", path.display()));
    }
    Ok(path.canonicalize()?)
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

#[cfg(not(target_arch = "wasm32"))]
struct CliProgress {
    prefix: &'static str,
    total: usize,
}

#[cfg(not(target_arch = "wasm32"))]
impl CliProgress {
    fn new(prefix: &'static str, total: usize) -> Self {
        Self {
            prefix,
            total: total.max(1),
        }
    }

    fn emit(&self, step: usize, label: &str) {
        self.emit_with_total(step, self.total, label);
    }

    fn emit_with_total(&self, step: usize, total: usize, label: &str) {
        let total = total.max(1);
        let step = step.clamp(1, total);
        let bar = render_progress_bar(step, total, 14);
        eprintln!(
            "[{}] {}... ({}/{}) {}",
            self.prefix, label, step, total, bar
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn render_progress_bar(step: usize, total: usize, width: usize) -> String {
    let width = width.max(1);
    let filled = ((step * width) + (total / 2)) / total;
    let mut s = String::with_capacity(width + 2);
    s.push('[');
    for i in 0..width {
        s.push(if i < filled { '=' } else { '-' });
    }
    s.push(']');
    s
}
