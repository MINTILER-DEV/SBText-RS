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
use lexer::{Lexer, TokenType};
use parser::Parser as SbParser;
use semantic::{
    analyze as semantic_analyze, analyze_with_options as semantic_analyze_with_options, SemanticOptions,
    SemanticReport,
};
#[cfg(not(target_arch = "wasm32"))]
use std::io::{self, IsTerminal, Write};
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
        if args.allow_unknown_procedures {
            anyhow::bail!("--allow-unknown-procedures cannot be used with --decompile.");
        }
        let mut progress = CliProgress::new("Decompile");
        progress.emit("Resolving input path", 1, 1);
        let input = canonicalize_file(&args.input)?;
        let result = {
            let mut decomp_stage_cb = |step: usize, total: usize, label: &str| {
                progress.emit(label, step, total);
            };
            decompile::decompile_sb3_with_progress(
                &input,
                args.output.as_deref(),
                args.split_sprites,
                Some(&mut decomp_stage_cb),
            )
        };
        progress.finish();
        return result;
    }

    if args.split_sprites {
        anyhow::bail!("--split-sprites requires --decompile.");
    }
    if args.python_backend && args.allow_unknown_procedures {
        anyhow::bail!(
            "--allow-unknown-procedures is only supported by the native Rust backend (remove --python-backend)."
        );
    }

    let mut progress = CliProgress::new("Compile");
    progress.emit("Resolving input path", 1, 1);
    let input = canonicalize_file(&args.input)?;

    progress.emit("Resolving imports", 1, 1);
    let merged = resolve_merged_source_with_map(&input)?;

    let (project, semantic_report) = {
        let mut analyze_progress_cb = |step: usize, total: usize, label: &str| {
            progress.emit(label, step, total);
        };
        parse_and_validate_project_with_options_with_progress(
            &merged,
            SemanticOptions {
                allow_unknown_procedures: args.allow_unknown_procedures,
            },
            Some(&mut analyze_progress_cb),
        )?
    };
    if args.allow_unknown_procedures {
        progress.finish();
        eprintln!(
            "Warning: --allow-unknown-procedures is enabled. Unknown procedure calls will compile as no-op wait(0) blocks."
        );
        for warning in semantic_report.warnings {
            eprintln!("Warning: {}", warning.message);
        }
    }

    if let Some(emit_path) = &args.emit_merged {
        progress.emit("Writing merged source", 1, 1);
        std::fs::write(emit_path, merged.source.as_bytes())?;
    }

    if let Some(output) = &args.output {
        if args.python_backend {
            progress.emit("Building .sb3 (Python backend)", 1, 1);
            python_backend::compile_with_python(&input, &merged.source, output, args.no_svg_scale)?;
        } else {
            let options = CodegenOptions {
                scale_svgs: !args.no_svg_scale,
                allow_unknown_procedures: args.allow_unknown_procedures,
            };
            let result = {
                let mut codegen_progress_cb = |step: usize, total: usize, label: &str| {
                    progress.emit(label, step, total);
                };
                codegen::write_sb3_with_progress(
                    &project,
                    &input.parent().unwrap_or(input.as_path()),
                    output,
                    options,
                    Some(&mut codegen_progress_cb),
                )
            };
            result?;
        }
    }

    progress.emit("Compile complete", 1, 1);
    progress.finish();
    Ok(())
}

pub fn compile_entry_to_sb3_bytes(input: &Path, scale_svgs: bool) -> Result<Vec<u8>> {
    let input = canonicalize_file(input)?;
    let merged = resolve_merged_source_with_map(&input)?;
    let project = parse_and_validate_project(&merged)?;
    codegen::build_sb3_bytes(
        &project,
        &input.parent().unwrap_or(input.as_path()),
        CodegenOptions {
            scale_svgs,
            allow_unknown_procedures: false,
        },
    )
}

pub fn compile_source_to_sb3_bytes(source: &str, source_dir: &Path, scale_svgs: bool) -> Result<Vec<u8>> {
    let project = parse_and_validate_source(source)?;
    codegen::build_sb3_bytes(
        &project,
        source_dir,
        CodegenOptions {
            scale_svgs,
            allow_unknown_procedures: false,
        },
    )
}

