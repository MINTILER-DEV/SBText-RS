use anyhow::{bail, Result};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct SourceLineOrigin {
    pub file: PathBuf,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct MappedPosition {
    pub file: PathBuf,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone)]
pub struct MergedSource {
    pub source: String,
    pub line_origins: Vec<SourceLineOrigin>,
    entry_file: PathBuf,
}

impl MergedSource {
    pub fn new(source: String, line_origins: Vec<SourceLineOrigin>, entry_file: PathBuf) -> Self {
        Self {
            source,
            line_origins,
            entry_file,
        }
    }

    pub fn entry_file(&self) -> &Path {
        &self.entry_file
    }

    pub fn map_position(&self, merged_line: usize, merged_column: usize) -> MappedPosition {
        if self.line_origins.is_empty() {
            return MappedPosition {
                file: self.entry_file.clone(),
                line: merged_line.max(1),
                column: merged_column.max(1),
            };
        }

        if merged_line > 0 && merged_line <= self.line_origins.len() {
            let origin = &self.line_origins[merged_line - 1];
            return MappedPosition {
                file: origin.file.clone(),
                line: origin.line,
                column: merged_column.max(1),
            };
        }

        let last = self.line_origins.last().expect("line_origins is non-empty");
        let extra = merged_line.saturating_sub(self.line_origins.len());
        MappedPosition {
            file: last.file.clone(),
            line: last.line + extra,
            column: merged_column.max(1),
        }
    }
}

#[derive(Debug, Clone)]
struct ImportSpec {
    sprite_name: String,
    relative_path: String,
    line: usize,
}

#[derive(Debug, Clone, Default)]
struct ParsedFile {
    imports: Vec<ImportSpec>,
    body_lines: Vec<(String, usize)>,
    local_sprites: Vec<String>,
    has_stage: bool,
}

#[derive(Debug, Clone, Default)]
struct ResolvedFile {
    merged_lines: Vec<String>,
    merged_line_origins: Vec<SourceLineOrigin>,
    local_sprites: Vec<String>,
    local_has_stage: bool,
    merged_sprites: Vec<String>,
}

#[allow(dead_code)]
pub fn resolve_merged_source(entry: &Path) -> Result<String> {
    Ok(resolve_merged_source_with_map(entry)?.source)
}

pub fn resolve_merged_source_with_map(entry: &Path) -> Result<MergedSource> {
    let canonical_entry = entry
        .canonicalize()
        .map_err(|_| anyhow::anyhow!("Input file not found: '{}'.", entry.display()))?;
    let mut cache: HashMap<PathBuf, ResolvedFile> = HashMap::new();
    let mut stack: Vec<PathBuf> = Vec::new();
    let resolved = resolve_file(&canonical_entry, &mut stack, &mut cache)?;
    ensure_unique_sprite_names(&resolved.merged_sprites)?;
    let source = if resolved.merged_lines.is_empty() {
        String::new()
    } else {
        let mut out = resolved.merged_lines.join("\n");
        out.push('\n');
        out
    };
    Ok(MergedSource::new(
        source,
        resolved.merged_line_origins,
        canonical_entry,
    ))
}

fn resolve_file(path: &Path, stack: &mut Vec<PathBuf>, cache: &mut HashMap<PathBuf, ResolvedFile>) -> Result<ResolvedFile> {
    let current = path
        .canonicalize()
        .map_err(|_| anyhow::anyhow!("Input file not found: '{}'.", path.display()))?;
    if let Some(cached) = cache.get(path) {
        return Ok(cached.clone());
    }
    if let Some(cached) = cache.get(&current) {
        return Ok(cached.clone());
    }

    if let Some(idx) = stack.iter().position(|p| p == &current) {
        let mut cycle = stack[idx..].to_vec();
        cycle.push(current.clone());
        let rendered = cycle
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(" -> ");
        bail!("Circular import detected: {}", rendered);
    }

    let source = fs::read_to_string(&current)?;
    let parsed = parse_file(&source, &current)?;

    stack.push(current.clone());
    let mut merged_lines: Vec<String> = Vec::new();
    let mut merged_line_origins: Vec<SourceLineOrigin> = Vec::new();
    let mut merged_sprites: Vec<String> = Vec::new();

    for spec in &parsed.imports {
        let imported_path = current
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&spec.relative_path)
            .canonicalize()
            .map_err(|_| {
                anyhow::anyhow!(
                    "Imported file does not exist: '{}' (from '{}', line {}).",
                    spec.relative_path,
                    current.display(),
                    spec.line
                )
            })?;

        let resolved_child = resolve_file(&imported_path, stack, cache)?;
        validate_import_target(
            spec,
            &current,
            &imported_path,
            &resolved_child.local_sprites,
            resolved_child.local_has_stage,
        )?;

        merged_lines.extend(resolved_child.merged_lines.clone());
        merged_line_origins.extend(resolved_child.merged_line_origins.clone());
        merged_sprites.extend(resolved_child.merged_sprites.clone());
    }
    stack.pop();

    for (line_text, line_no) in parsed.body_lines {
        merged_lines.push(line_text);
        merged_line_origins.push(SourceLineOrigin {
            file: current.clone(),
            line: line_no,
        });
    }

    merged_sprites.extend(parsed.local_sprites.clone());

    let resolved = ResolvedFile {
        merged_lines,
        merged_line_origins,
        local_sprites: parsed.local_sprites,
        local_has_stage: parsed.has_stage,
        merged_sprites,
    };
    cache.insert(path.to_path_buf(), resolved.clone());
    cache.insert(current, resolved.clone());
    Ok(resolved)
}

