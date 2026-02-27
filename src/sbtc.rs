use crate::imports::{MergedSource, SourceLineOrigin};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::ZipArchive;

const SBTC_FORMAT: &str = "sbtc";
const SBTC_VERSION: u64 = 1;

pub fn write_sbtc_file(merged: &MergedSource, source_dir: &Path, output_path: &Path) -> Result<()> {
    let bytes = build_sbtc_bytes(merged, source_dir)?;
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output_path, bytes)?;
    Ok(())
}

pub fn build_sbtc_bytes(merged: &MergedSource, source_dir: &Path) -> Result<Vec<u8>> {
    let mut out = Cursor::new(Vec::<u8>::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let manifest = json!({
        "format": SBTC_FORMAT,
        "version": SBTC_VERSION,
        "entry_file": merged.entry_file().to_string_lossy(),
        "source_dir": source_dir.to_string_lossy(),
        "line_count": merged.line_origins.len(),
    });
    let line_map = json!({
        "origins": merged
            .line_origins
            .iter()
            .map(|origin| {
                json!({
                    "file": origin.file.to_string_lossy(),
                    "line": origin.line,
                })
            })
            .collect::<Vec<_>>()
    });

    zip.start_file("manifest.json", opts)?;
    zip.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;

    zip.start_file("merged.sbtext", opts)?;
    zip.write_all(merged.source.as_bytes())?;

    zip.start_file("merged_marked.sbtext", opts)?;
    zip.write_all(build_marked_source(merged).as_bytes())?;

    zip.start_file("line_map.json", opts)?;
    zip.write_all(serde_json::to_string_pretty(&line_map)?.as_bytes())?;

    zip.finish()?;
    Ok(out.into_inner())
}

pub fn read_sbtc_file(path: &Path) -> Result<(MergedSource, Option<PathBuf>)> {
    let bytes = fs::read(path).with_context(|| format!("Failed to read '{}'.", path.display()))?;
    read_sbtc_bytes(&bytes)
}

pub fn read_sbtc_bytes(bytes: &[u8]) -> Result<(MergedSource, Option<PathBuf>)> {
    let mut zip = ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| anyhow!("Input is not a valid .sbtc archive."))?;

    let manifest_text = read_zip_entry_text(&mut zip, "manifest.json")?;
    let merged_source = read_zip_entry_text(&mut zip, "merged.sbtext")?;
    let line_map_text = read_zip_entry_text(&mut zip, "line_map.json")?;

    let manifest: Value = serde_json::from_str(&manifest_text)
        .context("Invalid manifest.json in .sbtc archive.")?;
    let format = manifest
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if format != SBTC_FORMAT {
        bail!("Invalid .sbtc archive format '{}'.", format);
    }
    let version = manifest
        .get("version")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    if version != SBTC_VERSION {
        bail!(
            "Unsupported .sbtc version {} (expected {}).",
            version,
            SBTC_VERSION
        );
    }

    let entry_file = manifest
        .get("entry_file")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("bundle.sbtext"));
    let source_dir = manifest
        .get("source_dir")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from);

    let line_map: Value = serde_json::from_str(&line_map_text)
        .context("Invalid line_map.json in .sbtc archive.")?;
    let origins = line_map
        .get("origins")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("line_map.json is missing 'origins' array."))?;
    let mut line_origins = Vec::with_capacity(origins.len());
    for origin in origins {
        let file = origin
            .get("file")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("line_map origin missing 'file'."))?;
        let line = origin
            .get("line")
            .and_then(Value::as_u64)
            .ok_or_else(|| anyhow!("line_map origin missing 'line'."))?;
        line_origins.push(SourceLineOrigin {
            file: PathBuf::from(file),
            line: line as usize,
        });
    }

    let source_line_count = merged_source.lines().count();
    if source_line_count != line_origins.len() {
        bail!(
            ".sbtc source/map mismatch: merged source has {} lines, line map has {} entries.",
            source_line_count,
            line_origins.len()
        );
    }

    Ok((MergedSource::new(merged_source, line_origins, entry_file), source_dir))
}

fn read_zip_entry_text<R: Read + std::io::Seek>(
    zip: &mut ZipArchive<R>,
    name: &str,
) -> Result<String> {
    let mut entry = zip
        .by_name(name)
        .with_context(|| format!("Missing '{}' in .sbtc archive.", name))?;
    let mut text = String::new();
    entry.read_to_string(&mut text)
        .with_context(|| format!("Failed reading '{}' from .sbtc archive.", name))?;
    Ok(text)
}

fn build_marked_source(merged: &MergedSource) -> String {
    if merged.line_origins.is_empty() {
        return merged.source.clone();
    }
    let mut out = String::new();
    let mut prev_file: Option<&Path> = None;
    let mut prev_line = 0usize;
    for (line_text, origin) in merged.source.lines().zip(merged.line_origins.iter()) {
        let continuous = prev_file
            .map(|f| f == origin.file.as_path() && origin.line == prev_line + 1)
            .unwrap_or(false);
        if !continuous {
            out.push_str(&format!(
                "# @sbtc-origin file=\"{}\" line={}\n",
                escape_marker_text(&origin.file.to_string_lossy()),
                origin.line
            ));
        }
        out.push_str(line_text);
        out.push('\n');
        prev_file = Some(origin.file.as_path());
        prev_line = origin.line;
    }
    out
}

fn escape_marker_text(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}