pub fn parse_and_validate_project(merged: &MergedSource) -> Result<ast::Project> {
    let (project, _) = parse_and_validate_project_with_options(merged, SemanticOptions::default())?;
    Ok(project)
}

pub fn parse_and_validate_project_with_options(
    merged: &MergedSource,
    semantic_options: SemanticOptions,
) -> Result<(ast::Project, SemanticReport)> {
    parse_and_validate_project_with_options_with_progress(
        merged,
        semantic_options,
        Option::<&mut fn(usize, usize, &str)>::None,
    )
}

fn parse_and_validate_project_with_options_with_progress<F>(
    merged: &MergedSource,
    semantic_options: SemanticOptions,
    mut progress: Option<&mut F>,
) -> Result<(ast::Project, SemanticReport)>
where
    F: FnMut(usize, usize, &str),
{
    let mut lexer = Lexer::new(&merged.source);
    let mut lex_progress_cb = |percent: usize| {
        report_analysis_progress(
            &mut progress,
            percent,
            100,
            &format!("Lexing {}%", percent),
        );
    };
    let tokens = lexer.tokenize_with_progress(Some(&mut lex_progress_cb)).map_err(|e| {
        anyhow::anyhow!(format_source_error(
            "Lex error",
            &e.message,
            e.pos.line,
            e.pos.column,
            merged,
        ))
    })?;
    emit_parsing_progress_from_tokens(&tokens, &mut progress);
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
    emit_semantic_progress_from_project(&project, &mut progress);
    let semantic_report = semantic_analyze_with_options(&project, semantic_options)
        .map_err(|e| anyhow::anyhow!(format_semantic_error(&e.message, merged)))?;
    Ok((project, semantic_report))
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

fn report_analysis_progress<F>(
    progress: &mut Option<&mut F>,
    step: usize,
    total: usize,
    label: &str,
) where
    F: FnMut(usize, usize, &str),
{
    if let Some(cb) = progress.as_deref_mut() {
        cb(step, total, label);
    }
}

fn emit_parsing_progress_from_tokens<F>(
    tokens: &[lexer::Token],
    progress: &mut Option<&mut F>,
) where
    F: FnMut(usize, usize, &str),
{
    let total_tokens = tokens
        .iter()
        .filter(|t| t.typ != TokenType::Eof)
        .count()
        .max(1);
    let mut done = 0usize;
    let mut last_percent = 0usize;
    for token in tokens {
        if token.typ == TokenType::Eof {
            continue;
        }
        done += 1;
        report_phase_percent_with_counts(
            progress,
            "Parsing",
            done,
            total_tokens,
            "tokens",
            &mut last_percent,
        );
    }
    if done == 0 {
        report_phase_percent_with_counts(
            progress,
            "Parsing",
            1,
            total_tokens,
            "tokens",
            &mut last_percent,
        );
    }
    if last_percent < 100 {
        report_analysis_progress(
            progress,
            total_tokens,
            total_tokens,
            &format!("Parsing 100% ({}/{}) tokens", total_tokens, total_tokens),
        );
    }
}

fn emit_semantic_progress_from_project<F>(project: &ast::Project, progress: &mut Option<&mut F>)
where
    F: FnMut(usize, usize, &str),
{
    let total_checks = count_semantic_statement_checks(project).max(1);
    let mut done = 0usize;
    let mut last_percent = 0usize;
    for target in &project.targets {
        for procedure in &target.procedures {
            walk_semantic_statement_checks(
                &procedure.body,
                &mut done,
                total_checks,
                progress,
                &mut last_percent,
            );
        }
        for script in &target.scripts {
            walk_semantic_statement_checks(
                &script.body,
                &mut done,
                total_checks,
                progress,
                &mut last_percent,
            );
        }
    }
    if done == 0 {
        report_phase_percent_with_counts(
            progress,
            "Semantic checks",
            1,
            total_checks,
            "checks",
            &mut last_percent,
        );
    }
    if last_percent < 100 {
        report_analysis_progress(
            progress,
            total_checks,
            total_checks,
            &format!(
                "Semantic checks 100% ({}/{}) checks",
                total_checks, total_checks
            ),
        );
    }
}

fn count_semantic_statement_checks(project: &ast::Project) -> usize {
    let mut total = 0usize;
    for target in &project.targets {
        for procedure in &target.procedures {
            total += count_statement_checks_recursive(&procedure.body);
        }
        for script in &target.scripts {
            total += count_statement_checks_recursive(&script.body);
        }
    }
    total
}

fn count_statement_checks_recursive(statements: &[ast::Statement]) -> usize {
    let mut total = 0usize;
    for statement in statements {
        total += 1;
        match statement {
            ast::Statement::Repeat { body, .. }
            | ast::Statement::ForEach { body, .. }
            | ast::Statement::While { body, .. }
            | ast::Statement::RepeatUntil { body, .. }
            | ast::Statement::Forever { body, .. } => {
                total += count_statement_checks_recursive(body);
            }
            ast::Statement::If {
                then_body,
                else_body,
                ..
            } => {
                total += count_statement_checks_recursive(then_body);
                total += count_statement_checks_recursive(else_body);
            }
            _ => {}
        }
    }
    total
}

fn walk_semantic_statement_checks<F>(
    statements: &[ast::Statement],
    done: &mut usize,
    total: usize,
    progress: &mut Option<&mut F>,
    last_percent: &mut usize,
) where
    F: FnMut(usize, usize, &str),
{
    for statement in statements {
        *done += 1;
        report_phase_percent_with_counts(
            progress,
            "Semantic checks",
            *done,
            total,
            "checks",
            last_percent,
        );
        match statement {
            ast::Statement::Repeat { body, .. }
            | ast::Statement::ForEach { body, .. }
            | ast::Statement::While { body, .. }
            | ast::Statement::RepeatUntil { body, .. }
            | ast::Statement::Forever { body, .. } => {
                walk_semantic_statement_checks(body, done, total, progress, last_percent);
            }
            ast::Statement::If {
                then_body,
                else_body,
                ..
            } => {
                walk_semantic_statement_checks(then_body, done, total, progress, last_percent);
                walk_semantic_statement_checks(else_body, done, total, progress, last_percent);
            }
            _ => {}
        }
    }
}

fn report_phase_percent_with_counts<F>(
    progress: &mut Option<&mut F>,
    phase: &str,
    done: usize,
    total: usize,
    unit_label: &str,
    last_percent: &mut usize,
) where
    F: FnMut(usize, usize, &str),
{
    let total = total.max(1);
    let done = done.clamp(1, total);
    let percent = ((done * 100) / total).clamp(1, 100);
    if percent <= *last_percent {
        return;
    }
    *last_percent = percent;
    report_analysis_progress(
        progress,
        done,
        total,
        &format!(
            "{} {}% ({}/{}) {}",
            phase, percent, done, total, unit_label
        ),
    );
}

#[cfg(not(target_arch = "wasm32"))]
struct CliProgress {
    prefix: &'static str,
    is_tty: bool,
    rendered_line_len: usize,
    has_rendered: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl CliProgress {
    fn new(prefix: &'static str) -> Self {
        Self {
            prefix,
            is_tty: io::stderr().is_terminal(),
            rendered_line_len: 0,
            has_rendered: false,
        }
    }

    fn emit(&mut self, label: &str, step: usize, total: usize) {
        let total = total.max(1);
        let step = step.clamp(1, total);
        let bar = render_progress_bar(step, total, 14);
        let line = format!(
            "[{}] {}... ({}/{}) {}",
            self.prefix, label, step, total, bar
        );
        if self.is_tty {
            let clear_padding_len = self.rendered_line_len.saturating_sub(line.len());
            eprint!("\r{}{}", line, " ".repeat(clear_padding_len));
            let _ = io::stderr().flush();
            self.rendered_line_len = line.len();
            self.has_rendered = true;
        } else {
            eprintln!("{}", line);
        }
    }

    fn finish(&mut self) {
        if self.is_tty && self.has_rendered {
            eprintln!();
            self.has_rendered = false;
            self.rendered_line_len = 0;
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for CliProgress {
    fn drop(&mut self) {
        self.finish();
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