fn parse_file(source: &str, source_path: &Path) -> Result<ParsedFile> {
    let import_re = Regex::new(r#"^\s*import\s+\[(?P<name>[^\]\r\n]+)\]\s+from\s+"(?P<path>[^"\r\n]+)"\s*(?:#.*)?$"#)?;
    let sprite_re = Regex::new(r#"^\s*sprite\s+(?P<name>"[^"]+"|[A-Za-z_][A-Za-z0-9_]*)\s*(?:#.*)?$"#)?;
    let stage_re = Regex::new(r#"^\s*stage(?:\s+("[^"]+"|[A-Za-z_][A-Za-z0-9_]*))?\s*(?:#.*)?$"#)?;

    let mut imports = Vec::new();
    let mut body_lines: Vec<(String, usize)> = Vec::new();
    let mut saw_non_import_code = false;
    let mut local_sprites: Vec<String> = Vec::new();
    let mut has_stage = false;

    for (idx, raw_line) in source.lines().enumerate() {
        let line_no = idx + 1;
        let line = if line_no == 1 {
            raw_line.trim_start_matches('\u{feff}')
        } else {
            raw_line
        };
        if let Some(caps) = import_re.captures(line) {
            if saw_non_import_code {
                bail!(
                    "Imports are only allowed at the top level. Invalid import in '{}' at line {}.",
                    source_path.display(),
                    line_no
                );
            }
            imports.push(ImportSpec {
                sprite_name: caps["name"].trim().to_string(),
                relative_path: caps["path"].trim().to_string(),
                line: line_no,
            });
            continue;
        }

        if !is_blank_or_comment(line) {
            saw_non_import_code = true;
        }
        if let Some(caps) = sprite_re.captures(line) {
            let raw_name = caps["name"].trim();
            local_sprites.push(unquote(raw_name));
        } else if stage_re.is_match(line) {
            has_stage = true;
        }

        body_lines.push((raw_line.to_string(), line_no));
    }

    Ok(ParsedFile {
        imports,
        body_lines,
        local_sprites,
        has_stage,
    })
}

fn validate_import_target(
    spec: &ImportSpec,
    source_path: &Path,
    imported_path: &Path,
    local_sprites: &[String],
    local_has_stage: bool,
) -> Result<()> {
    if local_sprites.is_empty() {
        bail!(
            "Imported file '{}' defines zero sprites; expected exactly one (imported from '{}', line {}).",
            imported_path.display(),
            source_path.display(),
            spec.line
        );
    }
    if local_sprites.len() > 1 {
        bail!(
            "Imported file '{}' defines more than one sprite; expected exactly one (imported from '{}', line {}).",
            imported_path.display(),
            source_path.display(),
            spec.line
        );
    }
    let actual = &local_sprites[0];
    if actual != &spec.sprite_name {
        bail!(
            "Imported sprite name mismatch in '{}', line {}: expected '{}', file defines '{}'.",
            source_path.display(),
            spec.line,
            spec.sprite_name,
            actual
        );
    }
    if local_has_stage {
        bail!(
            "Imported file '{}' must not define a stage (imported from '{}', line {}).",
            imported_path.display(),
            source_path.display(),
            spec.line
        );
    }
    Ok(())
}

fn ensure_unique_sprite_names(sprites: &[String]) -> Result<()> {
    let mut seen = HashSet::new();
    for sprite in sprites {
        let lowered = sprite.to_lowercase();
        if !seen.insert(lowered) {
            bail!("Duplicate sprite name in final project: '{}'.", sprite);
        }
    }
    Ok(())
}

fn is_blank_or_comment(line: &str) -> bool {
    let s = line.trim();
    s.is_empty() || s.starts_with('#')
}

fn unquote(name: &str) -> String {
    if name.len() >= 2 && name.starts_with('"') && name.ends_with('"') {
        name[1..name.len() - 1].to_string()
    } else {
        name.to_string()
    }
}
